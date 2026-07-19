//! Application error categories and safe mappings to HTTP responses.
//!
//! This module may depend on Axum response types and typed errors from lower layers. It must not
//! expose secrets, raw infrastructure failures, or persistence implementation details to clients.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use thiserror::Error;

use crate::{api::ErrorEnvelope, services::WorkflowError};

/// Errors that can cross Pipauto's HTTP boundary.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("request validation failed")]
    Validation(crate::domain::ValidationErrors),
    #[error("requested resource was not found")]
    NotFound,
    #[error("request conflicts with existing state")]
    Conflict,
    #[error("service unavailable")]
    Unavailable,
}

impl From<WorkflowError> for AppError {
    fn from(value: WorkflowError) -> Self {
        match value {
            WorkflowError::Validation(errors) => Self::Validation(errors),
            WorkflowError::NotFound => Self::NotFound,
            WorkflowError::Conflict => Self::Conflict,
            WorkflowError::Unavailable => Self::Unavailable,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, body) = match self {
            Self::Validation(errors) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                ErrorEnvelope::validation(errors.as_slice()),
            ),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                ErrorEnvelope::new("not_found", "requested resource was not found"),
            ),
            Self::Conflict => (
                StatusCode::CONFLICT,
                ErrorEnvelope::new("conflict", "request conflicts with existing state"),
            ),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorEnvelope::new("unavailable", "service is temporarily unavailable"),
            ),
        };
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_mapping_translates_workflows_only_at_http_boundary() {
        for (workflow, expected) in [
            (WorkflowError::NotFound, StatusCode::NOT_FOUND),
            (WorkflowError::Conflict, StatusCode::CONFLICT),
            (WorkflowError::Unavailable, StatusCode::SERVICE_UNAVAILABLE),
        ] {
            assert_eq!(AppError::from(workflow).into_response().status(), expected);
        }
    }
}
