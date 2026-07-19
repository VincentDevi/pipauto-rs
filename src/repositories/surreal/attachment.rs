//! SurrealDB attachment-metadata repository adapter.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use surrealdb::{
    engine::any::Any,
    types::{RecordId, SurrealValue},
    Surreal,
};

use crate::{
    domain::{AttachmentId, InterventionId, VehicleId},
    models::attachment::{
        AttachmentMediaType, AttachmentMetadata, AttachmentOwner, NewAttachmentMetadata,
    },
    repositories::{attachment::AttachmentRepository, RepositoryError},
};

use super::support;

const PROJECTION: &str = "id, vehicle, intervention, display_name, media_type, byte_size, caption, storage_state, created_at, updated_at";

#[derive(Clone)]
pub struct SurrealAttachmentRepository {
    client: Surreal<Any>,
}

impl SurrealAttachmentRepository {
    #[must_use]
    pub fn new(client: Surreal<Any>) -> Self {
        Self { client }
    }
}

#[derive(Deserialize, SurrealValue)]
struct DbAttachment {
    id: RecordId,
    vehicle: Option<RecordId>,
    intervention: Option<RecordId>,
    display_name: String,
    media_type: String,
    byte_size: Option<i64>,
    caption: Option<String>,
    storage_state: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<DbAttachment> for AttachmentMetadata {
    type Error = RepositoryError;

    fn try_from(value: DbAttachment) -> Result<Self, Self::Error> {
        if value.storage_state != "metadata_only" {
            return Err(RepositoryError::CorruptData);
        }
        let owner = match (value.vehicle, value.intervention) {
            (Some(id), None) => AttachmentOwner::Vehicle(
                VehicleId::parse(support::record_key(&id, "vehicle")?)
                    .map_err(|_| RepositoryError::CorruptData)?,
            ),
            (None, Some(id)) => AttachmentOwner::Intervention(
                InterventionId::parse(support::record_key(&id, "intervention")?)
                    .map_err(|_| RepositoryError::CorruptData)?,
            ),
            _ => return Err(RepositoryError::CorruptData),
        };
        Ok(Self {
            id: AttachmentId::parse(support::record_key(&value.id, "attachment")?)
                .map_err(|_| RepositoryError::CorruptData)?,
            owner,
            display_name: value.display_name,
            media_type: AttachmentMediaType::parse(&value.media_type)
                .map_err(|_| RepositoryError::CorruptData)?,
            byte_size: value
                .byte_size
                .map(u64::try_from)
                .transpose()
                .map_err(|_| RepositoryError::CorruptData)?,
            caption: value.caption,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

#[async_trait]
impl AttachmentRepository for SurrealAttachmentRepository {
    async fn create(
        &self,
        value: &NewAttachmentMetadata,
    ) -> Result<AttachmentMetadata, RepositoryError> {
        let mut response = write_query(
            &self.client,
            "CREATE attachment SET vehicle = $vehicle, intervention = $intervention, display_name = $display_name, media_type = $media_type, byte_size = $byte_size, caption = $caption, storage_state = 'metadata_only' RETURN AFTER;",
            None,
            value,
        ).await?;
        let row: Option<DbAttachment> = support::take(&mut response, 0)?;
        row.ok_or(RepositoryError::CorruptData)?.try_into()
    }

    async fn find_by_id(
        &self,
        id: &AttachmentId,
    ) -> Result<Option<AttachmentMetadata>, RepositoryError> {
        let query = format!("SELECT {PROJECTION} FROM ONLY $record;");
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind(("record", support::record_id("attachment", id.as_str())?))
                .await,
        )?;
        let row: Option<DbAttachment> = support::take(&mut response, 0)?;
        row.map(TryInto::try_into).transpose()
    }

    async fn update(
        &self,
        id: &AttachmentId,
        value: &NewAttachmentMetadata,
    ) -> Result<AttachmentMetadata, RepositoryError> {
        let mut response = write_query(
            &self.client,
            "UPDATE ONLY $record SET display_name = $display_name, media_type = $media_type, byte_size = $byte_size, caption = $caption WHERE storage_state = 'metadata_only' RETURN AFTER;",
            Some(support::record_id("attachment", id.as_str())?),
            value,
        ).await?;
        let row: Option<DbAttachment> = support::take(&mut response, 0)?;
        row.ok_or(RepositoryError::Conflict)?.try_into()
    }

    async fn list_owner(
        &self,
        owner: &AttachmentOwner,
    ) -> Result<Vec<AttachmentMetadata>, RepositoryError> {
        let (vehicle, intervention) = owner_ids(owner)?;
        let query = format!("SELECT {PROJECTION} FROM attachment WHERE ($vehicle IS NOT NONE AND vehicle = $vehicle) OR ($intervention IS NOT NONE AND intervention = $intervention) ORDER BY created_at DESC, id DESC;");
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind(("vehicle", vehicle))
                .bind(("intervention", intervention))
                .await,
        )?;
        let rows: Vec<DbAttachment> = support::take(&mut response, 0)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn delete(&self, id: &AttachmentId) -> Result<(), RepositoryError> {
        let mut response = support::checked_response(
            self.client
                .query("DELETE ONLY $record WHERE storage_state = 'metadata_only' RETURN BEFORE;")
                .bind(("record", support::record_id("attachment", id.as_str())?))
                .await,
        )?;
        let row: Option<DbAttachment> = support::take(&mut response, 0)?;
        row.ok_or(RepositoryError::NotFound)?;
        Ok(())
    }
}

fn owner_ids(
    owner: &AttachmentOwner,
) -> Result<(Option<RecordId>, Option<RecordId>), RepositoryError> {
    match owner {
        AttachmentOwner::Vehicle(id) => {
            Ok((Some(support::record_id("vehicle", id.as_str())?), None))
        }
        AttachmentOwner::Intervention(id) => {
            Ok((None, Some(support::record_id("intervention", id.as_str())?)))
        }
    }
}

async fn write_query(
    client: &Surreal<Any>,
    query: &str,
    record: Option<RecordId>,
    value: &NewAttachmentMetadata,
) -> Result<surrealdb::IndexedResults, RepositoryError> {
    let (vehicle, intervention) = owner_ids(&value.owner)?;
    support::checked_response(
        client
            .query(query)
            .bind(("record", record))
            .bind(("vehicle", vehicle))
            .bind(("intervention", intervention))
            .bind(("display_name", value.display_name.clone()))
            .bind(("media_type", value.media_type.as_str().to_owned()))
            .bind((
                "byte_size",
                value
                    .byte_size
                    .map(|size| i64::try_from(size).map_err(|_| RepositoryError::CorruptData))
                    .transpose()?,
            ))
            .bind(("caption", value.caption.clone()))
            .await,
    )
}
