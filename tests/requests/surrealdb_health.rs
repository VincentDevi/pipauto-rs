use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use loco_rs::testing::request::boot_test;
use pipauto::{
    app::App,
    database::client::{AppDatabase, DatabaseHealthError, DatabaseHealthService},
};
use serde_json::json;
use tower::ServiceExt;

struct ControlledDatabaseHealth {
    healthy: Arc<AtomicBool>,
}

#[async_trait]
impl DatabaseHealthService for ControlledDatabaseHealth {
    async fn health(&self) -> Result<(), DatabaseHealthError> {
        if self.healthy.load(Ordering::Relaxed) {
            Ok(())
        } else {
            Err(DatabaseHealthError)
        }
    }
}

#[tokio::test]
async fn database_health_endpoint_returns_stable_healthy_and_unavailable_responses() {
    let boot = boot_test::<App>()
        .await
        .expect("test application should boot");
    let healthy = Arc::new(AtomicBool::new(true));
    let database = AppDatabase::from_health_service(Arc::new(ControlledDatabaseHealth {
        healthy: Arc::clone(&healthy),
    }));
    boot.app_context.shared_store.insert(database);
    let router = boot.router.expect("server boot should create a router");

    let response = router
        .clone()
        .oneshot(health_request())
        .await
        .expect("healthy request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await, json!({"status": "healthy"}));

    healthy.store(false, Ordering::Relaxed);

    let response = router
        .oneshot(health_request())
        .await
        .expect("unavailable request should complete");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response_json(response).await,
        json!({"status": "unavailable"})
    );
}

fn health_request() -> Request<Body> {
    Request::builder()
        .uri("/_health/surrealdb")
        .body(Body::empty())
        .expect("health request should be valid")
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&bytes).expect("response body should be JSON")
}
