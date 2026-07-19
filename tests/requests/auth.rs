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
async fn security_headers_are_compatible_with_the_self_hosted_login_page() {
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
    let csp = response
        .headers()
        .get("content-security-policy")
        .and_then(header_text)
        .expect("CSP should be text");
    assert!(csp.contains("script-src 'self'"));
    assert!(csp.contains("style-src 'self'"));
    assert!(csp.contains("frame-ancestors 'none'"));
    assert!(!csp.contains("'unsafe-inline'"));
    assert_eq!(
        response
            .headers()
            .get("referrer-policy")
            .and_then(header_text),
        Some("no-referrer")
    );
    assert_eq!(
        response
            .headers()
            .get("x-content-type-options")
            .and_then(header_text),
        Some("nosniff")
    );
    assert_eq!(
        response
            .headers()
            .get("permissions-policy")
            .and_then(header_text),
        Some("camera=(), microphone=(), geolocation=()")
    );
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
async fn authenticated_layout_contains_only_safe_shell_identity_csrf_and_path() {
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
    let session = login_session(&router).await;

    let response = router
        .oneshot(get_with_cookie("/", &session))
        .await
        .expect("private request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(header_text),
        Some("no-store")
    );
    let html = body_text(response).await;
    assert!(html.contains("Filippo"));
    assert!(!html.contains("filippo@example.com"));
    assert!(!html.contains("user:"));
    assert!(!html.contains("pipauto_session"));
    assert!(html.contains("href=\"/\" aria-current=\"page\">Workshop</a>"));
    assert!(html.contains("<meta name=\"csrf-token\""));
    assert!(html.contains("name=\"_csrf\""));
    assert!(html.contains("method=\"post\" action=\"/logout\""));
}

#[tokio::test]
async fn login_requests_redirect_authenticated_user_and_clear_stale_credentials() {
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

    let stale = router
        .clone()
        .oneshot(get_with_cookie("/login", "pipauto_session=not-a-jwt"))
        .await
        .expect("stale login request should complete");
    assert_eq!(stale.status(), StatusCode::OK);
    assert!(stale
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .any(|value| {
            header_text(value).is_some_and(|cookie| {
                cookie.starts_with("pipauto_session=") && cookie.contains("Max-Age=0")
            })
        }));
    assert!(stale
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .any(|value| {
            header_text(value).is_some_and(|cookie| cookie.starts_with("pipauto_login_csrf="))
        }));

    let session = login_session(&router).await;
    let response = router
        .clone()
        .oneshot(get_with_cookie("/login?next=/vehicles", &session))
        .await
        .expect("authenticated login request should complete");
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(header_text),
        Some("/vehicles")
    );

    let mut request = get_with_cookie("/login?next=/vehicles", &session);
    request
        .headers_mut()
        .insert("HX-Request", "true".parse().expect("header should parse"));
    let response = router
        .oneshot(request)
        .await
        .expect("authenticated HTMX login request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("HX-Redirect").and_then(header_text),
        Some("/vehicles")
    );
    assert_eq!(
        response.headers().get(header::VARY).and_then(header_text),
        Some("HX-Request")
    );
}

#[tokio::test]
async fn login_requests_complete_and_htmx_flow_never_echoes_password() {
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
    assert!(success
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .any(|value| {
            header_text(value).is_some_and(|cookie| {
                cookie.starts_with("pipauto_login_csrf=") && cookie.contains("Max-Age=0")
            })
        }));
    assert!(!success
        .headers()
        .values()
        .filter_map(header_text)
        .any(|value| value.contains(&csrf)));

    let session = cookie_pair(&success, "pipauto_session");
    let private_page = router
        .oneshot(get_with_cookie("/", &session))
        .await
        .expect("authenticated landing page should load");
    let authenticated_csrf = meta_value(&body_text(private_page).await, "csrf-token");
    assert_ne!(authenticated_csrf, csrf);
}

#[tokio::test]
async fn login_requests_reject_missing_origin_cookie_and_conflicting_csrf_inputs() {
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
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(header_text),
        Some("no-store")
    );

    let mut referer_request = post_login(&cookie, &csrf, "wrong-password", false);
    referer_request.headers_mut().remove(header::ORIGIN);
    referer_request.headers_mut().insert(
        header::REFERER,
        "http://localhost:5150/login"
            .parse()
            .expect("referer should parse"),
    );
    let response = router
        .clone()
        .oneshot(referer_request)
        .await
        .expect("same-origin referer request should complete");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

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

#[tokio::test]
async fn logout_requests_reject_invalid_csrf_before_revocation_and_accept_html() {
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
    let session = login_session(&router).await;
    let private_page = router
        .clone()
        .oneshot(get_with_cookie("/", &session))
        .await
        .expect("private page should load");
    let csrf = meta_value(&body_text(private_page).await, "csrf-token");

    for request in [
        post_logout(&session, "", ORIGIN),
        post_logout(&session, "wrong-token", ORIGIN),
        post_logout(&session, &csrf, "https://attacker.example"),
    ] {
        let response = router
            .clone()
            .oneshot(request)
            .await
            .expect("rejected logout should complete");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let still_authenticated = router
            .clone()
            .oneshot(get_with_cookie("/", &session))
            .await
            .expect("session check should complete");
        assert_eq!(still_authenticated.status(), StatusCode::OK);
    }

    let response = router
        .clone()
        .oneshot(post_logout(&session, &csrf, ORIGIN))
        .await
        .expect("valid logout should complete");
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(header_text),
        Some("/login")
    );
    assert!(!response
        .headers()
        .values()
        .filter_map(header_text)
        .any(|value| value.contains(&csrf)));
}

#[tokio::test]
async fn login_requests_render_typed_field_errors_and_enforce_small_form_limit() {
    let boot = boot_test::<App>().await.expect("application should boot");
    let router = boot.router.expect("router should exist");
    let page = router
        .clone()
        .oneshot(get("/login?next=/vehicles"))
        .await
        .expect("login page should load");
    let cookie = cookie_pair(&page, "pipauto_login_csrf");
    let csrf = hidden_value(&body_text(page).await, "_csrf");

    let invalid_body = format!("email=invalid&_csrf={csrf}&next=%2Fvehicles");
    let invalid = router
        .clone()
        .oneshot(form_request("/login", &cookie, &invalid_body, true))
        .await
        .expect("invalid form should complete");
    assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let html = body_text(invalid).await;
    assert!(html.starts_with("<section"));
    assert!(html.contains("Email address: Enter a valid email address."));
    assert!(html.contains("Password: Enter your password."));
    assert!(html.contains("name=\"email\" type=\"email\" value=\"invalid\""));
    assert!(html.contains("name=\"next\" value=\"&#x2F;vehicles\""));
    assert!(!html.contains("autofocus"));
    assert!(!html.contains("value=\"password"));

    let oversized = format!(
        "email=filippo%40example.com&password={}&_csrf={csrf}&next=%2F",
        "x".repeat(4_096)
    );
    let response = router
        .clone()
        .oneshot(form_request("/login", &cookie, &oversized, false))
        .await
        .expect("oversized form should complete");
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(header_text),
        Some("no-store")
    );

    let response = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::ORIGIN, ORIGIN)
                .header(header::COOKIE, cookie)
                .body(Body::from("{}"))
                .expect("request should build"),
        )
        .await
        .expect("unsupported form should complete");
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(header_text),
        Some("no-store")
    );
}

#[tokio::test]
async fn login_throttling_is_temporary_bounded_and_preserves_htmx_behavior() {
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

    let page = router
        .clone()
        .oneshot(get("/login?next=/vehicles"))
        .await
        .expect("login page should load");
    let cookie = cookie_pair(&page, "pipauto_login_csrf");
    let csrf = hidden_value(&body_text(page).await, "_csrf");
    let body =
        format!("email=filippo%40example.com&password={PASSWORD}&_csrf={csrf}&next=%2Fvehicles");
    let response = router
        .clone()
        .oneshot(form_request("/login", &cookie, &body, true))
        .await
        .expect("HTMX login should complete");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("HX-Redirect").and_then(header_text),
        Some("/vehicles")
    );
    assert_eq!(
        response.headers().get(header::VARY).and_then(header_text),
        Some("HX-Request")
    );

    let mut throttled = None;
    for _attempt in 0..10 {
        let page = router
            .clone()
            .oneshot(get("/login"))
            .await
            .expect("login page should load");
        let cookie = cookie_pair(&page, "pipauto_login_csrf");
        let csrf = hidden_value(&body_text(page).await, "_csrf");
        let response = router
            .clone()
            .oneshot(post_login(&cookie, &csrf, "wrong-password", false))
            .await
            .expect("failed login should complete");
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            throttled = Some(response);
            break;
        }
    }
    let response = throttled.expect("repeated failures should throttle");
    let retry_after = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(header_text)
        .and_then(|value| value.parse::<u64>().ok())
        .expect("Retry-After should be numeric");
    assert!((1..=300).contains(&retry_after));
    assert!(body_text(response)
        .await
        .contains("Too many attempts. Wait briefly and try again."));
}

#[tokio::test]
async fn logout_requests_are_post_only_idempotent_and_htmx_equivalent() {
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

    let get_response = router
        .clone()
        .oneshot(get("/logout"))
        .await
        .expect("GET logout should complete");
    assert_eq!(get_response.status(), StatusCode::METHOD_NOT_ALLOWED);

    for cookie in ["", "pipauto_session=not-a-jwt"] {
        let response = router
            .clone()
            .oneshot(post_logout(cookie, "", ORIGIN))
            .await
            .expect("inactive logout should complete");
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_session_cookie_cleared(&response);
    }

    let session = login_session(&router).await;
    let page = router
        .clone()
        .oneshot(get_with_cookie("/", &session))
        .await
        .expect("private page should load");
    let csrf = meta_value(&body_text(page).await, "csrf-token");
    let mut request = post_logout(&session, &csrf, ORIGIN);
    request
        .headers_mut()
        .insert("HX-Request", "true".parse().expect("header should parse"));
    request
        .headers_mut()
        .insert("X-CSRF-Token", csrf.parse().expect("header should parse"));
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("HTMX logout should complete");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("HX-Redirect").and_then(header_text),
        Some("/login")
    );
    assert_eq!(
        response.headers().get(header::VARY).and_then(header_text),
        Some("HX-Request")
    );
    assert_session_cookie_cleared(&response);

    let response = router
        .oneshot(post_logout(&session, "", ORIGIN))
        .await
        .expect("already-revoked logout should complete");
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_session_cookie_cleared(&response);
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .body(Body::empty())
        .expect("request should build")
}

fn get_with_cookie(uri: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(header::COOKIE, cookie)
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
        builder = builder
            .header("HX-Request", "true")
            .header("X-CSRF-Token", csrf);
    }
    builder
        .body(Body::from(body))
        .expect("request should build")
}

fn post_logout(cookie: &str, csrf: &str, origin: &str) -> Request<Body> {
    let body = if csrf.is_empty() {
        String::new()
    } else {
        format!("_csrf={csrf}")
    };
    Request::builder()
        .method("POST")
        .uri("/logout")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ORIGIN, origin)
        .header(header::COOKIE, cookie)
        .body(Body::from(body))
        .expect("request should build")
}

fn form_request(uri: &str, cookie: &str, body: &str, htmx: bool) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::ORIGIN, ORIGIN)
        .header(header::COOKIE, cookie);
    if htmx {
        let csrf = body
            .split('&')
            .find_map(|field| field.strip_prefix("_csrf="))
            .expect("HTMX form should carry CSRF");
        builder = builder
            .header("HX-Request", "true")
            .header("X-CSRF-Token", csrf);
    }
    builder
        .body(Body::from(body.to_owned()))
        .expect("request should build")
}

async fn login_session(router: &axum::Router) -> String {
    let login_page = router
        .clone()
        .oneshot(get("/login"))
        .await
        .expect("login page should load");
    let cookie = cookie_pair(&login_page, "pipauto_login_csrf");
    let csrf = hidden_value(&body_text(login_page).await, "_csrf");
    let response = router
        .clone()
        .oneshot(post_login(&cookie, &csrf, PASSWORD, false))
        .await
        .expect("login should complete");
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    cookie_pair(&response, "pipauto_session")
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

fn assert_session_cookie_cleared(response: &axum::response::Response) {
    assert!(response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(header_text)
        .any(|cookie| cookie.starts_with("pipauto_session=") && cookie.contains("Max-Age=0")));
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

fn meta_value(html: &str, name: &str) -> String {
    let marker = format!("<meta name=\"{name}\" content=\"");
    let rest = html.split_once(&marker).expect("meta value should exist").1;
    rest.split_once('"')
        .expect("meta value should end")
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
