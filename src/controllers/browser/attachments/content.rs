//! Authenticated attachment content and download responses.

use axum::response::Response;
use loco_rs::{controller::extractor::shared_store::SharedStore, prelude::Path};

use crate::{
    auth::extractors::CurrentUser, domain::AttachmentId, errors::AppError,
    models::attachment::AttachmentModel,
};

pub(super) async fn content(
    CurrentUser(_): CurrentUser,
    SharedStore(attachments): SharedStore<AttachmentModel>,
    Path(raw_id): Path<String>,
) -> Result<Response, AppError> {
    attachment_bytes(&attachments, raw_id, false).await
}

pub(super) async fn download(
    CurrentUser(_): CurrentUser,
    SharedStore(attachments): SharedStore<AttachmentModel>,
    Path(raw_id): Path<String>,
) -> Result<Response, AppError> {
    attachment_bytes(&attachments, raw_id, true).await
}

async fn attachment_bytes(
    attachments: &AttachmentModel,
    raw_id: String,
    force_download: bool,
) -> Result<Response, AppError> {
    let id = AttachmentId::parse(raw_id).map_err(|_| AppError::NotFound)?;
    let content = attachments.content(&id).await?;
    crate::controllers::shared::downloads::content_response(content, force_download)
}
