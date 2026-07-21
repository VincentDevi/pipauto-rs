//! SurrealDB adapters for attachment records and the private file bucket.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use surrealdb::{
    engine::any::Any,
    types::{Bytes, File, RecordId, SurrealValue, Value},
    Surreal,
};

use crate::{
    domain::{AttachmentId, InterventionId, TechnicalNoteId, VehicleId},
    models::attachment::{
        AttachmentDigest, AttachmentFilePointer, AttachmentMediaType, AttachmentOwner,
        AttachmentRecord, AttachmentStorageState, NewAttachmentReservation,
        UpdateAttachmentMetadata, ATTACHMENT_BUCKET_NAME,
    },
    repositories::{
        attachment::{
            AttachmentFileHead, AttachmentFilePage, AttachmentFileStore, AttachmentFileStoreError,
            AttachmentRepository,
        },
        RepositoryError,
    },
};

use super::support;

const PROJECTION: &str = "id, vehicle, intervention, technical_note, display_name, media_type, byte_size, caption, file, sha256, storage_state, created_at, updated_at";

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

#[derive(Clone)]
pub struct SurrealAttachmentFileStore {
    client: Surreal<Any>,
}

impl SurrealAttachmentFileStore {
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
    technical_note: Option<RecordId>,
    display_name: String,
    media_type: String,
    byte_size: Option<i64>,
    caption: Option<String>,
    file: Option<File>,
    sha256: Option<String>,
    storage_state: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<DbAttachment> for AttachmentRecord {
    type Error = RepositoryError;

    fn try_from(value: DbAttachment) -> Result<Self, Self::Error> {
        let owner = match (value.vehicle, value.intervention, value.technical_note) {
            (Some(id), None, None) => AttachmentOwner::Vehicle(
                VehicleId::parse(support::record_key(&id, "vehicle")?)
                    .map_err(|_| RepositoryError::CorruptData)?,
            ),
            (None, Some(id), None) => AttachmentOwner::Intervention(
                InterventionId::parse(support::record_key(&id, "intervention")?)
                    .map_err(|_| RepositoryError::CorruptData)?,
            ),
            (None, None, Some(id)) => AttachmentOwner::TechnicalNote(
                TechnicalNoteId::parse(support::record_key(&id, "technical_note")?)
                    .map_err(|_| RepositoryError::CorruptData)?,
            ),
            _ => return Err(RepositoryError::CorruptData),
        };
        let file = value.file.ok_or(RepositoryError::CorruptData)?;
        let pointer = AttachmentFilePointer::new(file.bucket(), file.key())
            .map_err(|_| RepositoryError::CorruptData)?;
        let digest = value
            .sha256
            .map(AttachmentDigest::parse)
            .transpose()
            .map_err(|_| RepositoryError::CorruptData)?;
        let byte_size = value
            .byte_size
            .map(u64::try_from)
            .transpose()
            .map_err(|_| RepositoryError::CorruptData)?;
        AttachmentRecord::new(
            AttachmentId::parse(support::record_key(&value.id, "attachment")?)
                .map_err(|_| RepositoryError::CorruptData)?,
            owner,
            value.display_name,
            AttachmentMediaType::parse(&value.media_type)
                .map_err(|_| RepositoryError::CorruptData)?,
            byte_size,
            value.caption,
            digest,
            pointer,
            AttachmentStorageState::parse(&value.storage_state)
                .map_err(|_| RepositoryError::CorruptData)?,
            value.created_at,
            value.updated_at,
        )
        .map_err(|_| RepositoryError::CorruptData)
    }
}

#[async_trait]
impl AttachmentRepository for SurrealAttachmentRepository {
    async fn reserve(
        &self,
        value: &NewAttachmentReservation,
    ) -> Result<AttachmentRecord, RepositoryError> {
        let (vehicle, intervention, technical_note) = owner_ids(&value.owner)?;
        let mut response = support::checked_response(
            self.client
                .query(
                    "CREATE ONLY $record SET vehicle = $vehicle, intervention = $intervention, technical_note = $technical_note, display_name = $display_name, media_type = $media_type, caption = $caption, file = $file, storage_state = 'pending' RETURN AFTER;",
                )
                .bind(("record", support::record_id("attachment", value.id.as_str())?))
                .bind(("vehicle", vehicle))
                .bind(("intervention", intervention))
                .bind(("technical_note", technical_note))
                .bind(("display_name", value.display_name.clone()))
                .bind(("media_type", value.media_type.as_str().to_owned()))
                .bind(("caption", value.caption.clone()))
                .bind(("file", db_file(&value.file)))
                .await,
        )?;
        take_record(&mut response, 0)
    }

    async fn find_internal(
        &self,
        id: &AttachmentId,
    ) -> Result<Option<AttachmentRecord>, RepositoryError> {
        find(&self.client, id, None).await
    }

    async fn find_stored(
        &self,
        id: &AttachmentId,
    ) -> Result<Option<AttachmentRecord>, RepositoryError> {
        find(&self.client, id, Some(AttachmentStorageState::Stored)).await
    }

    async fn list_stored_owner(
        &self,
        owner: &AttachmentOwner,
    ) -> Result<Vec<AttachmentRecord>, RepositoryError> {
        let (vehicle, intervention, technical_note) = owner_ids(owner)?;
        let query = format!(
            "SELECT {PROJECTION} FROM attachment WHERE storage_state = 'stored' AND (($vehicle IS NOT NONE AND vehicle = $vehicle) OR ($intervention IS NOT NONE AND intervention = $intervention) OR ($technical_note IS NOT NONE AND technical_note = $technical_note)) ORDER BY created_at DESC, id DESC;"
        );
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind(("vehicle", vehicle))
                .bind(("intervention", intervention))
                .bind(("technical_note", technical_note))
                .await,
        )?;
        take_records(&mut response, 0)
    }

    async fn finalize(
        &self,
        id: &AttachmentId,
        byte_size: u64,
        digest: &AttachmentDigest,
    ) -> Result<AttachmentRecord, RepositoryError> {
        let byte_size = i64::try_from(byte_size).map_err(|_| RepositoryError::CorruptData)?;
        let mut response = support::checked_response(
            self.client
                .query("UPDATE ONLY $record SET byte_size = $byte_size, sha256 = $sha256, storage_state = 'stored' WHERE storage_state = 'pending' RETURN AFTER;")
                .bind(("record", support::record_id("attachment", id.as_str())?))
                .bind(("byte_size", byte_size))
                .bind(("sha256", digest.as_str().to_owned()))
                .await,
        )?;
        take_conditional(&mut response, 0)
    }

    async fn update_metadata(
        &self,
        id: &AttachmentId,
        value: &UpdateAttachmentMetadata,
    ) -> Result<AttachmentRecord, RepositoryError> {
        let mut response = support::checked_response(
            self.client
                .query("UPDATE ONLY $record SET display_name = $display_name, caption = $caption WHERE storage_state = 'stored' RETURN AFTER;")
                .bind(("record", support::record_id("attachment", id.as_str())?))
                .bind(("display_name", value.display_name.clone()))
                .bind(("caption", value.caption.clone()))
                .await,
        )?;
        take_conditional(&mut response, 0)
    }

    async fn mark_deleting(&self, id: &AttachmentId) -> Result<AttachmentRecord, RepositoryError> {
        let mut response = support::checked_response(
            self.client
                .query("UPDATE ONLY $record SET storage_state = 'deleting' WHERE storage_state IN ['pending', 'stored', 'deleting'] RETURN AFTER;")
                .bind(("record", support::record_id("attachment", id.as_str())?))
                .await,
        )?;
        take_conditional(&mut response, 0)
    }

    async fn delete_deleting(&self, id: &AttachmentId) -> Result<(), RepositoryError> {
        let mut response = support::checked_response(
            self.client
                .query("DELETE ONLY $record WHERE storage_state = 'deleting' RETURN BEFORE;")
                .bind(("record", support::record_id("attachment", id.as_str())?))
                .await,
        )?;
        let row: Option<DbAttachment> = support::take(&mut response, 0)?;
        row.ok_or(RepositoryError::NotFound)?;
        Ok(())
    }

    async fn list_state(
        &self,
        state: AttachmentStorageState,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<AttachmentRecord>, RepositoryError> {
        let limit = i64::try_from(limit).map_err(|_| RepositoryError::CorruptData)?;
        let offset = i64::try_from(offset).map_err(|_| RepositoryError::CorruptData)?;
        if !(1..=200).contains(&limit) {
            return Err(RepositoryError::CorruptData);
        }
        let query = format!(
            "SELECT {PROJECTION} FROM attachment WHERE storage_state = $state ORDER BY updated_at, id LIMIT $limit START $offset;"
        );
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind(("state", state.as_str().to_owned()))
                .bind(("limit", limit))
                .bind(("offset", offset))
                .await,
        )?;
        take_records(&mut response, 0)
    }
}

#[derive(Deserialize, SurrealValue)]
struct DbFileHead {
    file: File,
    size: i64,
}

#[async_trait]
impl AttachmentFileStore for SurrealAttachmentFileStore {
    async fn put_if_absent(
        &self,
        pointer: &AttachmentFilePointer,
        bytes: &[u8],
    ) -> Result<(), AttachmentFileStoreError> {
        let mut exists_response = checked_file_response(
            self.client
                .query("RETURN file::exists($file);")
                .bind(("file", db_file(pointer)))
                .await,
        )?;
        let exists: Value = exists_response
            .take(0)
            .map_err(|_| AttachmentFileStoreError::CorruptData)?;
        if !matches!(exists, Value::Bool(false)) {
            if !matches!(exists, Value::Bool(true)) {
                return Err(AttachmentFileStoreError::CorruptData);
            }
            return Err(AttachmentFileStoreError::Collision);
        }
        checked_file_response(
            self.client
                .query("RETURN file::put_if_not_exists($file, $bytes);")
                .bind(("file", db_file(pointer)))
                .bind(("bytes", Bytes::from(bytes.to_vec())))
                .await,
        )?;
        Ok(())
    }

    async fn get(
        &self,
        pointer: &AttachmentFilePointer,
    ) -> Result<Vec<u8>, AttachmentFileStoreError> {
        let mut response = checked_file_response(
            self.client
                .query("RETURN file::get($file);")
                .bind(("file", db_file(pointer)))
                .await,
        )?;
        let value: Value = response
            .take(0)
            .map_err(|_| AttachmentFileStoreError::CorruptData)?;
        match value {
            Value::Bytes(bytes) => Ok(bytes.to_vec()),
            Value::None => Err(AttachmentFileStoreError::MissingObject),
            _ => Err(AttachmentFileStoreError::CorruptData),
        }
    }

    async fn head(
        &self,
        pointer: &AttachmentFilePointer,
    ) -> Result<AttachmentFileHead, AttachmentFileStoreError> {
        let mut response = checked_file_response(
            self.client
                .query("RETURN file::head($file);")
                .bind(("file", db_file(pointer)))
                .await,
        )?;
        let head: Option<DbFileHead> = response
            .take(0)
            .map_err(|_| AttachmentFileStoreError::CorruptData)?;
        let head = head.ok_or(AttachmentFileStoreError::MissingObject)?;
        validate_head(pointer, &head)?;
        Ok(AttachmentFileHead {
            byte_size: u64::try_from(head.size)
                .map_err(|_| AttachmentFileStoreError::CorruptData)?,
        })
    }

    async fn delete(
        &self,
        pointer: &AttachmentFilePointer,
    ) -> Result<(), AttachmentFileStoreError> {
        checked_file_response(
            self.client
                .query("RETURN file::delete($file);")
                .bind(("file", db_file(pointer)))
                .await,
        )?;
        Ok(())
    }

    async fn list(
        &self,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<AttachmentFilePage, AttachmentFileStoreError> {
        if !(1..=200).contains(&limit) {
            return Err(AttachmentFileStoreError::CorruptData);
        }
        let limit_i64 = i64::try_from(limit).map_err(|_| AttachmentFileStoreError::CorruptData)?;
        let query = match cursor {
            Some(_) => "RETURN file::list($bucket, { start: $start, limit: $limit });",
            None => "RETURN file::list($bucket, { limit: $limit });",
        };
        let mut request = self
            .client
            .query(query)
            .bind(("bucket", ATTACHMENT_BUCKET_NAME.to_owned()))
            .bind(("limit", limit_i64));
        if let Some(cursor) = cursor {
            request = request.bind(("start", cursor.to_owned()));
        }
        let mut response = checked_file_response(request.await)?;
        let heads: Vec<DbFileHead> = response
            .take(0)
            .map_err(|_| AttachmentFileStoreError::CorruptData)?;
        let mut pointers = Vec::with_capacity(heads.len());
        for head in heads {
            let pointer = AttachmentFilePointer::new(head.file.bucket(), head.file.key())
                .map_err(|_| AttachmentFileStoreError::CorruptData)?;
            validate_head(&pointer, &head)?;
            pointers.push(pointer);
        }
        let next_cursor = (pointers.len() == limit)
            .then(|| pointers.last().map(|pointer| pointer.key().to_owned()))
            .flatten();
        Ok(AttachmentFilePage {
            pointers,
            next_cursor,
        })
    }
}

async fn find(
    client: &Surreal<Any>,
    id: &AttachmentId,
    state: Option<AttachmentStorageState>,
) -> Result<Option<AttachmentRecord>, RepositoryError> {
    let query = format!(
        "SELECT {PROJECTION} FROM ONLY $record WHERE $state IS NONE OR storage_state = $state;"
    );
    let mut response = support::checked_response(
        client
            .query(query)
            .bind(("record", support::record_id("attachment", id.as_str())?))
            .bind(("state", state.map(|state| state.as_str().to_owned())))
            .await,
    )?;
    let row: Option<DbAttachment> = support::take(&mut response, 0)?;
    row.map(TryInto::try_into).transpose()
}

type OwnerIds = (Option<RecordId>, Option<RecordId>, Option<RecordId>);

fn owner_ids(owner: &AttachmentOwner) -> Result<OwnerIds, RepositoryError> {
    match owner {
        AttachmentOwner::Vehicle(id) => Ok((
            Some(support::record_id("vehicle", id.as_str())?),
            None,
            None,
        )),
        AttachmentOwner::Intervention(id) => Ok((
            None,
            Some(support::record_id("intervention", id.as_str())?),
            None,
        )),
        AttachmentOwner::TechnicalNote(id) => Ok((
            None,
            None,
            Some(support::record_id("technical_note", id.as_str())?),
        )),
    }
}

fn db_file(pointer: &AttachmentFilePointer) -> File {
    File::new(pointer.bucket(), pointer.key())
}

fn take_record(
    response: &mut surrealdb::IndexedResults,
    index: usize,
) -> Result<AttachmentRecord, RepositoryError> {
    let row: Option<DbAttachment> = support::take(response, index)?;
    row.ok_or(RepositoryError::CorruptData)?.try_into()
}

fn take_conditional(
    response: &mut surrealdb::IndexedResults,
    index: usize,
) -> Result<AttachmentRecord, RepositoryError> {
    let row: Option<DbAttachment> = support::take(response, index)?;
    row.ok_or(RepositoryError::Conflict)?.try_into()
}

fn take_records(
    response: &mut surrealdb::IndexedResults,
    index: usize,
) -> Result<Vec<AttachmentRecord>, RepositoryError> {
    let rows: Vec<DbAttachment> = support::take(response, index)?;
    rows.into_iter().map(TryInto::try_into).collect()
}

fn checked_file_response(
    response: surrealdb::Result<surrealdb::IndexedResults>,
) -> Result<surrealdb::IndexedResults, AttachmentFileStoreError> {
    let response = response.map_err(|error| classify_file_error(&error.to_string()))?;
    response
        .check()
        .map_err(|error| classify_file_error(&error.to_string()))
}

fn classify_file_error(message: &str) -> AttachmentFileStoreError {
    let message = message.to_ascii_lowercase();
    if message.contains("already exists") || message.contains("already contains") {
        AttachmentFileStoreError::Collision
    } else if message.contains("not found") || message.contains("does not exist") {
        AttachmentFileStoreError::MissingObject
    } else if [
        "bucket",
        "connection",
        "network",
        "socket",
        "timeout",
        "unavailable",
        "closed",
    ]
    .iter()
    .any(|needle| message.contains(needle))
    {
        AttachmentFileStoreError::Unavailable
    } else {
        AttachmentFileStoreError::CorruptData
    }
}

fn validate_head(
    pointer: &AttachmentFilePointer,
    head: &DbFileHead,
) -> Result<(), AttachmentFileStoreError> {
    let decoded = AttachmentFilePointer::new(head.file.bucket(), head.file.key())
        .map_err(|_| AttachmentFileStoreError::CorruptData)?;
    if &decoded != pointer || head.size < 0 {
        return Err(AttachmentFileStoreError::CorruptData);
    }
    Ok(())
}
