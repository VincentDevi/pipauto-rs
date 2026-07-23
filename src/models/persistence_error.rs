//! Private persistence failure classification.

use thiserror::Error;

/// Backend-neutral failures used only inside private model persistence.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PersistenceError {
    #[error("record conflicts with existing data")]
    Conflict,
    #[error("record required by the operation was not found")]
    NotFound,
    #[error("persistence is unavailable")]
    Unavailable,
    #[error("persistence returned corrupt or unexpected data")]
    CorruptData,
}
