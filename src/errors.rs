//! Application error categories and safe mappings to HTTP responses.
//!
//! This module may depend on Axum response types and typed errors from lower layers. It must not
//! expose secrets, raw infrastructure failures, or persistence implementation details to clients.

use std::fmt::Write as _;

use axum::{
    http::{header::CACHE_CONTROL, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use thiserror::Error;

use crate::{api::ErrorEnvelope, services::WorkflowError};

/// Errors that can cross Pipauto's JSON HTTP boundary.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("request syntax is malformed")]
    MalformedRequest,
    #[error("request content type is not supported")]
    UnsupportedMediaType,
    #[error("request multipart content type is not supported")]
    UnsupportedMultipartMediaType,
    #[error("request body is too large")]
    PayloadTooLarge,
    #[error("request validation failed")]
    Validation(crate::domain::ValidationErrors),
    #[error("authentication is required")]
    Unauthenticated,
    #[error("request is forbidden")]
    Forbidden,
    #[error("requested resource was not found")]
    NotFound,
    #[error("request conflicts with existing state")]
    Conflict,
    #[error("service unavailable")]
    Unavailable,
    #[error("internal service failure")]
    Internal,
}

impl From<WorkflowError> for AppError {
    fn from(value: WorkflowError) -> Self {
        match value {
            WorkflowError::Validation(errors) => Self::Validation(errors),
            WorkflowError::NotFound => Self::NotFound,
            WorkflowError::Conflict => Self::Conflict,
            WorkflowError::Unavailable => Self::Unavailable,
            WorkflowError::Internal => Self::Internal,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, body, correlation_id) = match self {
            Self::MalformedRequest => (
                StatusCode::BAD_REQUEST,
                ErrorEnvelope::new("malformed_request", "The request could not be read."),
                None,
            ),
            Self::UnsupportedMediaType => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                ErrorEnvelope::new(
                    "malformed_request",
                    "Content-Type must be application/json.",
                ),
                None,
            ),
            Self::UnsupportedMultipartMediaType => (
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                ErrorEnvelope::new(
                    "malformed_request",
                    "Content-Type must be multipart/form-data.",
                ),
                None,
            ),
            Self::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                ErrorEnvelope::new("malformed_request", "The request body is too large."),
                None,
            ),
            Self::Validation(errors) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                ErrorEnvelope::validation(errors.as_slice()),
                None,
            ),
            Self::Unauthenticated => (
                StatusCode::UNAUTHORIZED,
                ErrorEnvelope::new("unauthenticated", "Authentication is required."),
                None,
            ),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                ErrorEnvelope::new("forbidden", "The request is not allowed."),
                None,
            ),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                ErrorEnvelope::new("not_found", "The requested resource was not found."),
                None,
            ),
            Self::Conflict => (
                StatusCode::CONFLICT,
                ErrorEnvelope::new("conflict", "The request conflicts with existing state."),
                None,
            ),
            Self::Unavailable => correlated_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "database_unavailable",
                "The service is temporarily unavailable.",
                "repository_unavailable",
            ),
            Self::Internal => correlated_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "The request could not be completed.",
                "internal_failure",
            ),
        };

        let mut response = (status, Json(body)).into_response();
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if let Some(correlation_id) = correlation_id {
            if let Ok(value) = HeaderValue::from_str(&correlation_id) {
                response.headers_mut().insert("X-Correlation-ID", value);
            }
        }
        response
    }
}

fn correlated_error(
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    category: &'static str,
) -> (StatusCode, ErrorEnvelope, Option<String>) {
    let correlation_id = correlation_id();
    tracing::error!(
        correlation_id = %correlation_id,
        error_category = category,
        "API request failed"
    );
    (
        status,
        ErrorEnvelope::correlated(code, message, &correlation_id),
        Some(correlation_id),
    )
}

fn correlation_id() -> String {
    let mut random = [0_u8; 12];
    if getrandom::fill(&mut random).is_err() {
        return "api-unavailable".to_owned();
    }
    let mut identifier = String::with_capacity(28);
    identifier.push_str("api-");
    for byte in random {
        let _ = write!(&mut identifier, "{byte:02x}");
    }
    identifier
}

#[cfg(test)]
mod tests {
    use axum::body::{to_bytes, Body};
    use serde_json::Value;

    use super::*;

    #[tokio::test]
    async fn api_foundation_error_mapping_is_stable_and_correlated() {
        for (error, expected_status, expected_code) in [
            (
                AppError::MalformedRequest,
                StatusCode::BAD_REQUEST,
                "malformed_request",
            ),
            (
                AppError::Unauthenticated,
                StatusCode::UNAUTHORIZED,
                "unauthenticated",
            ),
            (AppError::Forbidden, StatusCode::FORBIDDEN, "forbidden"),
            (AppError::NotFound, StatusCode::NOT_FOUND, "not_found"),
            (AppError::Conflict, StatusCode::CONFLICT, "conflict"),
        ] {
            let response = error.into_response();
            assert_eq!(response.status(), expected_status);
            assert_eq!(response_code(response).await, expected_code);
        }

        for (error, expected_status, expected_code) in [
            (
                AppError::Unavailable,
                StatusCode::SERVICE_UNAVAILABLE,
                "database_unavailable",
            ),
            (
                AppError::Internal,
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
            ),
        ] {
            let response = error.into_response();
            assert_eq!(response.status(), expected_status);
            assert!(response.headers().contains_key("X-Correlation-ID"));
            let body = response_json(response).await;
            assert_eq!(body["error"]["code"], expected_code);
            assert!(body["error"]["correlation_id"].is_string());
            assert!(!body.to_string().contains("Surreal"));
        }
    }

    async fn response_code(response: Response) -> String {
        response_json(response).await["error"]["code"]
            .as_str()
            .expect("error code should be a string")
            .to_owned()
    }

    async fn response_json(response: Response<Body>) -> Value {
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        serde_json::from_slice(&body).expect("response should be JSON")
    }
}
