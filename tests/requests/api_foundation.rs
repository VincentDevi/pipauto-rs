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
use loco_rs::testing::request::boot_test;
use pipauto::{
    api::DataEnvelope,
    app::App,
    auth::{csrf::AuthenticatedCsrfJson, extractors::CurrentUser},
    services::auth::AuthService,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::support::{authenticated_csrf, authenticated_json_request, authenticated_session};

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

async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&body).expect("response should contain JSON")
}
