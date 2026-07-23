use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use axum::{
    body::{to_bytes, Body},
    extract::{DefaultBodyLimit, Extension},
    http::{header, Method, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{TimeDelta, Utc};
use loco_rs::testing::request::boot_test;
use pipauto::{
    api::DataEnvelope,
    app::{route_access_inventory, App},
    auth::{
        csrf::{AuthenticatedCsrfJson, CsrfService},
        extractors::CurrentUser,
    },
    models::auth::AuthenticationModel as AuthService,
    routing::AccessClass,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::support::{
    authenticated_csrf, authenticated_json_request, authenticated_session, TEST_ORIGIN,
};

const PASSWORD: &str = "Workshop-password-123";

#[derive(Deserialize)]
struct ProbeRequest {
    value: String,
}

#[derive(Serialize)]
struct ProbeResponse {
    value: String,
}

async fn safe_probe(CurrentUser(user): CurrentUser) -> impl IntoResponse {
    Json(DataEnvelope::new(ProbeResponse {
        value: user.display_name,
    }))
}

async fn unsafe_probe(
    CurrentUser(_user): CurrentUser,
    Extension(invocations): Extension<Arc<AtomicUsize>>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<ProbeRequest>,
) -> impl IntoResponse {
    invocations.fetch_add(1, Ordering::SeqCst);
    Json(DataEnvelope::new(ProbeResponse {
        value: request.value,
    }))
}

#[tokio::test]
async fn api_authentication_returns_json_and_clears_stale_credentials() {
    let boot = boot_test::<App>().await.expect("application should boot");
    let router = Router::new()
        .route("/api/v1/probe", get(safe_probe))
        .layer(axum::middleware::from_fn(
            pipauto::auth::responses::no_store_layer,
        ))
        .with_state(boot.app_context);

    let response = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/v1/probe")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response_json(response).await["error"]["code"],
        "unauthenticated"
    );

    let response = router
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/v1/probe")
                .header(header::COOKIE, "pipauto_session=stale")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .any(|value| value.starts_with("pipauto_session=") && value.contains("Max-Age=0")));
}

#[tokio::test]
async fn api_csrf_rejects_unsafe_requests_before_handler_invocation() {
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
    let app_router = boot.router.expect("application router should exist");
    let session = authenticated_session(&app_router, PASSWORD).await;
    let csrf = authenticated_csrf(&app_router, &session).await;
    let invocations = Arc::new(AtomicUsize::new(0));
    let router = Router::new()
        .route(
            "/api/v1/probe",
            post(unsafe_probe).layer(DefaultBodyLimit::max(128)),
        )
        .layer(Extension(invocations.clone()))
        .layer(axum::middleware::from_fn(
            pipauto::auth::responses::no_store_layer,
        ))
        .with_state(boot.app_context);

    let mut missing = authenticated_json_request(
        Method::POST,
        "/api/v1/probe",
        &session,
        &csrf,
        json!({"value": "ok"}).to_string(),
    );
    missing.headers_mut().remove("X-CSRF-Token");
    let response = router
        .clone()
        .oneshot(missing)
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(invocations.load(Ordering::SeqCst), 0);

    let mut wrong_origin = authenticated_json_request(
        Method::POST,
        "/api/v1/probe",
        &session,
        &csrf,
        json!({"value": "ok"}).to_string(),
    );
    wrong_origin.headers_mut().insert(
        header::ORIGIN,
        "https://attacker.example"
            .parse()
            .expect("origin should parse"),
    );
    let response = router
        .clone()
        .oneshot(wrong_origin)
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(invocations.load(Ordering::SeqCst), 0);

    let response = router
        .oneshot(authenticated_json_request(
            Method::POST,
            "/api/v1/probe",
            &session,
            &csrf,
            json!({"value": "accepted"}).to_string(),
        ))
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(response_json(response).await["data"]["value"], "accepted");
    assert_eq!(invocations.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn api_foundation_enforces_json_content_type_and_route_body_limit() {
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
    let app_router = boot.router.expect("application router should exist");
    let session = authenticated_session(&app_router, PASSWORD).await;
    let csrf = authenticated_csrf(&app_router, &session).await;
    let router = Router::new()
        .route(
            "/api/v1/probe",
            post(unsafe_probe).layer(DefaultBodyLimit::max(64)),
        )
        .layer(Extension(Arc::new(AtomicUsize::new(0))))
        .with_state(boot.app_context);

    let mut wrong_type =
        authenticated_json_request(Method::POST, "/api/v1/probe", &session, &csrf, "value=ok");
    wrong_type.headers_mut().insert(
        header::CONTENT_TYPE,
        "application/x-www-form-urlencoded"
            .parse()
            .expect("content type should parse"),
    );
    let response = router
        .clone()
        .oneshot(wrong_type)
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(
        response_json(response).await["error"]["code"],
        "malformed_request"
    );

    let response = router
        .oneshot(authenticated_json_request(
            Method::POST,
            "/api/v1/probe",
            &session,
            &csrf,
            json!({"value": "x".repeat(100)}).to_string(),
        ))
        .await
        .expect("request should complete");
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(
        response_json(response).await["error"]["code"],
        "malformed_request"
    );
}

#[tokio::test]
async fn every_business_route_rejects_guests_and_unsafe_csrf_failures() {
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
    let router = boot.router.expect("application router should exist");

    let login_page = router
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/login")
                .body(Body::empty())
                .expect("login request should build"),
        )
        .await
        .expect("login request should complete");
    let login_html = response_text(login_page).await;
    let wrong_action = html_value(&login_html, "name=\"_csrf\" value=\"");

    let session = authenticated_session(&router, PASSWORD).await;
    let encoded_jwt = session
        .strip_prefix("pipauto_session=")
        .expect("session helper should return the session cookie");
    let authenticated = service
        .authenticate_session(encoded_jwt)
        .await
        .expect("fixture session should authenticate");
    let encoded_claims = encoded_jwt
        .split('.')
        .nth(1)
        .expect("JWT should contain claims");
    let claims: Value = serde_json::from_slice(
        &URL_SAFE_NO_PAD
            .decode(encoded_claims)
            .expect("JWT claims should be base64url"),
    )
    .expect("JWT claims should be JSON");
    let session_jti = claims["jti"].as_str().expect("JWT should contain a jti");
    let csrf_service = boot
        .app_context
        .shared_store
        .get::<CsrfService>()
        .expect("CSRF service should exist");
    let valid = csrf_service
        .issue_authenticated(session_jti, authenticated.user.session_expires_at)
        .expect("valid CSRF should issue")
        .expose()
        .to_owned();
    let expired = csrf_service
        .issue_authenticated(session_jti, Utc::now() - TimeDelta::seconds(1))
        .expect("expired CSRF fixture should issue")
        .expose()
        .to_owned();
    let wrong_session = csrf_service
        .issue_authenticated("different-session", authenticated.user.session_expires_at)
        .expect("wrong-session CSRF fixture should issue")
        .expose()
        .to_owned();

    let route_inventory = route_access_inventory();
    for route in route_inventory.iter().filter(|route| {
        route.class == AccessClass::Authenticated && route.path.starts_with("/api/v1/")
    }) {
        let method = route.method.clone();
        let uri = concrete_uri(&route.path);
        let guest = axum::http::Request::builder()
            .method(method.clone())
            .uri(&uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .expect("guest request should build");
        let response = router
            .clone()
            .oneshot(guest)
            .await
            .expect("guest request should complete");
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{} {} must reject guests",
            route.method,
            route.path
        );

        if method == Method::GET {
            continue;
        }

        let cases = [
            ("missing", None, TEST_ORIGIN),
            ("expired", Some(expired.as_str()), TEST_ORIGIN),
            ("wrong-action", Some(wrong_action.as_str()), TEST_ORIGIN),
            ("wrong-session", Some(wrong_session.as_str()), TEST_ORIGIN),
            (
                "wrong-origin",
                Some(valid.as_str()),
                "https://attacker.example",
            ),
        ];
        for (case, token, origin) in cases {
            let mut request = axum::http::Request::builder()
                .method(method.clone())
                .uri(&uri)
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::COOKIE, &session)
                .header(header::ORIGIN, origin);
            if let Some(token) = token {
                request = request.header("X-CSRF-Token", token);
            }
            let response = router
                .clone()
                .oneshot(
                    request
                        .body(Body::from("{}"))
                        .expect("request should build"),
                )
                .await
                .expect("CSRF request should complete");
            assert_eq!(
                response.status(),
                StatusCode::FORBIDDEN,
                "{} {} must reject {case} CSRF",
                route.method,
                route.path
            );
        }
    }
}

fn concrete_uri(path: &str) -> String {
    path.replace("{id}", "fixture-id")
        .replace("{line_id}", "fixture-line-id")
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
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    String::from_utf8(body.to_vec()).expect("response body should be UTF-8")
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&body).expect("response should contain JSON")
}
