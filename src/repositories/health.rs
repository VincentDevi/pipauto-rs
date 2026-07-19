//! Persistence contract for the workshop availability panel.

use async_trait::async_trait;

use super::RepositoryError;

#[async_trait]
pub trait HealthRepository: Send + Sync {
    async fn check(&self) -> Result<(), RepositoryError>;
}
