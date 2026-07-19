//! Workshop availability workflow.

use std::sync::Arc;

use crate::repositories::health::HealthRepository;

#[derive(Clone)]
pub struct HealthService {
    repository: Arc<dyn HealthRepository>,
}

impl HealthService {
    #[must_use]
    pub const fn new(repository: Arc<dyn HealthRepository>) -> Self {
        Self { repository }
    }

    pub async fn available(&self) -> bool {
        self.repository.check().await.is_ok()
    }
}
