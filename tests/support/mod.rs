//! Reusable test application bootstrapping, settings, fixtures, and helpers.
//!
//! Support code may depend on the public application API and test-only crates. It must not contain
//! test assertions, production behavior, or environment-specific credentials.

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request},
};
use surrealdb::{engine::any::Any, Surreal};
use tower::ServiceExt;

pub const TEST_ORIGIN: &str = "http://localhost:5150";

/// Explicitly define the private attachment bucket for one disposable test database.
///
/// Application startup deliberately does not call this helper. Each selected in-memory database
/// has its own bucket catalog and object namespace.
pub async fn define_attachment_memory_bucket(client: &Surreal<Any>) {
    let response = client
        .query("DEFINE BUCKET pipauto_attachments BACKEND 'memory' PERMISSIONS NONE;")
        .await
        .expect("the attachment test bucket should be definable");
    response
        .check()
        .expect("the attachment test bucket definition should be valid");
}

/// Apply the committed authentication schema to an isolated test database.
pub async fn apply_authentication_schema(client: &Surreal<Any>) {
    let schema = [
        include_str!("../../database/schema/authentication/user.surql"),
        include_str!("../../database/schema/authentication/auth_session.surql"),
        include_str!("../../database/schema/authentication/login_throttle.surql"),
    ]
    .join("\n");
    let response = client
        .query(schema)
        .await
        .expect("committed authentication schema should execute");
    response
        .check()
        .expect("committed authentication definitions should be valid");
}

/// Sign in through the public browser flow and return the complete session cookie pair.
pub async fn authenticated_session(router: &axum::Router, password: &str) -> String {
    let login_page = router
        .clone()
        .oneshot(simple_request(Method::GET, "/login"))
        .await
        .expect("login page request should complete");
    let cookie = cookie_pair(&login_page, "pipauto_login_csrf");
    let body = response_text(login_page).await;
    let csrf = html_value(&body, "name=\"_csrf\" value=\"");
    let body = format!("email=filippo%40example.com&password={password}&_csrf={csrf}&next=%2F");
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, cookie)
                .header(header::ORIGIN, TEST_ORIGIN)
                .body(Body::from(body))
                .expect("login request should build"),
        )
        .await
        .expect("login request should complete");
    cookie_pair(&response, "pipauto_session")
}

/// Read the authenticated session-bound CSRF token rendered by the workshop shell.
pub async fn authenticated_csrf(router: &axum::Router, session: &str) -> String {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::COOKIE, session)
                .body(Body::empty())
                .expect("authenticated request should build"),
        )
        .await
        .expect("authenticated page request should complete");
    let body = response_text(response).await;
    html_value(&body, "<meta name=\"csrf-token\" content=\"")
}

/// Build an authenticated JSON request with the standard session, origin, and CSRF headers.
pub fn authenticated_json_request(
    method: Method,
    uri: &str,
    session: &str,
    csrf: &str,
    body: impl Into<Body>,
) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::COOKIE, session)
        .header(header::ORIGIN, TEST_ORIGIN)
        .header("X-CSRF-Token", csrf)
        .body(body.into())
        .expect("authenticated JSON request should build")
}

fn simple_request(method: Method, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .expect("request should build")
}

fn cookie_pair(response: &axum::response::Response, name: &str) -> String {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(&format!("{name}=")))
        .and_then(|value| value.split(';').next())
        .expect("cookie should be present")
        .to_owned()
}

fn html_value(html: &str, marker: &str) -> String {
    html.split_once(marker)
        .expect("HTML value should exist")
        .1
        .split_once('"')
        .expect("HTML value should end")
        .0
        .to_owned()
}

async fn response_text(response: axum::response::Response) -> String {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    String::from_utf8(bytes.to_vec()).expect("response body should be UTF-8")
}
