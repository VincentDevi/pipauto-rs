use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
};
use loco_rs::testing::request::boot_test;
use pipauto::{app::App, models::auth::AuthenticationModel as AuthService};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::support::{authenticated_csrf, authenticated_json_request, authenticated_session};

const PASSWORD: &str = "Workshop-password-123";

#[tokio::test]
async fn dashboard_empty_and_htmx_states_keep_workshop_actions_available() {
    let (router, session, _) = authenticated_app().await;

    let response = router
        .clone()
        .oneshot(get("/", &session, false))
        .await
        .expect("dashboard request");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
    let html = body_text(response).await;
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("Welcome, Filippo"));
    for (href, label) in [
        ("/vehicles", "New intervention"),
        ("/customers/new", "New customer"),
        ("/vehicles", "Register vehicle"),
        ("/invoices/new", "New invoice"),
        ("/knowledge/new", "New technical note"),
    ] {
        assert!(
            html.contains(&format!("href=\"{href}\">{label}</a>")),
            "missing {label} action"
        );
    }
    assert!(html.contains("No interventions have been recorded yet"));
    assert!(html.contains("There are no draft interventions"));
    assert!(!html.contains("Outstanding invoices"));
    assert!(!html.contains("Database connection"));

    let fragment = router
        .clone()
        .oneshot(get("/", &session, true))
        .await
        .expect("HTMX dashboard request");
    assert_eq!(fragment.status(), StatusCode::OK);
    assert_eq!(fragment.headers().get(header::VARY).unwrap(), "HX-Request");
    let fragment = body_text(fragment).await;
    assert!(fragment.starts_with("<div id=\"dashboard-content\""));
    assert!(!fragment.contains("<!doctype html>"));

    let section = router
        .clone()
        .oneshot(get("/dashboard/draft-interventions", &session, true))
        .await
        .expect("HTMX section request");
    assert_eq!(section.status(), StatusCode::OK);
    let section = body_text(section).await;
    assert!(section.contains("id=\"draft-interventions\""));
    assert!(!section.contains("id=\"recent-interventions\""));

    let fallback = router
        .oneshot(get("/dashboard/draft-interventions", &session, false))
        .await
        .expect("standard section request");
    assert_eq!(fallback.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        fallback.headers().get(header::LOCATION).unwrap(),
        "/#draft-interventions"
    );
}

#[tokio::test]
async fn dashboard_preserves_intervention_service_order_and_draft_filter() {
    let (router, session, csrf) = authenticated_app().await;
    let vehicle_id = create_vehicle(&router, &session, &csrf).await;
    let older_id = create_intervention(
        &router,
        &session,
        &csrf,
        &vehicle_id,
        "2026-07-19",
        100_000,
        "Older completed work",
    )
    .await;
    write_json(
        &router,
        Method::POST,
        &format!("/api/v1/interventions/{older_id}/complete"),
        &session,
        &csrf,
        Value::Null,
    )
    .await;
    let newest_id = create_intervention(
        &router,
        &session,
        &csrf,
        &vehicle_id,
        "2026-07-20",
        101_000,
        "Newest draft work",
    )
    .await;

    let response = router
        .oneshot(get("/", &session, false))
        .await
        .expect("dashboard request");
    assert_eq!(response.status(), StatusCode::OK);
    let html = body_text(response).await;
    let recent = section(&html, "recent-interventions");
    let drafts = section(&html, "draft-interventions");

    assert!(
        recent.find("Newest draft work").unwrap() < recent.find("Older completed work").unwrap()
    );
    assert!(recent.contains(&format!("href=\"/interventions/{newest_id}\"")));
    assert!(recent.contains(&format!("href=\"/interventions/{older_id}\"")));
    assert!(drafts.contains("Newest draft work"));
    assert!(!drafts.contains("Older completed work"));
    assert!(drafts.contains("href=\"/interventions?status=draft\""));
}

async fn authenticated_app() -> (axum::Router, String, String) {
    let boot = boot_test::<App>().await.expect("application should boot");
    boot.app_context
        .shared_store
        .get::<AuthService>()
        .expect("auth service")
        .create_user("filippo@example.com", "Filippo", PASSWORD)
        .await
        .expect("fixture user");
    let router = boot.router.expect("router");
    let session = authenticated_session(&router, PASSWORD).await;
    let csrf = authenticated_csrf(&router, &session).await;
    (router, session, csrf)
}

async fn create_vehicle(router: &axum::Router, session: &str, csrf: &str) -> String {
    let customer = write_json(
        router,
        Method::POST,
        "/api/v1/customers",
        session,
        csrf,
        json!({"display_name": "Dashboard owner"}),
    )
    .await["data"]["id"]
        .as_str()
        .expect("customer id")
        .to_owned();
    write_json(
        router,
        Method::POST,
        "/api/v1/vehicles",
        session,
        csrf,
        json!({"customer_id": customer, "make": "Volkswagen", "model": "Golf"}),
    )
    .await["data"]["id"]
        .as_str()
        .expect("vehicle id")
        .to_owned()
}

#[allow(clippy::too_many_arguments)]
async fn create_intervention(
    router: &axum::Router,
    session: &str,
    csrf: &str,
    vehicle_id: &str,
    service_date: &str,
    mileage: u64,
    performed_work: &str,
) -> String {
    write_json(
        router,
        Method::POST,
        "/api/v1/interventions",
        session,
        csrf,
        json!({
            "vehicle_id": vehicle_id,
            "service_date": format!("{service_date}T09:00"),
            "estimated_duration_minutes": 60,
            "mileage": mileage,
            "performed_work": performed_work
        }),
    )
    .await["data"]["id"]
        .as_str()
        .expect("intervention id")
        .to_owned()
}

async fn write_json(
    router: &axum::Router,
    method: Method,
    uri: &str,
    session: &str,
    csrf: &str,
    value: Value,
) -> Value {
    let response = router
        .clone()
        .oneshot(authenticated_json_request(
            method,
            uri,
            session,
            csrf,
            value.to_string(),
        ))
        .await
        .expect("JSON request");
    assert!(response.status().is_success(), "request failed: {uri}");
    serde_json::from_slice(
        &to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body"),
    )
    .expect("JSON response")
}

fn get(uri: &str, session: &str, htmx: bool) -> Request<Body> {
    let mut builder = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header(header::COOKIE, session);
    if htmx {
        builder = builder.header("HX-Request", "true");
    }
    builder.body(Body::empty()).expect("request")
}

fn section<'html>(html: &'html str, id: &str) -> &'html str {
    html.split_once(&format!("id=\"{id}\""))
        .expect("section start")
        .1
        .split_once("</section>")
        .expect("section end")
        .0
}

async fn body_text(response: axum::response::Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body")
            .to_vec(),
    )
    .expect("UTF-8 response")
}
