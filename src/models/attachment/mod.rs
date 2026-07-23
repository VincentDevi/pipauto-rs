//! Attachment metadata, storage lifecycle, operations, and private persistence.

mod domain;
mod operations;
pub(crate) mod persistence;
mod reconciliation;
pub(crate) mod repository;

pub(crate) use crate::models::ModelError as WorkflowError;
pub use domain::*;
pub use operations::{
    AttachmentContent, AttachmentIdentitySource, AttachmentModel, OsAttachmentIdentitySource,
    UploadAttachment, WriteAttachmentMetadata,
};
pub use reconciliation::{
    AttachmentReconciliation, AttachmentReconciliationError, AttachmentReconciliationReport,
    ReconciliationMode,
};
pub use repository::{
    AttachmentFileHead, AttachmentFilePage, AttachmentFileStore, AttachmentFileStoreError,
};
