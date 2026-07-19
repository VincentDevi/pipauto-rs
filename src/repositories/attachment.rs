//! Persistence-neutral attachment-metadata repository contract.

use async_trait::async_trait;

use crate::{
    domain::AttachmentId,
    models::attachment::{AttachmentMetadata, AttachmentOwner, NewAttachmentMetadata},
};

use super::RepositoryError;

#[async_trait]
pub trait AttachmentRepository: Send + Sync {
    async fn create(
        &self,
        value: &NewAttachmentMetadata,
    ) -> Result<AttachmentMetadata, RepositoryError>;
    async fn find_by_id(
        &self,
        id: &AttachmentId,
    ) -> Result<Option<AttachmentMetadata>, RepositoryError>;
    async fn update(
        &self,
        id: &AttachmentId,
        value: &NewAttachmentMetadata,
    ) -> Result<AttachmentMetadata, RepositoryError>;
    async fn list_owner(
        &self,
        owner: &AttachmentOwner,
    ) -> Result<Vec<AttachmentMetadata>, RepositoryError>;
    async fn delete(&self, id: &AttachmentId) -> Result<(), RepositoryError>;
}
