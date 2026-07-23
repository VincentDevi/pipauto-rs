//! Public health response for the application-managed `SurrealDB` client.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use loco_rs::{controller::extractor::shared_store::SharedStore, controller::Routes, prelude::get};
use serde::Serialize;

use crate::database::client::AppDatabase;

const HEALTHY: &str = "healthy";
const UNAVAILABLE: &str = "unavailable";

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    attachment_bucket: &'static str,
}

async fn health(SharedStore(database): SharedStore<AppDatabase>) -> Response {
    match database.health().await {
        Ok(()) => {
            let attachment_bucket = database
                .attachment_bucket_status()
                .await
                .map_or("unavailable", |status| status.as_str());
            (
                StatusCode::OK,
                Json(HealthResponse {
                    status: HEALTHY,
                    attachment_bucket,
                }),
            )
                .into_response()
        }
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: UNAVAILABLE,
                attachment_bucket: "unavailable",
            }),
        )
            .into_response(),
    }
}

/// Routes exposed by the `SurrealDB` health controller.
#[must_use]
pub fn routes() -> Routes {
    Routes::new().add("/_health/surrealdb", get(health))
}
