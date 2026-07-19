//! Application workflows coordinating domain models and repository contracts.
//!
//! Services may depend on `models`, repository contracts, and application error types. They must
//! not depend on HTTP frameworks, templates, database clients, or concrete persistence adapters.

pub mod auth;

use thiserror::Error;

use crate::{domain::ValidationErrors, repositories::RepositoryError};

/// Persistence-neutral outcomes exposed by business workflows.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum WorkflowError {
    #[error("workflow input is invalid")]
    Validation(ValidationErrors),
    #[error("requested record was not found")]
    NotFound,
    #[error("workflow conflicts with existing state")]
    Conflict,
    #[error("workflow is temporarily unavailable")]
    Unavailable,
    #[error("workflow failed internally")]
    Internal,
}

impl From<RepositoryError> for WorkflowError {
    fn from(value: RepositoryError) -> Self {
        match value {
            RepositoryError::Conflict => Self::Conflict,
            RepositoryError::NotFound => Self::NotFound,
            RepositoryError::Unavailable => Self::Unavailable,
            RepositoryError::CorruptData => Self::Internal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_mapping_keeps_repository_and_service_categories_separate() {
        assert_eq!(
            WorkflowError::from(RepositoryError::Conflict),
            WorkflowError::Conflict
        );
        assert_eq!(
            WorkflowError::from(RepositoryError::NotFound),
            WorkflowError::NotFound
        );
        assert_eq!(
            WorkflowError::from(RepositoryError::Unavailable),
            WorkflowError::Unavailable
        );
        assert_eq!(
            WorkflowError::from(RepositoryError::CorruptData),
            WorkflowError::Internal
        );
    }
}
