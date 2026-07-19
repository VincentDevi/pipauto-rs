//! SurrealDB availability adapter.

use async_trait::async_trait;

use crate::{
    database::client::AppDatabase,
    repositories::{health::HealthRepository, RepositoryError},
};

#[derive(Clone)]
pub struct SurrealHealthRepository {
    database: AppDatabase,
}

impl SurrealHealthRepository {
    #[must_use]
    pub const fn new(database: AppDatabase) -> Self {
        Self { database }
    }
}

#[async_trait]
impl HealthRepository for SurrealHealthRepository {
    async fn check(&self) -> Result<(), RepositoryError> {
        self.database
            .health()
            .await
            .map_err(|_| RepositoryError::Unavailable)
    }
}
