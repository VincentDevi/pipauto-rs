//! Application error categories and safe mappings to HTTP responses.
//!
//! This module may depend on Axum response types and typed errors from lower layers. It must not
//! expose secrets, raw infrastructure failures, or persistence implementation details to clients.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;

/// Errors that can cross Pipauto's HTTP boundary.
#[derive(Debug, Error)]
pub enum AppError {
    /// The submitted request is invalid.
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// An internal operation failed and must not reveal its underlying cause.
    #[error("internal application error")]
    Internal,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, self.to_string()).into_response()
    }
}
