//! Authenticated stored-attachment upload, metadata, and content routes.

use axum::{extract::DefaultBodyLimit, http::StatusCode, response::Response, Json};
use loco_rs::{
    controller::{extractor::shared_store::SharedStore, Routes},
    prelude::{delete, get, patch, post, Path},
};
use serde::{Deserialize, Serialize};

use crate::{
    api::{
        ids::{AttachmentIdDto, InterventionIdDto, TechnicalNoteIdDto, VehicleIdDto},
        DataEnvelope, TimestampDto,
    },
    auth::{
        csrf::{AuthenticatedAttachmentMultipart, AuthenticatedCsrfJson},
        extractors::CurrentUser,
    },
    domain::{
        AttachmentId, InterventionId, TechnicalNoteId, ValidationCode, ValidationError,
        ValidationErrors, VehicleId,
    },
    errors::AppError,
    models::attachment::{
        AttachmentModel as AttachmentService, AttachmentOwner, StoredAttachment, UploadAttachment,
        WriteAttachmentMetadata,
    },
    settings::MULTIPART_ENVELOPE_BYTES,
};

const JSON_BODY_LIMIT: usize = 16 * 1_024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateAttachmentRequest {
    display_name: Option<String>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    caption: Option<Option<String>>,
}

#[derive(Serialize)]
struct AttachmentDto {
    id: AttachmentIdDto,
    owner_type: &'static str,
    vehicle_id: Option<VehicleIdDto>,
    intervention_id: Option<InterventionIdDto>,
    technical_note_id: Option<TechnicalNoteIdDto>,
    display_name: String,
    media_type: String,
    byte_size: u64,
    caption: Option<String>,
    storage_state: &'static str,
    created_at: TimestampDto,
    updated_at: TimestampDto,
    content_url: String,
    download_url: String,
}

impl From<StoredAttachment> for AttachmentDto {
    fn from(value: StoredAttachment) -> Self {
        let id = value.id.as_str();
        let content_url = format!("/api/v1/attachments/{id}/content");
        let download_url = format!("/api/v1/attachments/{id}/download");
        let storage_state = value.storage_state();
        let (owner_type, vehicle_id, intervention_id, technical_note_id) = match &value.owner {
            AttachmentOwner::Vehicle(id) => ("vehicle", Some(VehicleIdDto::from(id)), None, None),
            AttachmentOwner::Intervention(id) => (
                "intervention",
                None,
                Some(InterventionIdDto::from(id)),
                None,
            ),
            AttachmentOwner::TechnicalNote(id) => (
                "technical_note",
                None,
                None,
                Some(TechnicalNoteIdDto::from(id)),
            ),
        };
        Self {
            id: AttachmentIdDto::from(&value.id),
            owner_type,
            vehicle_id,
            intervention_id,
            technical_note_id,
            display_name: value.display_name,
            media_type: value.media_type.as_str().to_owned(),
            byte_size: value.byte_size,
            caption: value.caption,
            storage_state,
            created_at: value.created_at.into(),
            updated_at: value.updated_at.into(),
            content_url,
            download_url,
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

async fn technical_note_list(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<AttachmentService>,
    Path(id): Path<String>,
) -> Result<Json<DataEnvelope<Vec<AttachmentDto>>>, AppError> {
    list_owner(
        &service,
        AttachmentOwner::TechnicalNote(parse_technical_note_id(id)?),
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

async fn vehicle_upload(
    SharedStore(service): SharedStore<AttachmentService>,
    Path(id): Path<String>,
    upload: AuthenticatedAttachmentMultipart,
) -> Result<(StatusCode, Json<DataEnvelope<AttachmentDto>>), AppError> {
    upload_owner(
        &service,
        AttachmentOwner::Vehicle(parse_vehicle_id(id)?),
        upload,
    )
    .await
}

async fn intervention_upload(
    SharedStore(service): SharedStore<AttachmentService>,
    Path(id): Path<String>,
    upload: AuthenticatedAttachmentMultipart,
) -> Result<(StatusCode, Json<DataEnvelope<AttachmentDto>>), AppError> {
    upload_owner(
        &service,
        AttachmentOwner::Intervention(parse_intervention_id(id)?),
        upload,
    )
    .await
}

async fn technical_note_upload(
    SharedStore(service): SharedStore<AttachmentService>,
    Path(id): Path<String>,
    upload: AuthenticatedAttachmentMultipart,
) -> Result<(StatusCode, Json<DataEnvelope<AttachmentDto>>), AppError> {
    upload_owner(
        &service,
        AttachmentOwner::TechnicalNote(parse_technical_note_id(id)?),
        upload,
    )
    .await
}

async fn upload_owner(
    service: &AttachmentService,
    owner: AttachmentOwner,
    upload: AuthenticatedAttachmentMultipart,
) -> Result<(StatusCode, Json<DataEnvelope<AttachmentDto>>), AppError> {
    let command = UploadAttachment {
        bytes: upload.bytes,
        display_name: upload.display_name,
        original_filename: upload.original_filename,
        caption: upload.caption,
    };
    Ok((
        StatusCode::CREATED,
        Json(DataEnvelope::new(
            service.upload(owner, command).await?.into(),
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
        media_type: current.media_type.as_str().to_owned(),
        byte_size: Some(current.byte_size),
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

async fn content(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<AttachmentService>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    crate::controllers::shared::downloads::content_response(
        service.content(&parse_id(id)?).await?,
        false,
    )
}

async fn download(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<AttachmentService>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    crate::controllers::shared::downloads::content_response(
        service.content(&parse_id(id)?).await?,
        true,
    )
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

fn parse_technical_note_id(value: String) -> Result<TechnicalNoteId, AppError> {
    TechnicalNoteId::parse(value).map_err(|_| {
        invalid(
            "technical_note_id",
            "Use a valid technical note identifier.",
        )
    })
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
            post(vehicle_upload).layer(DefaultBodyLimit::max(MULTIPART_ENVELOPE_BYTES)),
        )
        .add("/interventions/{id}/attachments", get(intervention_list))
        .add(
            "/interventions/{id}/attachments",
            post(intervention_upload).layer(DefaultBodyLimit::max(MULTIPART_ENVELOPE_BYTES)),
        )
        .add(
            "/technical-notes/{id}/attachments",
            get(technical_note_list),
        )
        .add(
            "/technical-notes/{id}/attachments",
            post(technical_note_upload).layer(DefaultBodyLimit::max(MULTIPART_ENVELOPE_BYTES)),
        )
        .add("/attachments/{id}", get(show))
        .add(
            "/attachments/{id}",
            patch(update).layer(DefaultBodyLimit::max(JSON_BODY_LIMIT)),
        )
        .add(
            "/attachments/{id}",
            delete(remove).layer(DefaultBodyLimit::max(64)),
        )
        .add("/attachments/{id}/content", get(content))
        .add("/attachments/{id}/download", get(download))
}
