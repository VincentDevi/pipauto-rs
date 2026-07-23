//! Stable errors returned by public model operations.

use thiserror::Error;

use crate::domain::ValidationErrors;

/// Persistence-neutral outcomes exposed by model operations.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ModelError {
    #[error("model input is invalid")]
    Validation(ValidationErrors),
    #[error("requested record was not found")]
    NotFound,
    #[error("operation conflicts with existing state")]
    Conflict,
    #[error("operation is temporarily unavailable")]
    Unavailable,
    #[error("operation failed internally")]
    Internal,
}

impl From<crate::models::persistence_error::PersistenceError> for ModelError {
    fn from(value: crate::models::persistence_error::PersistenceError) -> Self {
        match value {
            crate::models::persistence_error::PersistenceError::Conflict => Self::Conflict,
            crate::models::persistence_error::PersistenceError::NotFound => Self::NotFound,
            crate::models::persistence_error::PersistenceError::Unavailable => Self::Unavailable,
            crate::models::persistence_error::PersistenceError::CorruptData => Self::Internal,
        }
    }
}
