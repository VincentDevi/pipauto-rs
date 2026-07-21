//! Authenticated technical-note CRUD, search, and archive JSON routes.

use axum::{extract::DefaultBodyLimit, http::StatusCode, Json};
use loco_rs::{
    controller::{extractor::shared_store::SharedStore, Routes},
    prelude::{get, patch, post, Path, Query},
};
use serde::{Deserialize, Serialize};

use crate::{
    api::{
        ids::{InterventionIdDto, TechnicalNoteIdDto, VehicleIdDto},
        DataEnvelope, PaginationEnvelope, TimestampDto,
    },
    auth::{csrf::AuthenticatedCsrfJson, extractors::CurrentUser},
    domain::{
        normalize_search_text, InterventionId, PageRequest, TechnicalNoteId, ValidationCode,
        ValidationError, ValidationErrors, VehicleId,
    },
    errors::AppError,
    models::technical_note::{TechnicalNote, TechnicalNoteContext},
    repositories::{customer::ArchiveFilter, technical_note::TechnicalNoteFilter},
    services::technical_note::{validate_write, TechnicalNoteService},
    settings::BusinessSettings,
};

// Preserve the former global 64 KiB ceiling now that multipart raises the global middleware.
const BODY_LIMIT: usize = 64 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteQuery {
    limit: Option<u16>,
    cursor: Option<String>,
    q: Option<String>,
    tags: Option<String>,
    make: Option<String>,
    model: Option<String>,
    engine: Option<String>,
    archived: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteNoteRequest {
    title: String,
    body: String,
    #[serde(default)]
    tags: Vec<String>,
    vehicle_id: Option<String>,
    source_intervention_id: Option<String>,
    make: Option<String>,
    model: Option<String>,
    engine: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateNoteRequest {
    title: Option<String>,
    body: Option<String>,
    tags: Option<Vec<String>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    vehicle_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    source_intervention_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    make: Option<Option<String>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    model: Option<Option<String>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    engine: Option<Option<String>>,
}

#[derive(Serialize)]
struct ContextDto {
    display: String,
    normalized: String,
}

impl From<TechnicalNoteContext> for ContextDto {
    fn from(value: TechnicalNoteContext) -> Self {
        Self {
            display: value.display,
            normalized: value.normalized,
        }
    }
}

#[derive(Serialize)]
struct TechnicalNoteDto {
    id: TechnicalNoteIdDto,
    title: String,
    body: String,
    tags: Vec<String>,
    vehicle_id: Option<VehicleIdDto>,
    source_intervention_id: Option<InterventionIdDto>,
    make: Option<ContextDto>,
    model: Option<ContextDto>,
    engine: Option<ContextDto>,
    created_at: TimestampDto,
    updated_at: TimestampDto,
    archived_at: Option<TimestampDto>,
}

impl From<TechnicalNote> for TechnicalNoteDto {
    fn from(value: TechnicalNote) -> Self {
        Self {
            id: TechnicalNoteIdDto::from(&value.id),
            title: value.title,
            body: value.body,
            tags: value.tags,
            vehicle_id: value.vehicle_id.as_ref().map(Into::into),
            source_intervention_id: value.source_intervention_id.as_ref().map(Into::into),
            make: value.make.map(Into::into),
            model: value.model.map(Into::into),
            engine: value.engine.map(Into::into),
            created_at: value.created_at.into(),
            updated_at: value.updated_at.into(),
            archived_at: value.archived_at.map(Into::into),
        }
    }
}

async fn list(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<TechnicalNoteService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    Query(query): Query<NoteQuery>,
) -> Result<Json<PaginationEnvelope<TechnicalNoteDto>>, AppError> {
    let pagination = crate::api::PaginationQuery {
        limit: query.limit,
        cursor: query.cursor,
    }
    .resolve(&settings)
    .map_err(AppError::Validation)?;
    let tags = query.tags.map_or_else(Vec::new, |value| {
        value
            .split(',')
            .map(normalize_search_text)
            .filter(|tag| !tag.is_empty())
            .collect()
    });
    let filter = TechnicalNoteFilter {
        query: normalized_optional(query.q),
        tags,
        make: normalized_optional(query.make),
        model: normalized_optional(query.model),
        engine: normalized_optional(query.engine),
        archive: parse_archive(query.archived)?,
    };
    Ok(Json(
        service
            .list(PageRequest {
                filter,
                limit: pagination.limit,
                after: pagination.after,
            })
            .await?
            .into(),
    ))
}

async fn create(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<TechnicalNoteService>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<WriteNoteRequest>,
) -> Result<(StatusCode, Json<DataEnvelope<TechnicalNoteDto>>), AppError> {
    let value = service.create(command(request)?).await?;
    Ok((StatusCode::CREATED, Json(DataEnvelope::new(value.into()))))
}

async fn show(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<TechnicalNoteService>,
    Path(id): Path<String>,
) -> Result<Json<DataEnvelope<TechnicalNoteDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service.get(&parse_id(id)?).await?.into(),
    )))
}

async fn update(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<TechnicalNoteService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<UpdateNoteRequest>,
) -> Result<Json<DataEnvelope<TechnicalNoteDto>>, AppError> {
    let id = parse_id(id)?;
    let current = service.get(&id).await?;
    let value = validate_write(
        request.title.unwrap_or(current.title),
        request.body.unwrap_or(current.body),
        request.tags.unwrap_or(current.tags),
        request.vehicle_id.map_or_else(
            || Ok(current.vehicle_id),
            |id| {
                id.map(VehicleId::parse)
                    .transpose()
                    .map_err(|_| invalid("vehicle_id", "Use a valid vehicle identifier."))
            },
        )?,
        request.source_intervention_id.map_or_else(
            || Ok(current.source_intervention_id),
            |id| {
                id.map(InterventionId::parse).transpose().map_err(|_| {
                    invalid(
                        "source_intervention_id",
                        "Use a valid intervention identifier.",
                    )
                })
            },
        )?,
        request
            .make
            .unwrap_or_else(|| current.make.map(|value| value.display)),
        request
            .model
            .unwrap_or_else(|| current.model.map(|value| value.display)),
        request
            .engine
            .unwrap_or_else(|| current.engine.map(|value| value.display)),
    )?;
    Ok(Json(DataEnvelope::new(
        service.update(&id, value).await?.into(),
    )))
}

async fn archive(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<TechnicalNoteService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(()): AuthenticatedCsrfJson<()>,
) -> Result<Json<DataEnvelope<TechnicalNoteDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service.archive(&parse_id(id)?).await?.into(),
    )))
}

async fn restore(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<TechnicalNoteService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(()): AuthenticatedCsrfJson<()>,
) -> Result<Json<DataEnvelope<TechnicalNoteDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service.restore(&parse_id(id)?).await?.into(),
    )))
}

fn command(
    request: WriteNoteRequest,
) -> Result<crate::models::technical_note::NewTechnicalNote, AppError> {
    validate_write(
        request.title,
        request.body,
        request.tags,
        request
            .vehicle_id
            .map(VehicleId::parse)
            .transpose()
            .map_err(|_| invalid("vehicle_id", "Use a valid vehicle identifier."))?,
        request
            .source_intervention_id
            .map(InterventionId::parse)
            .transpose()
            .map_err(|_| {
                invalid(
                    "source_intervention_id",
                    "Use a valid intervention identifier.",
                )
            })?,
        request.make,
        request.model,
        request.engine,
    )
    .map_err(Into::into)
}

fn normalized_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| normalize_search_text(&value))
        .filter(|value| !value.is_empty())
}

fn parse_archive(value: Option<String>) -> Result<ArchiveFilter, AppError> {
    match value.as_deref().unwrap_or("active") {
        "active" => Ok(ArchiveFilter::Active),
        "archived" => Ok(ArchiveFilter::Archived),
        "all" => Ok(ArchiveFilter::All),
        _ => Err(invalid("archived", "Use active, archived, or all.")),
    }
}

fn parse_id(value: String) -> Result<TechnicalNoteId, AppError> {
    TechnicalNoteId::parse(value)
        .map_err(|_| invalid("id", "Use a valid technical-note identifier."))
}

fn invalid(field: &str, message: &str) -> AppError {
    AppError::Validation(ValidationErrors::one(
        ValidationError::new(field, ValidationCode::InvalidFormat, message)
            .expect("static validation metadata is valid"),
    ))
}

#[must_use]
pub fn routes() -> Routes {
    Routes::new()
        .add("/technical-notes", get(list))
        .add(
            "/technical-notes",
            post(create).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add("/technical-notes/{id}", get(show))
        .add(
            "/technical-notes/{id}",
            patch(update).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add(
            "/technical-notes/{id}/archive",
            post(archive).layer(DefaultBodyLimit::max(64)),
        )
        .add(
            "/technical-notes/{id}/restore",
            post(restore).layer(DefaultBodyLimit::max(64)),
        )
}
