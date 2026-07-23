use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
};
use loco_rs::testing::request::boot_test;
use pipauto::{
    app::App,
    models::{
        auth::AuthenticationModel as AuthService,
        calendar::{CalendarEntry, CalendarModel as CalendarService, CalendarRange},
    },
    testing::persistence::{calendar::CalendarRepository, RepositoryError},
};
use serde_json::{json, Value};
use std::sync::Arc;
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
    assert_eq!(response.3.as_deref(), Some("HX-Request"));
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
    assert!(response.1.contains("aria-label=\"Tuesday 21 July 2026, 08:00 to 09:00, CAL-77, Volkswagen Golf, Calendar Snapshot Owner, Draft, 1 h\""));
    assert!(response
        .1
        .contains("datetime=\"2026-07-21T08:00:00+02:00\""));
    assert!(!response.1.contains("role=\"grid\""));
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
async fn calendar_browser_escapes_snapshots_and_uses_only_canonical_local_links() {
    let (router, session, csrf) = authenticated_app().await;
    let customer = write_json(
        &router,
        Method::POST,
        "/api/v1/customers",
        &session,
        &csrf,
        json!({"display_name": "<img src=x onerror=alert(1)> & Owner"}),
    )
    .await["data"]["id"]
        .as_str()
        .expect("customer id")
        .to_owned();
    let vehicle = write_json(
        &router,
        Method::POST,
        "/api/v1/vehicles",
        &session,
        &csrf,
        json!({
            "customer_id": customer,
            "make": "<script>alert(2)</script>",
            "model": "Roadster & Sons",
            "registration": "SAFE-79"
        }),
    )
    .await["data"]["id"]
        .as_str()
        .expect("vehicle id")
        .to_owned();
    intervention_fixture(&router, &session, &csrf, &vehicle, "2026-07-21T08:00", 60).await;

    let response = send(
        &router,
        get("/calendar?view=month&date=2026-07-21", &session, false),
    )
    .await;
    assert_eq!(response.0, StatusCode::OK);
    assert!(!response.1.contains("<script>alert(2)</script>"));
    assert!(!response.1.contains("<img src=x onerror=alert(1)>"));
    assert!(response
        .1
        .contains("&lt;script&gt;alert(2)&lt;&#x2F;script&gt;"));
    assert!(response.1.contains("Roadster &amp; Sons"));
    for href in response
        .1
        .split("href=\"")
        .skip(1)
        .filter_map(|part| part.split('"').next())
    {
        assert!(
            href.starts_with('/') || href.starts_with('#') || href.starts_with("&#x2F;"),
            "non-local calendar link: {href}"
        );
        assert!(
            !href.starts_with("//") && !href.starts_with("&#x2F;&#x2F;"),
            "scheme-relative calendar link: {href}"
        );
    }
}

#[tokio::test]
async fn calendar_browser_failure_states_are_safe_and_equivalent_for_htmx() {
    for (failure, status, heading) in [
        (
            RepositoryError::Unavailable,
            StatusCode::SERVICE_UNAVAILABLE,
            "Calendar is temporarily unavailable",
        ),
        (
            RepositoryError::CorruptData,
            StatusCode::INTERNAL_SERVER_ERROR,
            "Something went wrong",
        ),
    ] {
        let (router, session) = authenticated_app_with_calendar_failure(failure).await;
        let uri = "/calendar?view=week&date=2026-07-21";
        let full = send(&router, get(uri, &session, false)).await;
        let fragment = send(&router, get(uri, &session, true)).await;

        assert_eq!(full.0, status);
        assert_eq!(fragment.0, status);
        assert!(full.1.contains(heading));
        assert!(fragment.1.contains(heading));
        assert_eq!(
            full.1.matches("data-calendar-view=\"week\"").count(),
            fragment.1.matches("data-calendar-view=\"week\"").count()
        );
        for copy in [heading, "Try again", "No intervention data was changed."] {
            assert!(full.1.contains(copy));
            assert!(fragment.1.contains(copy));
        }
        assert_eq!(full.2.as_deref(), Some("no-store"));
        assert_eq!(fragment.2.as_deref(), Some("no-store"));
        assert_eq!(full.3.as_deref(), Some("HX-Request"));
        assert_eq!(fragment.3.as_deref(), Some("HX-Request"));
        for leaked in ["SurrealDB", "websocket", "database", "RepositoryError"] {
            assert!(!full.1.contains(leaked), "leaked {leaked}");
            assert!(!fragment.1.contains(leaked), "leaked {leaked}");
        }
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            let reference = full
                .1
                .split("Reference: <code>")
                .nth(1)
                .and_then(|value| value.split("</code>").next())
                .expect("safe correlation reference");
            assert!(reference.starts_with("browser-"));
            assert_eq!(reference.len(), "browser-".len() + 16);
            assert!(reference["browser-".len()..]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()));
            assert_eq!(full.4.as_deref(), Some(reference));
            assert!(fragment.4.as_deref().is_some_and(|value| {
                value.starts_with("browser-") && value.len() == "browser-".len() + 16
            }));
        }
    }
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
        .clone()
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
        .starts_with("/login?next=/calendar%3Fview%3Dmonth%26date%3D2026-07-21"));

    let expired_htmx = router
        .oneshot(
            Request::builder()
                .uri("/calendar?view=week&date=2026-07-21")
                .header("HX-Request", "true")
                .body(Body::empty())
                .expect("HTMX expired-session request"),
        )
        .await
        .expect("expired-session HTMX response");
    assert_eq!(expired_htmx.status(), StatusCode::UNAUTHORIZED);
    assert!(expired_htmx.headers()["HX-Redirect"]
        .to_str()
        .expect("login redirect")
        .starts_with("/login?next=/calendar%3Fview%3Dweek%26date%3D2026-07-21"));
    assert_eq!(expired_htmx.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(expired_htmx.headers()[header::VARY], "HX-Request");
}

struct FailingCalendarRepository(RepositoryError);

#[async_trait]
impl CalendarRepository for FailingCalendarRepository {
    async fn entries(&self, _range: &CalendarRange) -> Result<Vec<CalendarEntry>, RepositoryError> {
        Err(self.0)
    }
}

async fn authenticated_app_with_calendar_failure(
    failure: RepositoryError,
) -> (axum::Router, String) {
    let boot = boot_test::<App>().await.expect("application should boot");
    let calendar = boot
        .app_context
        .shared_store
        .get::<CalendarService>()
        .expect("calendar service");
    boot.app_context.shared_store.insert(CalendarService::new(
        Arc::new(FailingCalendarRepository(failure)),
        calendar.workshop_time().clone(),
    ));
    boot.app_context
        .shared_store
        .get::<AuthService>()
        .expect("auth service")
        .create_user("filippo@example.com", "Filippo", PASSWORD)
        .await
        .expect("fixture user");
    let router = boot.router.expect("router");
    let session = authenticated_session(&router, PASSWORD).await;
    (router, session)
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
    let correlation = response
        .headers()
        .get("X-Correlation-ID")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body")
            .to_vec(),
    )
    .expect("UTF-8 response");
    (status, body, cache_control, vary, correlation)
}

type CalendarResponse = (
    StatusCode,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);
