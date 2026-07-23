use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, StatusCode},
};
use loco_rs::testing::request::boot_test;
use pipauto::{app::App, models::auth::AuthenticationModel as AuthService};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::support::{authenticated_csrf, authenticated_json_request, authenticated_session};

const PASSWORD: &str = "Workshop-password-123";

#[tokio::test]
async fn interventions_api_covers_draft_lines_completion_and_history() {
    let (router, session, csrf) = authenticated_app().await;
    let vehicle_id = create_vehicle(&router, &session, &csrf).await;
    let created = write_json(
        &router,
        Method::POST,
        "/api/v1/interventions",
        &session,
        &csrf,
        json!({
            "vehicle_id": vehicle_id,
            "service_date": "2026-07-19T09:00",
            "estimated_duration_minutes": 60,
            "mileage": 100000,
            "performed_work": "Brake inspection"
        }),
    )
    .await;
    assert_eq!(created.0, StatusCode::CREATED);
    assert_eq!(created.1["data"]["status"], "draft");
    let intervention_id = created.1["data"]["id"]
        .as_str()
        .expect("intervention id")
        .to_owned();

    let line = write_json(
        &router,
        Method::POST,
        &format!("/api/v1/interventions/{intervention_id}/lines"),
        &session,
        &csrf,
        json!({
            "category": "part",
            "description": "Brake pads",
            "quantity": "2",
            "unit_label": "set",
            "unit_price_minor": 7000,
            "unit_cost_minor": 4500,
            "position": 0
        }),
    )
    .await;
    assert_eq!(line.0, StatusCode::CREATED);
    assert_eq!(line.1["data"]["totals"]["price"]["minor_units"], 14000);
    let line_id = line.1["data"]["line"]["id"]
        .as_str()
        .expect("line id")
        .to_owned();
    let updated_line = write_json(
        &router,
        Method::PATCH,
        &format!("/api/v1/interventions/{intervention_id}/lines/{line_id}"),
        &session,
        &csrf,
        json!({
            "category": "part",
            "description": "Front brake pads",
            "quantity": "2",
            "unit_label": "set",
            "unit_price_minor": 7500,
            "unit_cost_minor": 4500,
            "position": 0
        }),
    )
    .await;
    assert_eq!(updated_line.0, StatusCode::OK);
    assert_eq!(
        updated_line.1["data"]["totals"]["price"]["minor_units"],
        15000
    );

    let removable = write_json(
        &router,
        Method::POST,
        &format!("/api/v1/interventions/{intervention_id}/lines"),
        &session,
        &csrf,
        json!({
            "category": "other",
            "description": "Temporary charge",
            "quantity": "1",
            "unit_label": "item",
            "unit_price_minor": 100,
            "position": 1
        }),
    )
    .await;
    let removable_id = removable.1["data"]["line"]["id"]
        .as_str()
        .expect("removable line id");
    let removed = write_json(
        &router,
        Method::DELETE,
        &format!("/api/v1/interventions/{intervention_id}/lines/{removable_id}"),
        &session,
        &csrf,
        Value::Null,
    )
    .await;
    assert_eq!(removed.0, StatusCode::OK);
    assert_eq!(removed.1["data"]["totals"]["price"]["minor_units"], 15000);
    assert_eq!(
        get_json(
            &router,
            &format!("/api/v1/interventions/{intervention_id}/lines"),
            &session,
        )
        .await
        .1["data"]
            .as_array()
            .expect("lines")
            .len(),
        1
    );

    let updated = write_json(
        &router,
        Method::PATCH,
        &format!("/api/v1/interventions/{intervention_id}"),
        &session,
        &csrf,
        json!({"notes": "Ready to close"}),
    )
    .await;
    assert_eq!(updated.0, StatusCode::OK);
    assert_eq!(
        get_json(
            &router,
            &format!("/api/v1/interventions/{intervention_id}"),
            &session,
        )
        .await
        .1["data"]["notes"],
        "Ready to close"
    );
    assert_eq!(
        get_json(&router, "/api/v1/interventions", &session).await.1["data"][0]["intervention"]
            ["id"],
        intervention_id
    );

    let completed = write_json(
        &router,
        Method::POST,
        &format!("/api/v1/interventions/{intervention_id}/complete"),
        &session,
        &csrf,
        Value::Null,
    )
    .await;
    assert_eq!(completed.0, StatusCode::OK);
    assert_eq!(completed.1["data"]["status"], "completed");
    assert!(completed.1["data"]["completed_at"].is_string());

    assert_eq!(
        write_json(
            &router,
            Method::PATCH,
            &format!("/api/v1/interventions/{intervention_id}"),
            &session,
            &csrf,
            json!({"notes": "unsupported edit"}),
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        write_json(
            &router,
            Method::DELETE,
            &format!("/api/v1/interventions/{intervention_id}/lines/{line_id}"),
            &session,
            &csrf,
            Value::Null,
        )
        .await
        .0,
        StatusCode::CONFLICT
    );

    let history = get_json(
        &router,
        &format!("/api/v1/vehicles/{vehicle_id}/service-history"),
        &session,
    )
    .await;
    assert_eq!(history.0, StatusCode::OK);
    assert_eq!(history.1["data"][0]["intervention"]["id"], intervention_id);
    assert_eq!(
        history.1["data"][0]["totals"]["price"]["minor_units"],
        15000
    );
}

#[tokio::test]
async fn interventions_api_rejects_archived_work_regressions_and_invalid_transitions() {
    let (router, session, csrf) = authenticated_app().await;
    let vehicle_id = create_vehicle(&router, &session, &csrf).await;

    let first = create_intervention(
        &router,
        &session,
        &csrf,
        &vehicle_id,
        "2026-07-01",
        100000,
        None,
    )
    .await;
    let later = create_intervention(
        &router,
        &session,
        &csrf,
        &vehicle_id,
        "2026-07-20",
        120000,
        Some("Later work"),
    )
    .await;
    assert_eq!(first.0, StatusCode::CREATED);
    assert_eq!(later.0, StatusCode::CREATED);

    let regression = create_intervention(
        &router,
        &session,
        &csrf,
        &vehicle_id,
        "2026-07-10",
        120001,
        None,
    )
    .await;
    assert_eq!(regression.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(regression.1["error"]["fields"]["mileage"].is_array());

    let first_id = first.1["data"]["id"].as_str().expect("id");
    let missing_work = write_json(
        &router,
        Method::POST,
        &format!("/api/v1/interventions/{first_id}/complete"),
        &session,
        &csrf,
        Value::Null,
    )
    .await;
    assert_eq!(missing_work.0, StatusCode::UNPROCESSABLE_ENTITY);

    let cancelled = write_json(
        &router,
        Method::POST,
        &format!("/api/v1/interventions/{first_id}/cancel"),
        &session,
        &csrf,
        Value::Null,
    )
    .await;
    assert_eq!(cancelled.0, StatusCode::OK);
    assert_eq!(cancelled.1["data"]["status"], "cancelled");
    assert_eq!(
        write_json(
            &router,
            Method::POST,
            &format!("/api/v1/interventions/{first_id}/cancel"),
            &session,
            &csrf,
            Value::Null,
        )
        .await
        .0,
        StatusCode::CONFLICT
    );

    write_json(
        &router,
        Method::POST,
        &format!("/api/v1/vehicles/{vehicle_id}/archive"),
        &session,
        &csrf,
        Value::Null,
    )
    .await;
    assert_eq!(
        create_intervention(
            &router,
            &session,
            &csrf,
            &vehicle_id,
            "2026-07-21",
            121000,
            None,
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let history = get_json(
        &router,
        &format!("/api/v1/vehicles/{vehicle_id}/service-history"),
        &session,
    )
    .await;
    assert_eq!(history.0, StatusCode::OK);
    assert!(history.1["data"]
        .as_array()
        .expect("history")
        .iter()
        .any(|entry| entry["intervention"]["status"] == "cancelled"));
}

#[tokio::test]
async fn interventions_preserve_creation_identity_after_customer_and_vehicle_changes() {
    let (router, session, csrf) = authenticated_app().await;
    let owner = write_json(
        &router,
        Method::POST,
        "/api/v1/customers",
        &session,
        &csrf,
        json!({"display_name": "Original Owner"}),
    )
    .await
    .1["data"]["id"]
        .as_str()
        .expect("owner id")
        .to_owned();
    let new_owner = write_json(
        &router,
        Method::POST,
        "/api/v1/customers",
        &session,
        &csrf,
        json!({"display_name": "New Owner"}),
    )
    .await
    .1["data"]["id"]
        .as_str()
        .expect("new owner id")
        .to_owned();
    let vehicle = write_json(
        &router,
        Method::POST,
        "/api/v1/vehicles",
        &session,
        &csrf,
        json!({
            "customer_id": owner,
            "make": "Volkswagen",
            "model": "Golf",
            "registration": "1-ABC-234"
        }),
    )
    .await
    .1["data"]["id"]
        .as_str()
        .expect("vehicle id")
        .to_owned();
    let intervention = write_json(
        &router,
        Method::POST,
        "/api/v1/interventions",
        &session,
        &csrf,
        json!({
            "vehicle_id": vehicle,
            "service_date": "2026-07-22T09:30",
            "estimated_duration_minutes": 120
        }),
    )
    .await;
    assert_eq!(intervention.0, StatusCode::CREATED);
    let intervention_id = intervention.1["data"]["id"]
        .as_str()
        .expect("intervention id");
    let original_snapshot = intervention.1["data"]["customer_snapshot"].clone();
    let original_vehicle_snapshot = intervention.1["data"]["vehicle_snapshot"].clone();
    assert_eq!(
        intervention.1["data"]["service_date"],
        "2026-07-22T07:30:00Z"
    );
    assert_eq!(intervention.1["data"]["estimated_duration_minutes"], 120);

    assert_eq!(
        write_json(
            &router,
            Method::PATCH,
            &format!("/api/v1/customers/{owner}"),
            &session,
            &csrf,
            json!({"display_name": "Renamed Owner"}),
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        write_json(
            &router,
            Method::PATCH,
            &format!("/api/v1/vehicles/{vehicle}"),
            &session,
            &csrf,
            json!({
                "customer_id": new_owner,
                "make": "Peugeot",
                "model": "208",
                "registration": "2-XYZ-789"
            }),
        )
        .await
        .0,
        StatusCode::OK
    );

    let returned = get_json(
        &router,
        &format!("/api/v1/interventions/{intervention_id}"),
        &session,
    )
    .await;
    assert_eq!(returned.1["data"]["customer_snapshot"], original_snapshot);
    assert_eq!(
        returned.1["data"]["vehicle_snapshot"],
        original_vehicle_snapshot
    );
}

#[tokio::test]
async fn interventions_api_enforces_complete_unambiguous_workshop_scheduling() {
    let (router, session, csrf) = authenticated_app().await;
    let vehicle_id = create_vehicle(&router, &session, &csrf).await;

    for body in [
        json!({
            "vehicle_id": vehicle_id,
            "service_date": "2026-07-22T09:30"
        }),
        json!({
            "vehicle_id": vehicle_id,
            "estimated_duration_minutes": 60
        }),
    ] {
        assert_eq!(
            write_json(
                &router,
                Method::POST,
                "/api/v1/interventions",
                &session,
                &csrf,
                body,
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }

    for (service_date, message) in [
        ("2026-03-29T02:30", "does not exist"),
        ("2026-10-25T02:30", "occurs twice"),
    ] {
        let invalid = write_json(
            &router,
            Method::POST,
            "/api/v1/interventions",
            &session,
            &csrf,
            json!({
                "vehicle_id": vehicle_id,
                "service_date": service_date,
                "estimated_duration_minutes": 60
            }),
        )
        .await;
        assert_eq!(invalid.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(invalid.1["error"]["fields"]["service_date"][0]
            .as_str()
            .is_some_and(|value| value.contains(message)));
    }

    for service_date in [
        "2026-07-22",
        "2026-07-22T09:30:00",
        "2026-07-22T09:30+02:00",
    ] {
        let invalid = write_json(
            &router,
            Method::POST,
            "/api/v1/interventions",
            &session,
            &csrf,
            json!({
                "vehicle_id": vehicle_id,
                "service_date": service_date,
                "estimated_duration_minutes": 60
            }),
        )
        .await;
        assert_eq!(invalid.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(invalid.1["error"]["fields"]["service_date"].is_array());
    }

    for duration in [0, 45, 1470] {
        let invalid = write_json(
            &router,
            Method::POST,
            "/api/v1/interventions",
            &session,
            &csrf,
            json!({
                "vehicle_id": vehicle_id,
                "service_date": "2026-07-22T09:30",
                "estimated_duration_minutes": duration
            }),
        )
        .await;
        assert_eq!(invalid.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(invalid.1["error"]["fields"]["estimated_duration_minutes"].is_array());
    }

    let created = write_json(
        &router,
        Method::POST,
        "/api/v1/interventions",
        &session,
        &csrf,
        json!({
            "vehicle_id": vehicle_id,
            "service_date": "2026-07-22T09:30",
            "estimated_duration_minutes": 120,
            "performed_work": "Schedule verification"
        }),
    )
    .await;
    assert_eq!(created.0, StatusCode::CREATED);
    let id = created.1["data"]["id"].as_str().expect("intervention id");
    let preserved = write_json(
        &router,
        Method::PATCH,
        &format!("/api/v1/interventions/{id}"),
        &session,
        &csrf,
        json!({"notes": "Schedule retained"}),
    )
    .await;
    assert_eq!(preserved.0, StatusCode::OK);
    assert_eq!(preserved.1["data"]["service_date"], "2026-07-22T07:30:00Z");
    assert_eq!(preserved.1["data"]["estimated_duration_minutes"], 120);

    assert_eq!(
        write_json(
            &router,
            Method::POST,
            &format!("/api/v1/interventions/{id}/complete"),
            &session,
            &csrf,
            Value::Null,
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        write_json(
            &router,
            Method::PATCH,
            &format!("/api/v1/interventions/{id}"),
            &session,
            &csrf,
            json!({
                "service_date": "2026-07-22T10:00",
                "estimated_duration_minutes": 60
            }),
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn intervention_local_date_filters_cover_complete_workshop_dates() {
    let (router, session, csrf) = authenticated_app().await;
    let vehicle_id = create_vehicle(&router, &session, &csrf).await;
    let mut ids = Vec::new();
    for service_date in ["2026-03-28T23:30", "2026-03-29T23:30", "2026-03-30T00:00"] {
        let created = write_json(
            &router,
            Method::POST,
            "/api/v1/interventions",
            &session,
            &csrf,
            json!({
                "vehicle_id": vehicle_id,
                "service_date": service_date,
                "estimated_duration_minutes": 60
            }),
        )
        .await;
        assert_eq!(created.0, StatusCode::CREATED);
        ids.push(created.1["data"]["id"].as_str().expect("id").to_owned());
    }

    let filtered = get_json(
        &router,
        "/api/v1/interventions?service_date_from=2026-03-29&service_date_to=2026-03-29",
        &session,
    )
    .await;
    assert_eq!(filtered.0, StatusCode::OK);
    let returned = filtered.1["data"].as_array().expect("interventions");
    assert_eq!(returned.len(), 1);
    assert_eq!(returned[0]["intervention"]["id"], ids[1]);
    assert!(returned[0]["intervention"]["customer_snapshot"].is_object());
    assert!(returned[0]["intervention"]["vehicle_snapshot"].is_object());

    let reversed = get_json(
        &router,
        "/api/v1/interventions?service_date_from=2026-03-30&service_date_to=2026-03-29",
        &session,
    )
    .await;
    assert_eq!(reversed.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(reversed.1["error"]["fields"]["service_date_to"].is_array());
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
        json!({"display_name": "Owner"}),
    )
    .await
    .1["data"]["id"]
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
    .await
    .1["data"]["id"]
        .as_str()
        .expect("vehicle id")
        .to_owned()
}

async fn create_intervention(
    router: &axum::Router,
    session: &str,
    csrf: &str,
    vehicle_id: &str,
    service_date: &str,
    mileage: u64,
    performed_work: Option<&str>,
) -> (StatusCode, Value) {
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
    .await
}

async fn get_json(router: &axum::Router, uri: &str, session: &str) -> (StatusCode, Value) {
    response(
        router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(uri)
                    .header("Cookie", session)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response"),
    )
    .await
}

async fn write_json(
    router: &axum::Router,
    method: Method,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
) -> (StatusCode, Value) {
    response(
        router
            .clone()
            .oneshot(authenticated_json_request(
                method,
                uri,
                session,
                csrf,
                body.to_string(),
            ))
            .await
            .expect("response"),
    )
    .await
}

async fn response(response: axum::response::Response) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&bytes)}));
    (status, value)
}
