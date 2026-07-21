use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
};
use loco_rs::testing::request::boot_test;
use pipauto::{app::App, services::auth::AuthService};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::support::{authenticated_csrf, authenticated_json_request, authenticated_session};

const PASSWORD: &str = "Workshop-password-123";

#[tokio::test]
async fn calendar_browser_renders_authenticated_month_and_equivalent_htmx_fragment() {
    let (router, session, csrf) = authenticated_app().await;
    let vehicle_id = vehicle_fixture(&router, &session, &csrf).await;
    for (index, start) in ["08:00", "09:00", "10:00", "11:00"].into_iter().enumerate() {
        let intervention_id = intervention_fixture(
            &router,
            &session,
            &csrf,
            &vehicle_id,
            &format!("2026-07-21T{start}"),
            60,
        )
        .await;
        if index == 1 {
            write_json(
                &router,
                Method::POST,
                &format!("/api/v1/interventions/{intervention_id}/complete"),
                &session,
                &csrf,
                Value::Null,
            )
            .await;
        }
    }
    intervention_fixture(
        &router,
        &session,
        &csrf,
        &vehicle_id,
        "2026-07-21T23:30",
        120,
    )
    .await;

    let response = send(
        &router,
        get("/calendar?view=month&date=2026-07-21", &session, false),
    )
    .await;
    assert_eq!(response.0, StatusCode::OK, "{}", response.1);
    assert_eq!(response.2.as_deref(), Some("no-store"));
    assert!(response.1.starts_with("<!doctype html>"));
    assert!(response.1.contains("id=\"calendar-region\""));
    assert!(response
        .1
        .contains("href=\"/calendar\" aria-current=\"page\">Calendar</a>"));
    assert!(response.1.contains("New intervention"));
    assert!(response.1.contains("href=\"/vehicles\""));
    assert!(response.1.contains("July 2026"));
    assert!(response.1.contains("Monday"));
    assert!(response.1.contains("Show 2 more interventions"));
    assert!(response.1.contains("Calendar Snapshot Owner"));
    assert!(response.1.contains("CAL-77 · Volkswagen Golf"));
    assert!(response.1.contains("Draft"));
    assert!(response.1.contains("Completed"));
    assert!(response.1.contains("Continues into the next day"));
    assert!(response.1.contains("Continues from the previous day"));
    assert!(response
        .1
        .contains("Tuesday 21 July 2026 · 5 interventions"));
    for control in ["Previous", "Today", "Next", "Month", "Week"] {
        assert!(response.1.contains(&format!(">{control}</a>")));
    }

    let fragment = send(
        &router,
        get("/calendar?view=month&date=2026-07-21", &session, true),
    )
    .await;
    assert_eq!(fragment.0, StatusCode::OK);
    assert!(fragment.1.starts_with("<div id=\"calendar-region\""));
    assert!(!fragment.1.contains("<!doctype html>"));
    assert_eq!(fragment.3.as_deref(), Some("HX-Request"));
}

#[tokio::test]
async fn calendar_browser_owns_invalid_query_and_session_recovery_states() {
    let (router, session, _) = authenticated_app().await;
    for uri in [
        "/calendar?view=day&date=2026-07-21",
        "/calendar?view=month&date=2026-7-21",
        "/calendar?view=month&date=2026-02-30",
        "/calendar?view=month&date=2026-07-21&cursor=opaque",
    ] {
        let invalid = send(&router, get(uri, &session, false)).await;
        assert_eq!(invalid.0, StatusCode::UNPROCESSABLE_ENTITY, "{uri}");
        assert!(invalid.1.contains("Check the Calendar link"));
        assert!(invalid.1.contains("Open current month"));
        assert!(invalid
            .1
            .contains("href=\"/calendar\" aria-current=\"page\">Calendar</a>"));
    }

    let week = send(
        &router,
        get("/calendar?view=week&date=2026-07-21", &session, false),
    )
    .await;
    assert_eq!(week.0, StatusCode::OK);
    assert!(week.1.contains("data-calendar-view=\"week\""));
    assert!(week.1.contains("Scrollable 24-hour Week timeline"));
    assert!(week.1.contains("Focused Week view"));
    assert_eq!(week.1.matches("class=\"calendar-week-date").count(), 7);
    assert_eq!(week.1.matches("class=\"calendar-time-row\"").count(), 96);

    let expired = router
        .oneshot(
            Request::builder()
                .uri("/calendar?view=month&date=2026-07-21")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("expired-session request");
    assert_eq!(expired.status(), StatusCode::SEE_OTHER);
    assert!(expired.headers()[header::LOCATION]
        .to_str()
        .expect("login location")
        .starts_with("/login?next="));
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

async fn vehicle_fixture(router: &axum::Router, session: &str, csrf: &str) -> String {
    let customer = write_json(
        router,
        Method::POST,
        "/api/v1/customers",
        session,
        csrf,
        json!({"display_name": "Calendar Snapshot Owner"}),
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
        json!({
            "customer_id": customer,
            "make": "Volkswagen",
            "model": "Golf",
            "registration": "CAL-77"
        }),
    )
    .await["data"]["id"]
        .as_str()
        .expect("vehicle id")
        .to_owned()
}

async fn intervention_fixture(
    router: &axum::Router,
    session: &str,
    csrf: &str,
    vehicle_id: &str,
    service_date: &str,
    duration: u16,
) -> String {
    write_json(
        router,
        Method::POST,
        "/api/v1/interventions",
        session,
        csrf,
        json!({
            "vehicle_id": vehicle_id,
            "service_date": service_date,
            "estimated_duration_minutes": duration,
            "performed_work": "Calendar rendering fixture"
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

async fn send(router: &axum::Router, request: Request<Body>) -> CalendarResponse {
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("Calendar request");
    let status = response.status();
    let cache_control = response
        .headers()
        .get(header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let vary = response
        .headers()
        .get(header::VARY)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body")
            .to_vec(),
    )
    .expect("UTF-8 response");
    (status, body, cache_control, vary)
}

type CalendarResponse = (StatusCode, String, Option<String>, Option<String>);
