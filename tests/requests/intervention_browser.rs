use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
};
use loco_rs::testing::request::boot_test;
use pipauto::{app::App, services::auth::AuthService};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::support::{
    authenticated_csrf, authenticated_json_request, authenticated_session, TEST_ORIGIN,
};

const PASSWORD: &str = "Workshop-password-123";

#[tokio::test]
async fn intervention_browser_draft_completion_and_read_only_history() {
    let (router, session, csrf) = authenticated_app().await;
    let vehicle_id = vehicle_fixture(&router, &session, &csrf).await;

    let new_page = send(
        &router,
        get(
            &format!("/vehicles/{vehicle_id}/interventions/new"),
            &session,
            false,
        ),
    )
    .await;
    assert_eq!(new_page.0, StatusCode::OK, "{}", new_page.1);
    assert!(new_page.1.contains("New intervention"));
    assert!(new_page.1.contains("Currency: <strong>EUR</strong>"));

    let create = router
        .clone()
        .oneshot(form_request(
            Method::POST,
            &format!("/vehicles/{vehicle_id}/interventions"),
            &session,
            intervention_form(
                &csrf,
                "2026-07-18",
                "126400",
                "Grinding under braking",
                "Front pads worn",
                "",
                "Inspect discs next service",
                "Keep exact notes",
            ),
            false,
        ))
        .await
        .expect("create draft");
    assert_eq!(create.status(), StatusCode::SEE_OTHER);
    let location = create.headers()[header::LOCATION]
        .to_str()
        .expect("location")
        .to_owned();

    let detail = send(&router, get(&location, &session, false)).await;
    assert_eq!(detail.0, StatusCode::OK, "{}", detail.1);
    for value in [
        "Grinding under braking",
        "Front pads worn",
        "Inspect discs next service",
        "Keep exact notes",
        "126400 km",
        "EUR 0.00",
    ] {
        assert!(detail.1.contains(value), "missing {value}");
    }
    assert!(detail.1.contains("Edit details"));

    let intervention_id = location.trim_start_matches("/interventions/");
    let missing_work = send(
        &router,
        form_request(
            Method::POST,
            &format!("{location}/complete"),
            &session,
            csrf_only(&csrf),
            true,
        ),
    )
    .await;
    assert_eq!(missing_work.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(missing_work
        .1
        .contains("Record the work performed before completion."));
    assert!(missing_work.1.starts_with("<article"));

    let edit = send(
        &router,
        form_request(
            Method::POST,
            &format!("{location}/edit"),
            &session,
            intervention_form(
                &csrf,
                "2026-07-18",
                "126400",
                "Grinding under braking",
                "Front pads worn",
                "Replaced front pads",
                "Inspect discs next service",
                "Keep exact notes",
            ),
            false,
        ),
    )
    .await;
    assert_eq!(edit.0, StatusCode::SEE_OTHER);

    let confirmation = send(
        &router,
        get(&format!("{location}/complete"), &session, false),
    )
    .await;
    assert_eq!(confirmation.0, StatusCode::OK);
    assert!(confirmation.1.contains("Complete and lock intervention"));
    assert!(confirmation.1.contains("Completion cannot be undone"));
    assert!(confirmation.1.contains("Replaced front pads"));

    let completed = router
        .clone()
        .oneshot(form_request(
            Method::POST,
            &format!("{location}/complete"),
            &session,
            csrf_only(&csrf),
            false,
        ))
        .await
        .expect("complete");
    assert_eq!(completed.status(), StatusCode::SEE_OTHER);

    let old_edit = send(&router, get(&format!("{location}/edit"), &session, false)).await;
    assert_eq!(old_edit.0, StatusCode::OK);
    assert!(old_edit.1.contains("authoritative read-only state"));
    assert!(!old_edit.1.contains("Save changes"));
    assert!(!old_edit.1.contains("Add line item"));
    assert!(old_edit.1.contains("Create invoice draft"));

    let stale = send(
        &router,
        form_request(
            Method::POST,
            &format!("/interventions/{intervention_id}/complete"),
            &session,
            csrf_only(&csrf),
            true,
        ),
    )
    .await;
    assert_eq!(stale.0, StatusCode::CONFLICT);
    assert!(stale.1.contains("transition was not repeated"));
    assert!(stale.1.contains("Completed"));

    let list = send(
        &router,
        get(
            &format!(
                "/interventions?vehicle={vehicle_id}&status=completed&from=2026-07-01&to=2026-07-31"
            ),
            &session,
            false,
        ),
    )
    .await;
    assert_eq!(list.0, StatusCode::OK, "{}", list.1);
    assert!(list.1.contains("Replaced front pads"));
    assert!(!list.1.contains("name=\"q\""));
    assert!(!list.1.contains("name=\"customer\""));

    let cancellable = write_json(
        &router,
        Method::POST,
        "/api/v1/interventions",
        &session,
        &csrf,
        json!({
            "vehicle_id": vehicle_id,
            "service_date": "2026-07-19",
            "mileage": 126400,
            "customer_reported_problem": "Cancelled booking"
        }),
    )
    .await;
    let cancellable_id = cancellable["data"]["id"].as_str().expect("draft id");
    let cancellation = send(
        &router,
        get(
            &format!("/interventions/{cancellable_id}/cancel"),
            &session,
            false,
        ),
    )
    .await;
    assert_eq!(cancellation.0, StatusCode::OK);
    assert!(cancellation.1.contains("remains visible as Cancelled"));
    assert!(!cancellation.1.contains("name=\"reason\""));
    let cancelled = send(
        &router,
        form_request(
            Method::POST,
            &format!("/interventions/{cancellable_id}/cancel"),
            &session,
            csrf_only(&csrf),
            false,
        ),
    )
    .await;
    assert_eq!(cancelled.0, StatusCode::SEE_OTHER);
    let cancelled_detail = send(
        &router,
        get(&format!("/interventions/{cancellable_id}"), &session, false),
    )
    .await;
    assert!(cancelled_detail.1.contains("Cancelled"));
    assert!(!cancelled_detail.1.contains("Create invoice draft"));
}

#[tokio::test]
async fn intervention_browser_validation_and_chronology_preserve_fragment_values() {
    let (router, session, csrf) = authenticated_app().await;
    let vehicle_id = vehicle_fixture(&router, &session, &csrf).await;

    let invalid = send(
        &router,
        form_request(
            Method::POST,
            &format!("/vehicles/{vehicle_id}/interventions"),
            &session,
            intervention_form(
                &csrf,
                "not-a-date",
                "12.5",
                "Safe submitted problem",
                "",
                "",
                "",
                "Safe submitted notes",
            ),
            true,
        ),
    )
    .await;
    assert_eq!(invalid.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(invalid.1.starts_with("<form id=\"intervention-form\""));
    assert!(invalid.1.contains("value=\"not-a-date\""));
    assert!(invalid.1.contains("value=\"12.5\""));
    assert!(invalid.1.contains("Safe submitted problem"));
    assert!(invalid.1.contains("Safe submitted notes"));

    write_json(
        &router,
        Method::POST,
        "/api/v1/interventions",
        &session,
        &csrf,
        json!({
            "vehicle_id": vehicle_id,
            "service_date": "2026-07-20",
            "mileage": 120000,
            "performed_work": "Later dated work"
        }),
    )
    .await;

    let chronology = send(
        &router,
        form_request(
            Method::POST,
            &format!("/vehicles/{vehicle_id}/interventions"),
            &session,
            intervention_form(
                &csrf,
                "2026-07-10",
                "120001",
                "Preserved chronology problem",
                "",
                "",
                "",
                "Preserved chronology note",
            ),
            true,
        ),
    )
    .await;
    assert_eq!(chronology.0, StatusCode::CONFLICT, "{}", chronology.1);
    assert!(chronology
        .1
        .contains("does not fit the vehicle&#x27;s dated service history"));
    assert!(chronology.1.contains("value=\"2026-07-10\""));
    assert!(chronology.1.contains("value=\"120001\""));
    assert!(chronology.1.contains("Preserved chronology problem"));
    assert!(chronology
        .1
        .contains("Review this vehicle’s complete service history"));

    let malformed = send(
        &router,
        get("/interventions?cursor=not%20opaque", &session, true),
    )
    .await;
    assert_eq!(malformed.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(malformed.1.starts_with("<section"));
    assert!(malformed.1.contains("Check the intervention filters"));

    let missing = send(&router, get("/interventions/not%20an%20id", &session, true)).await;
    assert_eq!(missing.0, StatusCode::NOT_FOUND);
    assert!(missing.1.contains("requested intervention was not found"));
}

async fn authenticated_app() -> (axum::Router, String, String) {
    let boot = boot_test::<App>().await.expect("application should boot");
    boot.app_context
        .shared_store
        .get::<AuthService>()
        .expect("auth service")
        .create_user("filippo@example.com", "Filippo", PASSWORD)
        .await
        .expect("user fixture");
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
        json!({"display_name": "Intervention Owner"}),
    )
    .await;
    let customer_id = customer["data"]["id"].as_str().expect("customer id");
    let vehicle = write_json(
        router,
        Method::POST,
        "/api/v1/vehicles",
        session,
        csrf,
        json!({
            "customer_id": customer_id,
            "make": "Volkswagen",
            "model": "Golf",
            "registration": "1-INT-057",
            "current_mileage": 126400
        }),
    )
    .await;
    vehicle["data"]["id"]
        .as_str()
        .expect("vehicle id")
        .to_owned()
}

#[allow(clippy::too_many_arguments)]
fn intervention_form(
    csrf: &str,
    service_date: &str,
    mileage: &str,
    problem: &str,
    diagnostics: &str,
    work: &str,
    recommendations: &str,
    notes: &str,
) -> String {
    let mut body = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in [
        ("_csrf", csrf),
        ("service_date", service_date),
        ("mileage", mileage),
        ("customer_reported_problem", problem),
        ("diagnostics", diagnostics),
        ("performed_work", work),
        ("recommendations", recommendations),
        ("notes", notes),
    ] {
        body.append_pair(key, value);
    }
    body.finish()
}

fn csrf_only(csrf: &str) -> String {
    let mut body = url::form_urlencoded::Serializer::new(String::new());
    body.append_pair("_csrf", csrf);
    body.finish()
}

fn form_request(
    method: Method,
    uri: &str,
    session: &str,
    body: String,
    htmx: bool,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, session)
        .header(header::ORIGIN, TEST_ORIGIN);
    if htmx {
        builder = builder.header("HX-Request", "true");
    }
    builder.body(Body::from(body)).expect("form request")
}

fn get(uri: &str, session: &str, htmx: bool) -> Request<Body> {
    let mut builder = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header(header::COOKIE, session);
    if htmx {
        builder = builder.header("HX-Request", "true");
    }
    builder.body(Body::empty()).expect("get request")
}

async fn send(router: &axum::Router, request: Request<Body>) -> (StatusCode, String) {
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("request should complete");
    let status = response.status();
    let body = String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body")
            .to_vec(),
    )
    .expect("UTF-8 body");
    (status, body)
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
            .expect("JSON body"),
    )
    .expect("JSON response")
}
