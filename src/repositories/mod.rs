//! Persistence contracts and their technology-specific adapters.
//!
//! Repository contracts may depend on domain models and persistence-neutral error types. Adapter
//! submodules may depend on their technology, but this boundary must not contain HTTP, templates,
//! or application workflow policy. Contracts accept domain values, typed filters, and opaque
//! cursors rather than HTTP request data.

use thiserror::Error;

pub mod auth;
pub mod surreal;

/// Technology-independent persistence failures.
///
/// Absence is normally represented as `Ok(None)`. `NotFound` is reserved for conditional
/// mutations where the target must exist. Infrastructure and corrupt-data failures remain distinct
/// and must never be collapsed into absence.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RepositoryError {
    #[error("record conflicts with existing data")]
    Conflict,
    #[error("record required by the operation was not found")]
    NotFound,
    #[error("repository is unavailable")]
    Unavailable,
    #[error("repository returned corrupt or unexpected data")]
    CorruptData,
}
