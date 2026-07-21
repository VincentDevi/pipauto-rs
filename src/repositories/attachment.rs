//! Persistence-neutral attachment record and bucket contracts.

use async_trait::async_trait;

use crate::{
    domain::AttachmentId,
    models::attachment::{
        AttachmentDigest, AttachmentFilePointer, AttachmentOwner, AttachmentRecord,
        AttachmentStorageState, NewAttachmentReservation, UpdateAttachmentMetadata,
        ATTACHMENT_BUCKET_NAME,
    },
};

use super::RepositoryError;

#[async_trait]
pub trait AttachmentRepository: Send + Sync {
    async fn reserve(
        &self,
        value: &NewAttachmentReservation,
    ) -> Result<AttachmentRecord, RepositoryError>;
    async fn find_internal(
        &self,
        id: &AttachmentId,
    ) -> Result<Option<AttachmentRecord>, RepositoryError>;
    async fn find_stored(
        &self,
        id: &AttachmentId,
    ) -> Result<Option<AttachmentRecord>, RepositoryError>;
    async fn list_stored_owner(
        &self,
        owner: &AttachmentOwner,
    ) -> Result<Vec<AttachmentRecord>, RepositoryError>;
    async fn finalize(
        &self,
        id: &AttachmentId,
        byte_size: u64,
        digest: &AttachmentDigest,
    ) -> Result<AttachmentRecord, RepositoryError>;
    async fn update_metadata(
        &self,
        id: &AttachmentId,
        value: &UpdateAttachmentMetadata,
    ) -> Result<AttachmentRecord, RepositoryError>;
    async fn mark_deleting(&self, id: &AttachmentId) -> Result<AttachmentRecord, RepositoryError>;
    async fn delete_deleting(&self, id: &AttachmentId) -> Result<(), RepositoryError>;
    async fn list_state(
        &self,
        state: AttachmentStorageState,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<AttachmentRecord>, RepositoryError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AttachmentFileStoreError {
    #[error("attachment bucket is unavailable")]
    Unavailable,
    #[error("attachment object does not exist")]
    MissingObject,
    #[error("attachment object key already exists")]
    Collision,
    #[error("attachment bucket returned corrupt data")]
    CorruptData,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentFileHead {
    pub byte_size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentFilePage {
    pub pointers: Vec<AttachmentFilePointer>,
    pub next_cursor: Option<String>,
}

#[async_trait]
pub trait AttachmentFileStore: Send + Sync {
    async fn put_if_absent(
        &self,
        pointer: &AttachmentFilePointer,
        bytes: &[u8],
    ) -> Result<(), AttachmentFileStoreError>;
    async fn get(
        &self,
        pointer: &AttachmentFilePointer,
    ) -> Result<Vec<u8>, AttachmentFileStoreError>;
    async fn head(
        &self,
        pointer: &AttachmentFilePointer,
    ) -> Result<AttachmentFileHead, AttachmentFileStoreError>;
    async fn delete(&self, pointer: &AttachmentFilePointer)
        -> Result<(), AttachmentFileStoreError>;
    async fn list(
        &self,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<AttachmentFilePage, AttachmentFileStoreError>;
}

/// Deterministic in-memory adapters used by lifecycle and failure-injection tests.
pub mod memory {
    use std::{
        collections::{BTreeMap, VecDeque},
        sync::Mutex,
    };

    use chrono::Utc;

    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum RepositoryOperation {
        Reserve,
        Find,
        List,
        Finalize,
        Update,
        MarkDeleting,
        Delete,
    }

    #[derive(Default)]
    pub struct InMemoryAttachmentRepository {
        records: Mutex<BTreeMap<String, AttachmentRecord>>,
        failures: Mutex<VecDeque<(RepositoryOperation, RepositoryError)>>,
    }

    impl InMemoryAttachmentRepository {
        pub fn fail_next(&self, operation: RepositoryOperation, error: RepositoryError) {
            if let Ok(mut failures) = self.failures.lock() {
                failures.push_back((operation, error));
            }
        }

        pub fn snapshot(&self) -> Result<Vec<AttachmentRecord>, RepositoryError> {
            self.records
                .lock()
                .map(|records| records.values().cloned().collect())
                .map_err(|_| RepositoryError::CorruptData)
        }

        fn failure(&self, operation: RepositoryOperation) -> Result<(), RepositoryError> {
            let mut failures = self
                .failures
                .lock()
                .map_err(|_| RepositoryError::CorruptData)?;
            if let Some(index) = failures.iter().position(|(queued, _)| *queued == operation) {
                let (_, error) = failures.remove(index).ok_or(RepositoryError::CorruptData)?;
                return Err(error);
            }
            Ok(())
        }
    }

    #[async_trait]
    impl AttachmentRepository for InMemoryAttachmentRepository {
        async fn reserve(
            &self,
            value: &NewAttachmentReservation,
        ) -> Result<AttachmentRecord, RepositoryError> {
            self.failure(RepositoryOperation::Reserve)?;
            let mut records = self
                .records
                .lock()
                .map_err(|_| RepositoryError::CorruptData)?;
            if records.contains_key(value.id.as_str()) {
                return Err(RepositoryError::Conflict);
            }
            let now = Utc::now();
            let record = AttachmentRecord::new(
                value.id.clone(),
                value.owner.clone(),
                value.display_name.clone(),
                value.media_type,
                None,
                value.caption.clone(),
                None,
                value.file.clone(),
                AttachmentStorageState::Pending,
                now,
                now,
            )
            .map_err(|_| RepositoryError::CorruptData)?;
            records.insert(value.id.as_str().to_owned(), record.clone());
            Ok(record)
        }

        async fn find_internal(
            &self,
            id: &AttachmentId,
        ) -> Result<Option<AttachmentRecord>, RepositoryError> {
            self.failure(RepositoryOperation::Find)?;
            self.records
                .lock()
                .map(|records| records.get(id.as_str()).cloned())
                .map_err(|_| RepositoryError::CorruptData)
        }

        async fn find_stored(
            &self,
            id: &AttachmentId,
        ) -> Result<Option<AttachmentRecord>, RepositoryError> {
            Ok(self
                .find_internal(id)
                .await?
                .filter(|record| record.storage_state == AttachmentStorageState::Stored))
        }

        async fn list_stored_owner(
            &self,
            owner: &AttachmentOwner,
        ) -> Result<Vec<AttachmentRecord>, RepositoryError> {
            self.failure(RepositoryOperation::List)?;
            let mut values: Vec<_> = self
                .records
                .lock()
                .map_err(|_| RepositoryError::CorruptData)?
                .values()
                .filter(|record| {
                    record.storage_state == AttachmentStorageState::Stored && &record.owner == owner
                })
                .cloned()
                .collect();
            values.sort_by(|left, right| {
                right
                    .created_at
                    .cmp(&left.created_at)
                    .then_with(|| right.id.cmp(&left.id))
            });
            Ok(values)
        }

        async fn finalize(
            &self,
            id: &AttachmentId,
            byte_size: u64,
            digest: &AttachmentDigest,
        ) -> Result<AttachmentRecord, RepositoryError> {
            self.failure(RepositoryOperation::Finalize)?;
            let mut records = self
                .records
                .lock()
                .map_err(|_| RepositoryError::CorruptData)?;
            let record = records
                .get_mut(id.as_str())
                .ok_or(RepositoryError::NotFound)?;
            if record.storage_state != AttachmentStorageState::Pending {
                return Err(RepositoryError::Conflict);
            }
            record.byte_size = Some(byte_size);
            record.digest = Some(digest.clone());
            record.storage_state = AttachmentStorageState::Stored;
            record.updated_at = Utc::now();
            Ok(record.clone())
        }

        async fn update_metadata(
            &self,
            id: &AttachmentId,
            value: &UpdateAttachmentMetadata,
        ) -> Result<AttachmentRecord, RepositoryError> {
            self.failure(RepositoryOperation::Update)?;
            let mut records = self
                .records
                .lock()
                .map_err(|_| RepositoryError::CorruptData)?;
            let record = records
                .get_mut(id.as_str())
                .ok_or(RepositoryError::NotFound)?;
            if record.storage_state != AttachmentStorageState::Stored {
                return Err(RepositoryError::Conflict);
            }
            record.display_name.clone_from(&value.display_name);
            record.caption.clone_from(&value.caption);
            record.updated_at = Utc::now();
            Ok(record.clone())
        }

        async fn mark_deleting(
            &self,
            id: &AttachmentId,
        ) -> Result<AttachmentRecord, RepositoryError> {
            self.failure(RepositoryOperation::MarkDeleting)?;
            let mut records = self
                .records
                .lock()
                .map_err(|_| RepositoryError::CorruptData)?;
            let record = records
                .get_mut(id.as_str())
                .ok_or(RepositoryError::NotFound)?;
            record.storage_state = AttachmentStorageState::Deleting;
            record.updated_at = Utc::now();
            Ok(record.clone())
        }

        async fn delete_deleting(&self, id: &AttachmentId) -> Result<(), RepositoryError> {
            self.failure(RepositoryOperation::Delete)?;
            let mut records = self
                .records
                .lock()
                .map_err(|_| RepositoryError::CorruptData)?;
            if records
                .get(id.as_str())
                .is_none_or(|record| record.storage_state != AttachmentStorageState::Deleting)
            {
                return Err(RepositoryError::NotFound);
            }
            records.remove(id.as_str());
            Ok(())
        }

        async fn list_state(
            &self,
            state: AttachmentStorageState,
            offset: usize,
            limit: usize,
        ) -> Result<Vec<AttachmentRecord>, RepositoryError> {
            self.failure(RepositoryOperation::List)?;
            if !(1..=200).contains(&limit) {
                return Err(RepositoryError::CorruptData);
            }
            Ok(self
                .records
                .lock()
                .map_err(|_| RepositoryError::CorruptData)?
                .values()
                .filter(|record| record.storage_state == state)
                .skip(offset)
                .take(limit)
                .cloned()
                .collect())
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum FileOperation {
        Put,
        Get,
        Head,
        Delete,
        List,
    }

    #[derive(Default)]
    pub struct InMemoryAttachmentFileStore {
        objects: Mutex<BTreeMap<String, Vec<u8>>>,
        failures: Mutex<VecDeque<(FileOperation, AttachmentFileStoreError)>>,
    }

    impl InMemoryAttachmentFileStore {
        pub fn fail_next(&self, operation: FileOperation, error: AttachmentFileStoreError) {
            if let Ok(mut failures) = self.failures.lock() {
                failures.push_back((operation, error));
            }
        }

        pub fn contains(&self, pointer: &AttachmentFilePointer) -> bool {
            self.objects
                .lock()
                .is_ok_and(|objects| objects.contains_key(pointer.key()))
        }

        fn failure(&self, operation: FileOperation) -> Result<(), AttachmentFileStoreError> {
            let mut failures = self
                .failures
                .lock()
                .map_err(|_| AttachmentFileStoreError::CorruptData)?;
            if let Some(index) = failures.iter().position(|(queued, _)| *queued == operation) {
                let (_, error) = failures
                    .remove(index)
                    .ok_or(AttachmentFileStoreError::CorruptData)?;
                return Err(error);
            }
            Ok(())
        }
    }

    #[async_trait]
    impl AttachmentFileStore for InMemoryAttachmentFileStore {
        async fn put_if_absent(
            &self,
            pointer: &AttachmentFilePointer,
            bytes: &[u8],
        ) -> Result<(), AttachmentFileStoreError> {
            self.failure(FileOperation::Put)?;
            let mut objects = self
                .objects
                .lock()
                .map_err(|_| AttachmentFileStoreError::CorruptData)?;
            if objects.contains_key(pointer.key()) {
                return Err(AttachmentFileStoreError::Collision);
            }
            objects.insert(pointer.key().to_owned(), bytes.to_vec());
            Ok(())
        }

        async fn get(
            &self,
            pointer: &AttachmentFilePointer,
        ) -> Result<Vec<u8>, AttachmentFileStoreError> {
            self.failure(FileOperation::Get)?;
            self.objects
                .lock()
                .map_err(|_| AttachmentFileStoreError::CorruptData)?
                .get(pointer.key())
                .cloned()
                .ok_or(AttachmentFileStoreError::MissingObject)
        }

        async fn head(
            &self,
            pointer: &AttachmentFilePointer,
        ) -> Result<AttachmentFileHead, AttachmentFileStoreError> {
            self.failure(FileOperation::Head)?;
            let size = self
                .objects
                .lock()
                .map_err(|_| AttachmentFileStoreError::CorruptData)?
                .get(pointer.key())
                .map(Vec::len)
                .ok_or(AttachmentFileStoreError::MissingObject)?;
            Ok(AttachmentFileHead {
                byte_size: u64::try_from(size)
                    .map_err(|_| AttachmentFileStoreError::CorruptData)?,
            })
        }

        async fn delete(
            &self,
            pointer: &AttachmentFilePointer,
        ) -> Result<(), AttachmentFileStoreError> {
            self.failure(FileOperation::Delete)?;
            self.objects
                .lock()
                .map_err(|_| AttachmentFileStoreError::CorruptData)?
                .remove(pointer.key());
            Ok(())
        }

        async fn list(
            &self,
            cursor: Option<&str>,
            limit: usize,
        ) -> Result<AttachmentFilePage, AttachmentFileStoreError> {
            self.failure(FileOperation::List)?;
            if !(1..=200).contains(&limit) {
                return Err(AttachmentFileStoreError::CorruptData);
            }
            let objects = self
                .objects
                .lock()
                .map_err(|_| AttachmentFileStoreError::CorruptData)?;
            let pointers = objects
                .keys()
                .filter(|key| cursor.is_none_or(|cursor| key.as_str() > cursor))
                .take(limit)
                .map(|key| AttachmentFilePointer::new(ATTACHMENT_BUCKET_NAME, key.clone()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| AttachmentFileStoreError::CorruptData)?;
            let next_cursor = (pointers.len() == limit)
                .then(|| pointers.last().map(|pointer| pointer.key().to_owned()))
                .flatten();
            Ok(AttachmentFilePage {
                pointers,
                next_cursor,
            })
        }
    }
}
