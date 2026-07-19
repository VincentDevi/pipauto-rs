use axum::{
    body::{to_bytes, Body},
    http::{
        header::{CONTENT_TYPE, LOCATION},
        Request, StatusCode,
    },
};
use loco_rs::testing::request::boot_test;
use pipauto::app::App;
use tower::ServiceExt;

#[tokio::test]
async fn protected_routes_redirect_guests_to_login_with_a_safe_return_path() {
    let boot = boot_test::<App>()
        .await
        .expect("test application should boot");
    let router = boot.router.expect("server boot should create a router");

    let response = router
        .oneshot(request("/"))
        .await
        .expect("setup page request should complete");

    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/login?next=/")
    );
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::VARY)
            .and_then(|value| value.to_str().ok()),
        Some("HX-Request")
    );
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
async fn auth_redirects_htmx_guests_without_leaking_database_state() {
    let boot = boot_test::<App>()
        .await
        .expect("test application should boot");
    let router = boot.router.expect("server boot should create a router");

    let response = router
        .oneshot(htmx_request("/setup/status"))
        .await
        .expect("status request should complete");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get("HX-Redirect")
            .and_then(|value| value.to_str().ok()),
        Some("/login?next=/setup/status")
    );
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::VARY)
            .and_then(|value| value.to_str().ok()),
        Some("HX-Request")
    );
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
