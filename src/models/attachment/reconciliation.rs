//! Explicit, dry-run-first attachment reconciliation.

use std::{collections::BTreeSet, sync::Arc};

use crate::{
    domain::AttachmentId,
    models::{
        attachment::{
            AttachmentDigest, AttachmentMediaType, AttachmentRecord, AttachmentStorageState,
        },
        persistence_error::PersistenceError as RepositoryError,
    },
};

use super::repository::{AttachmentFileStore, AttachmentFileStoreError, AttachmentRepository};

const PAGE_SIZE: usize = 200;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconciliationMode {
    DryRun,
    Apply,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AttachmentReconciliationReport {
    pub pending: Vec<AttachmentId>,
    pub pending_ready: Vec<AttachmentId>,
    pub pending_incomplete: Vec<AttachmentId>,
    pub pending_missing: Vec<AttachmentId>,
    pub pending_invalid: Vec<AttachmentId>,
    pub deleting: Vec<AttachmentId>,
    pub deleting_object_present: Vec<AttachmentId>,
    pub deleting_object_missing: Vec<AttachmentId>,
    pub stored_missing: Vec<AttachmentId>,
    pub stored_wrong_size: Vec<AttachmentId>,
    pub stored_checksum_mismatch: Vec<AttachmentId>,
    pub orphan_objects: usize,
    pub finalized_pending: usize,
    pub removed_incomplete_pending: usize,
    pub resumed_deleting: usize,
    pub removed_orphans: usize,
}

impl AttachmentReconciliationReport {
    /// Render operator output containing counts and safe attachment identifiers only.
    #[must_use]
    pub fn safe_output(&self, mode: ReconciliationMode) -> String {
        let mut lines = vec![format!(
            "attachment reconciliation mode={}",
            match mode {
                ReconciliationMode::DryRun => "dry-run",
                ReconciliationMode::Apply => "apply",
            }
        )];
        append_ids(&mut lines, "pending", &self.pending);
        append_ids(&mut lines, "pending_ready", &self.pending_ready);
        append_ids(&mut lines, "pending_incomplete", &self.pending_incomplete);
        append_ids(&mut lines, "pending_missing", &self.pending_missing);
        append_ids(&mut lines, "pending_invalid", &self.pending_invalid);
        append_ids(&mut lines, "deleting", &self.deleting);
        append_ids(
            &mut lines,
            "deleting_object_present",
            &self.deleting_object_present,
        );
        append_ids(
            &mut lines,
            "deleting_object_missing",
            &self.deleting_object_missing,
        );
        append_ids(&mut lines, "stored_missing", &self.stored_missing);
        append_ids(&mut lines, "stored_wrong_size", &self.stored_wrong_size);
        append_ids(
            &mut lines,
            "stored_checksum_mismatch",
            &self.stored_checksum_mismatch,
        );
        lines.push(format!("orphan_objects count={}", self.orphan_objects));
        lines.push(format!(
            "finalized_pending count={}",
            self.finalized_pending
        ));
        lines.push(format!(
            "removed_incomplete_pending count={}",
            self.removed_incomplete_pending
        ));
        lines.push(format!("resumed_deleting count={}", self.resumed_deleting));
        lines.push(format!("removed_orphans count={}", self.removed_orphans));
        lines.join("\n")
    }
}

fn append_ids(lines: &mut Vec<String>, label: &str, ids: &[AttachmentId]) {
    let count = ids.len();
    let ids = ids
        .iter()
        .map(AttachmentId::as_str)
        .collect::<Vec<_>>()
        .join(",");
    lines.push(format!("{label} count={} attachment_ids={ids}", count));
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum AttachmentReconciliationError {
    #[error("attachment reconciliation record storage is unavailable")]
    RecordStorageUnavailable,
    #[error("attachment reconciliation bucket is unavailable")]
    BucketUnavailable,
    #[error("attachment reconciliation found invalid persisted data")]
    InvalidPersistedData,
}

#[derive(Clone)]
pub struct AttachmentReconciliation {
    attachments: Arc<dyn AttachmentRepository>,
    files: Arc<dyn AttachmentFileStore>,
}

impl AttachmentReconciliation {
    pub fn from_context(
        context: &crate::models::ModelContext,
    ) -> Result<Self, crate::models::ModelError> {
        Ok(Self::new(context.attachments(), context.attachment_files()))
    }

    #[must_use]
    pub fn new(
        attachments: Arc<dyn AttachmentRepository>,
        files: Arc<dyn AttachmentFileStore>,
    ) -> Self {
        Self { attachments, files }
    }

    /// Scan every attachment state and bucket page, then optionally apply documented repairs.
    ///
    /// # Errors
    ///
    /// Returns a safe error when record storage, the bucket, or persisted data cannot be trusted.
    pub async fn reconcile(
        &self,
        mode: ReconciliationMode,
    ) -> Result<AttachmentReconciliationReport, AttachmentReconciliationError> {
        let pending = self.records(AttachmentStorageState::Pending).await?;
        let stored = self.records(AttachmentStorageState::Stored).await?;
        let deleting = self.records(AttachmentStorageState::Deleting).await?;
        let known_pointers = pending
            .iter()
            .chain(&stored)
            .chain(&deleting)
            .map(|record| record.file.key().to_owned())
            .collect::<BTreeSet<_>>();
        let bucket_objects = self.bucket_objects().await?;
        let orphans = bucket_objects
            .into_iter()
            .filter(|pointer| !known_pointers.contains(pointer.key()))
            .collect::<Vec<_>>();

        let mut report = AttachmentReconciliationReport {
            pending: pending.iter().map(|record| record.id.clone()).collect(),
            deleting: deleting.iter().map(|record| record.id.clone()).collect(),
            orphan_objects: orphans.len(),
            ..AttachmentReconciliationReport::default()
        };

        for record in &pending {
            match self.pending_object_state(record).await? {
                PendingObjectState::Ready => report.pending_ready.push(record.id.clone()),
                PendingObjectState::Missing => {
                    report.pending_missing.push(record.id.clone());
                    report.pending_incomplete.push(record.id.clone());
                }
                PendingObjectState::Invalid => {
                    report.pending_invalid.push(record.id.clone());
                    report.pending_incomplete.push(record.id.clone());
                }
            }
        }
        for record in &deleting {
            match self.files.head(&record.file).await {
                Ok(_) => report.deleting_object_present.push(record.id.clone()),
                Err(AttachmentFileStoreError::MissingObject) => {
                    report.deleting_object_missing.push(record.id.clone());
                }
                Err(error) => return Err(file_error(error)),
            }
        }
        for record in &stored {
            self.inspect_stored(record, &mut report).await?;
        }

        if mode == ReconciliationMode::Apply {
            for record in &pending {
                if report.pending_ready.contains(&record.id) {
                    let bytes = self.files.get(&record.file).await.map_err(file_error)?;
                    let byte_size = u64::try_from(bytes.len())
                        .map_err(|_| AttachmentReconciliationError::InvalidPersistedData)?;
                    let digest = AttachmentDigest::calculate(&bytes);
                    self.attachments
                        .finalize(&record.id, byte_size, &digest)
                        .await
                        .map_err(repository_error)?;
                    report.finalized_pending += 1;
                } else {
                    self.remove_incomplete(record).await?;
                    report.removed_incomplete_pending += 1;
                }
            }
            for record in &deleting {
                self.delete_object_if_present(record).await?;
                self.attachments
                    .delete_deleting(&record.id)
                    .await
                    .map_err(repository_error)?;
                report.resumed_deleting += 1;
            }
            for pointer in &orphans {
                self.files.delete(pointer).await.map_err(file_error)?;
                report.removed_orphans += 1;
            }
        }

        Ok(report)
    }

    async fn records(
        &self,
        state: AttachmentStorageState,
    ) -> Result<Vec<AttachmentRecord>, AttachmentReconciliationError> {
        let mut records = Vec::new();
        loop {
            let page = self
                .attachments
                .list_state(state, records.len(), PAGE_SIZE)
                .await
                .map_err(repository_error)?;
            let done = page.len() < PAGE_SIZE;
            records.extend(page);
            if done {
                return Ok(records);
            }
        }
    }

    async fn bucket_objects(
        &self,
    ) -> Result<Vec<crate::models::attachment::AttachmentFilePointer>, AttachmentReconciliationError>
    {
        let mut pointers = Vec::new();
        let mut cursor = None;
        loop {
            let page = self
                .files
                .list(cursor.as_deref(), PAGE_SIZE)
                .await
                .map_err(file_error)?;
            if page.pointers.is_empty() {
                return Ok(pointers);
            }
            let next_cursor = page.next_cursor;
            pointers.extend(page.pointers);
            match next_cursor {
                Some(next) if cursor.as_deref() != Some(next.as_str()) => cursor = Some(next),
                Some(_) => return Err(AttachmentReconciliationError::InvalidPersistedData),
                None => return Ok(pointers),
            }
        }
    }

    async fn pending_object_state(
        &self,
        record: &AttachmentRecord,
    ) -> Result<PendingObjectState, AttachmentReconciliationError> {
        let head = match self.files.head(&record.file).await {
            Ok(head) => head,
            Err(AttachmentFileStoreError::MissingObject) => {
                return Ok(PendingObjectState::Missing);
            }
            Err(error) => return Err(file_error(error)),
        };
        let bytes = match self.files.get(&record.file).await {
            Ok(bytes) => bytes,
            Err(AttachmentFileStoreError::MissingObject) => {
                return Ok(PendingObjectState::Missing);
            }
            Err(error) => return Err(file_error(error)),
        };
        let byte_size = u64::try_from(bytes.len())
            .map_err(|_| AttachmentReconciliationError::InvalidPersistedData)?;
        if head.byte_size == byte_size
            && AttachmentMediaType::detect(&bytes).is_ok_and(|media| media == record.media_type)
        {
            Ok(PendingObjectState::Ready)
        } else {
            Ok(PendingObjectState::Invalid)
        }
    }

    async fn inspect_stored(
        &self,
        record: &AttachmentRecord,
        report: &mut AttachmentReconciliationReport,
    ) -> Result<(), AttachmentReconciliationError> {
        let head = match self.files.head(&record.file).await {
            Ok(head) => head,
            Err(AttachmentFileStoreError::MissingObject) => {
                report.stored_missing.push(record.id.clone());
                return Ok(());
            }
            Err(error) => return Err(file_error(error)),
        };
        let expected_size = record
            .byte_size
            .ok_or(AttachmentReconciliationError::InvalidPersistedData)?;
        if head.byte_size != expected_size {
            report.stored_wrong_size.push(record.id.clone());
            return Ok(());
        }
        let bytes = self.files.get(&record.file).await.map_err(file_error)?;
        let digest = record
            .digest
            .as_ref()
            .ok_or(AttachmentReconciliationError::InvalidPersistedData)?;
        if AttachmentDigest::calculate(&bytes) != *digest {
            report.stored_checksum_mismatch.push(record.id.clone());
        }
        Ok(())
    }

    async fn remove_incomplete(
        &self,
        record: &AttachmentRecord,
    ) -> Result<(), AttachmentReconciliationError> {
        let deleting = self
            .attachments
            .mark_deleting(&record.id)
            .await
            .map_err(repository_error)?;
        self.delete_object_if_present(&deleting).await?;
        self.attachments
            .delete_deleting(&deleting.id)
            .await
            .map_err(repository_error)
    }

    async fn delete_object_if_present(
        &self,
        record: &AttachmentRecord,
    ) -> Result<(), AttachmentReconciliationError> {
        match self.files.delete(&record.file).await {
            Ok(()) | Err(AttachmentFileStoreError::MissingObject) => Ok(()),
            Err(error) => Err(file_error(error)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingObjectState {
    Ready,
    Missing,
    Invalid,
}

fn repository_error(error: RepositoryError) -> AttachmentReconciliationError {
    match error {
        RepositoryError::Unavailable => AttachmentReconciliationError::RecordStorageUnavailable,
        RepositoryError::NotFound | RepositoryError::Conflict | RepositoryError::CorruptData => {
            AttachmentReconciliationError::InvalidPersistedData
        }
    }
}

fn file_error(error: AttachmentFileStoreError) -> AttachmentReconciliationError {
    match error {
        AttachmentFileStoreError::Unavailable => AttachmentReconciliationError::BucketUnavailable,
        AttachmentFileStoreError::MissingObject
        | AttachmentFileStoreError::Collision
        | AttachmentFileStoreError::CorruptData => {
            AttachmentReconciliationError::InvalidPersistedData
        }
    }
}
