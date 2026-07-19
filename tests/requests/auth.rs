use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use loco_rs::testing::request::boot_test;
use pipauto::{app::App, services::auth::AuthService};
use tower::ServiceExt;

const ORIGIN: &str = "http://localhost:5150";
const PASSWORD: &str = "Workshop-password-123";

#[tokio::test]
async fn login_page_is_public_no_store_and_hardened() {
    let boot = boot_test::<App>().await.expect("application should boot");
    let response = boot
        .router
        .expect("router should exist")
        .oneshot(get("/login?next=/vehicles"))
        .await
        .expect("request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(header_text),
        Some("no-store")
    );
    assert_eq!(
        response
            .headers()
            .get("x-frame-options")
            .and_then(header_text),
        Some("DENY")
    );
    assert!(response.headers().contains_key("content-security-policy"));
    assert!(response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .any(|value| {
            header_text(value).is_some_and(|cookie| {
                cookie.starts_with("pipauto_login_csrf=")
                    && cookie.contains("HttpOnly")
                    && cookie.contains("SameSite=Lax")
                    && cookie.contains("Path=/")
            })
        }));

    let html = body_text(response).await;
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("name=\"next\" value=\"&#x2F;vehicles\""));
    assert!(html.contains("name=\"_csrf\""));
    assert!(html.contains("autofocus"));
    assert!(!html.contains(PASSWORD));
}

#[tokio::test]
async fn complete_and_htmx_login_flow_never_echoes_password() {
    let boot = boot_test::<App>().await.expect("application should boot");
    let service = boot
        .app_context
        .shared_store
        .get::<AuthService>()
        .expect("auth service should exist");
    service
        .create_user("filippo@example.com", "Filippo", PASSWORD)
        .await
        .expect("fixture user should be created");
    let router = boot.router.expect("router should exist");

    let login_page = router
        .clone()
        .oneshot(get("/login"))
        .await
        .expect("login page should load");
    let cookie = cookie_pair(&login_page, "pipauto_login_csrf");
    let csrf = hidden_value(&body_text(login_page).await, "_csrf");

    let invalid = router
        .clone()
        .oneshot(post_login(&cookie, &csrf, "wrong-password", true))
        .await
        .expect("invalid login should complete");
    assert_eq!(invalid.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        invalid.headers().get(header::VARY).and_then(header_text),
        Some("HX-Request")
    );
    let invalid_html = body_text(invalid).await;
    assert!(invalid_html.starts_with("<section"));
    assert!(invalid_html.contains("Invalid credentials."));
    assert!(!invalid_html.contains("wrong-password"));

    let login_page = router
        .clone()
        .oneshot(get("/login"))
        .await
        .expect("login page should reload");
    let cookie = cookie_pair(&login_page, "pipauto_login_csrf");
    let csrf = hidden_value(&body_text(login_page).await, "_csrf");
    let success = router
        .clone()
        .oneshot(post_login(&cookie, &csrf, PASSWORD, false))
        .await
        .expect("login should complete");
    assert_eq!(success.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        success
            .headers()
            .get(header::LOCATION)
            .and_then(header_text),
        Some("/")
    );
    assert!(success
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .any(|value| {
            header_text(value).is_some_and(|cookie| cookie.starts_with("pipauto_session="))
        }));
}

#[tokio::test]
async fn login_rejects_missing_origin_cookie_and_conflicting_csrf_inputs() {
    let boot = boot_test::<App>().await.expect("application should boot");
    let router = boot.router.expect("router should exist");
    let page = router
        .clone()
        .oneshot(get("/login"))
        .await
        .expect("login page should load");
    let cookie = cookie_pair(&page, "pipauto_login_csrf");
    let csrf = hidden_value(&body_text(page).await, "_csrf");

    let missing_origin = post_login(&cookie, &csrf, PASSWORD, false);
    let (mut parts, body) = missing_origin.into_parts();
    parts.headers.remove(header::ORIGIN);
    let response = router
        .clone()
        .oneshot(Request::from_parts(parts, body))
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let response = router
        .clone()
        .oneshot(post_login("", &csrf, PASSWORD, false))
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    let mut request = post_login(&cookie, &csrf, PASSWORD, false);
    request.headers_mut().insert(
        "X-CSRF-Token",
        "conflict".parse().expect("header should parse"),
    );
    let response = router
        .oneshot(request)
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request should build")
}

fn post_login(cookie: &str, csrf: &str, password: &str, htmx: bool) -> Request<Body> {
    let body = format!("email=filippo%40example.com&password={password}&_csrf={csrf}&next=%2F");
    let mut builder = Request::builder()
        .method("POST")
        .uri("/login")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ORIGIN, ORIGIN)
        .header(header::COOKIE, cookie);
    if htmx {
        builder = builder.header("HX-Request", "true");
    }
    builder
        .body(Body::from(body))
        .expect("request should build")
}

fn cookie_pair(response: &axum::response::Response, name: &str) -> String {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(header_text)
        .find(|value| value.starts_with(&format!("{name}=")))
        .and_then(|value| value.split(';').next())
        .expect("cookie should be present")
        .to_owned()
}

fn hidden_value(html: &str, name: &str) -> String {
    let marker = format!("name=\"{name}\" value=\"");
    let rest = html
        .split_once(&marker)
        .expect("hidden field should exist")
        .1;
    rest.split_once('"')
        .expect("hidden value should end")
        .0
        .to_owned()
}

fn header_text(value: &header::HeaderValue) -> Option<&str> {
    value.to_str().ok()
}

async fn body_text(response: axum::response::Response) -> String {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should be readable");
    String::from_utf8(bytes.to_vec()).expect("body should be UTF-8")
}
