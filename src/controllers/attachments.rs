//! JSON-only attachment metadata routes for vehicle and intervention owners.
//!
//! This temporary lifecycle exposes metadata creation, editing, listing, and deletion only. The
//! later storage milestone will replace it when binary storage and upload state are defined.

use axum::{extract::DefaultBodyLimit, http::StatusCode, Json};
use loco_rs::{
    controller::{extractor::shared_store::SharedStore, Routes},
    prelude::{delete, get, patch, post, Path},
};
use serde::{Deserialize, Serialize};

use crate::{
    api::{
        ids::{AttachmentIdDto, InterventionIdDto, VehicleIdDto},
        DataEnvelope, TimestampDto,
    },
    auth::{csrf::AuthenticatedCsrfJson, extractors::CurrentUser},
    domain::{
        AttachmentId, InterventionId, ValidationCode, ValidationError, ValidationErrors, VehicleId,
    },
    errors::AppError,
    models::attachment::{AttachmentMetadata, AttachmentOwner},
    services::attachment::{AttachmentService, WriteAttachmentMetadata},
};

const BODY_LIMIT: usize = 16 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteAttachmentRequest {
    display_name: String,
    media_type: String,
    byte_size: Option<u64>,
    caption: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateAttachmentRequest {
    display_name: Option<String>,
    media_type: Option<String>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    byte_size: Option<Option<u64>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    caption: Option<Option<String>>,
}

impl From<WriteAttachmentRequest> for WriteAttachmentMetadata {
    fn from(value: WriteAttachmentRequest) -> Self {
        Self {
            display_name: value.display_name,
            media_type: value.media_type,
            byte_size: value.byte_size,
            caption: value.caption,
        }
    }
}

#[derive(Serialize)]
struct AttachmentDto {
    id: AttachmentIdDto,
    owner_type: &'static str,
    vehicle_id: Option<VehicleIdDto>,
    intervention_id: Option<InterventionIdDto>,
    display_name: String,
    media_type: String,
    byte_size: Option<u64>,
    caption: Option<String>,
    storage_state: &'static str,
    created_at: TimestampDto,
    updated_at: TimestampDto,
}

impl From<AttachmentMetadata> for AttachmentDto {
    fn from(value: AttachmentMetadata) -> Self {
        let storage_state = value.storage_state();
        let (owner_type, vehicle_id, intervention_id) = match &value.owner {
            AttachmentOwner::Vehicle(id) => ("vehicle", Some(VehicleIdDto::from(id)), None),
            AttachmentOwner::Intervention(id) => {
                ("intervention", None, Some(InterventionIdDto::from(id)))
            }
            AttachmentOwner::TechnicalNote(_) => ("technical_note", None, None),
        };
        Self {
            id: AttachmentIdDto::from(&value.id),
            owner_type,
            vehicle_id,
            intervention_id,
            display_name: value.display_name,
            media_type: value.media_type.as_str().to_owned(),
            byte_size: Some(value.byte_size),
            caption: value.caption,
            storage_state,
            created_at: value.created_at.into(),
            updated_at: value.updated_at.into(),
        }
    }
}

async fn vehicle_list(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<AttachmentService>,
    Path(id): Path<String>,
) -> Result<Json<DataEnvelope<Vec<AttachmentDto>>>, AppError> {
    list_owner(&service, AttachmentOwner::Vehicle(parse_vehicle_id(id)?)).await
}

async fn intervention_list(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<AttachmentService>,
    Path(id): Path<String>,
) -> Result<Json<DataEnvelope<Vec<AttachmentDto>>>, AppError> {
    list_owner(
        &service,
        AttachmentOwner::Intervention(parse_intervention_id(id)?),
    )
    .await
}

async fn list_owner(
    service: &AttachmentService,
    owner: AttachmentOwner,
) -> Result<Json<DataEnvelope<Vec<AttachmentDto>>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service
            .list(&owner)
            .await?
            .into_iter()
            .map(Into::into)
            .collect(),
    )))
}

async fn vehicle_create(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<AttachmentService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<WriteAttachmentRequest>,
) -> Result<(StatusCode, Json<DataEnvelope<AttachmentDto>>), AppError> {
    create_owner(
        &service,
        AttachmentOwner::Vehicle(parse_vehicle_id(id)?),
        request.into(),
    )
    .await
}

async fn intervention_create(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<AttachmentService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<WriteAttachmentRequest>,
) -> Result<(StatusCode, Json<DataEnvelope<AttachmentDto>>), AppError> {
    create_owner(
        &service,
        AttachmentOwner::Intervention(parse_intervention_id(id)?),
        request.into(),
    )
    .await
}

async fn create_owner(
    service: &AttachmentService,
    owner: AttachmentOwner,
    command: WriteAttachmentMetadata,
) -> Result<(StatusCode, Json<DataEnvelope<AttachmentDto>>), AppError> {
    Ok((
        StatusCode::CREATED,
        Json(DataEnvelope::new(
            service.create(owner, command).await?.into(),
        )),
    ))
}

async fn show(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<AttachmentService>,
    Path(id): Path<String>,
) -> Result<Json<DataEnvelope<AttachmentDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service.get(&parse_id(id)?).await?.into(),
    )))
}

async fn update(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<AttachmentService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<UpdateAttachmentRequest>,
) -> Result<Json<DataEnvelope<AttachmentDto>>, AppError> {
    let id = parse_id(id)?;
    let current = service.get(&id).await?;
    let command = WriteAttachmentMetadata {
        display_name: request.display_name.unwrap_or(current.display_name),
        media_type: request
            .media_type
            .unwrap_or_else(|| current.media_type.as_str().to_owned()),
        byte_size: request.byte_size.unwrap_or(Some(current.byte_size)),
        caption: request.caption.unwrap_or(current.caption),
    };
    Ok(Json(DataEnvelope::new(
        service.update(&id, command).await?.into(),
    )))
}

async fn remove(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<AttachmentService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(()): AuthenticatedCsrfJson<()>,
) -> Result<StatusCode, AppError> {
    service.delete(&parse_id(id)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_id(value: String) -> Result<AttachmentId, AppError> {
    AttachmentId::parse(value).map_err(|_| invalid("id", "Use a valid attachment identifier."))
}
fn parse_vehicle_id(value: String) -> Result<VehicleId, AppError> {
    VehicleId::parse(value).map_err(|_| invalid("vehicle_id", "Use a valid vehicle identifier."))
}
fn parse_intervention_id(value: String) -> Result<InterventionId, AppError> {
    InterventionId::parse(value)
        .map_err(|_| invalid("intervention_id", "Use a valid intervention identifier."))
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
        .add("/vehicles/{id}/attachments", get(vehicle_list))
        .add(
            "/vehicles/{id}/attachments",
            post(vehicle_create).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add("/interventions/{id}/attachments", get(intervention_list))
        .add(
            "/interventions/{id}/attachments",
            post(intervention_create).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add("/attachments/{id}", get(show))
        .add(
            "/attachments/{id}",
            patch(update).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add(
            "/attachments/{id}",
            delete(remove).layer(DefaultBodyLimit::max(64)),
        )
}
