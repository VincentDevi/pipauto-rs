use axum::{
    body::{to_bytes, Body},
    http::{header::CONTENT_TYPE, Request, StatusCode},
};
use loco_rs::testing::request::boot_test;
use pipauto::app::App;
use tower::ServiceExt;

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
    assert!(!html.contains("<script"));
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

fn request(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request should be valid")
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
