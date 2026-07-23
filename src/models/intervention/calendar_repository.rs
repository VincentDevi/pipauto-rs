//! Private calendar projection persistence contract.

use async_trait::async_trait;

use crate::models::calendar::{CalendarEntry, CalendarRange};

use crate::models::persistence_error::PersistenceError as RepositoryError;

#[async_trait]
pub trait CalendarRepository: Send + Sync {
    /// Return every Draft or Completed entry overlapping the validated range.
    async fn entries(&self, range: &CalendarRange) -> Result<Vec<CalendarEntry>, RepositoryError>;
}
