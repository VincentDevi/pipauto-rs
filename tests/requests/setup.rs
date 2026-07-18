use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    http::{
        header::{CONTENT_TYPE, VARY},
        HeaderValue, Request, StatusCode,
    },
};
use loco_rs::testing::request::boot_test;
use pipauto::{
    app::App,
    database::client::{AppDatabase, DatabaseHealthError, DatabaseHealthService},
};
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
async fn setup_page_returns_complete_server_rendered_html() {
    let boot = boot_test::<App>()
        .await
        .expect("test application should boot");
    let router = boot.router.expect("server boot should create a router");

    let response = router
        .oneshot(request("/"))
        .await
        .expect("setup page request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_content_type(response.headers(), "text/html");

    let html = response_text(response).await;
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("<title>Pipauto setup</title>"));
    assert!(html.contains("<main id=\"main-content\""));
    assert!(html.contains("The application foundation is running."));
    assert!(html.contains("href=\"/static/css/app.css\""));
    assert!(html.contains("src=\"/static/vendor/htmx.min.js\""));
    assert!(html.contains("hx-get=\"/setup/status\""));
    assert!(html.contains("hx-target=\"#setup-status\""));
    assert!(html.contains("role=\"status\""));
    assert!(html.contains("aria-live=\"polite\""));
    assert!(html.contains("Not checked yet"));
    assert!(html.contains("Checking…"));
    assert!(!html.contains("https://cdn.jsdelivr.net"));
}

#[tokio::test]
async fn setup_stylesheet_is_served_from_static_assets() {
    let boot = boot_test::<App>()
        .await
        .expect("test application should boot");
    let router = boot.router.expect("server boot should create a router");

    let response = router
        .oneshot(request("/static/css/app.css"))
        .await
        .expect("stylesheet request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_content_type(response.headers(), "text/css");
    assert!(response_text(response)
        .await
        .contains("@media (max-width: 22rem)"));
}

#[tokio::test]
async fn vendored_htmx_is_served_from_static_assets() {
    let boot = boot_test::<App>()
        .await
        .expect("test application should boot");
    let router = boot.router.expect("server boot should create a router");

    let response = router
        .oneshot(request("/static/vendor/htmx.min.js"))
        .await
        .expect("HTMX asset request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_content_type(response.headers(), "text/javascript");
    assert!(response_text(response)
        .await
        .starts_with("var htmx=function()"));
}

#[tokio::test]
async fn setup_status_returns_accessible_fragments_for_both_database_outcomes() {
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
        .oneshot(htmx_request("/setup/status"))
        .await
        .expect("connected status request should complete");
    assert_setup_status_response(&response);
    let connected = response_text(response).await;
    assert!(connected.contains("Connected"));
    assert!(connected.contains("application database responded"));
    assert!(!connected.contains("<!doctype html>"));

    healthy.store(false, Ordering::Relaxed);

    let response = router
        .oneshot(htmx_request("/setup/status"))
        .await
        .expect("unavailable status request should complete");
    assert_setup_status_response(&response);
    let unavailable = response_text(response).await;
    assert!(unavailable.contains("Unavailable"));
    assert!(unavailable.contains("application database did not respond"));
    assert!(!unavailable.contains("<!doctype html>"));
    assert_ne!(connected, unavailable);
}

fn request(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request should be valid")
}

fn htmx_request(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("HX-Request", "true")
        .body(Body::empty())
        .expect("HTMX request should be valid")
}

fn assert_setup_status_response(response: &axum::response::Response) {
    assert_eq!(response.status(), StatusCode::OK);
    assert_content_type(response.headers(), "text/html");
    assert_eq!(
        response.headers().get(VARY),
        Some(&HeaderValue::from_static("HX-Request"))
    );
}

fn assert_content_type(headers: &axum::http::HeaderMap, expected: &str) {
    let content_type = headers
        .get(CONTENT_TYPE)
        .expect("response should include a content type")
        .to_str()
        .expect("content type should be valid ASCII");
    assert!(
        content_type.starts_with(expected),
        "expected content type to start with {expected:?}, got {content_type:?}"
    );
}

async fn response_text(response: axum::response::Response) -> String {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    String::from_utf8(bytes.to_vec()).expect("response body should be UTF-8")
}
