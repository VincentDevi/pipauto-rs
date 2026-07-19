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
async fn invoices_api_covers_drafts_snapshots_payments_and_immutable_transitions() {
    let (router, session, csrf) = authenticated_app().await;
    let customer = write_json(
        &router,
        Method::POST,
        "/api/v1/customers",
        &session,
        &csrf,
        json!({
            "display_name": "Mario Rossi",
            "address": {
                "line_1": "Via Roma 1",
                "postal_code": "10100",
                "city": "Torino",
                "country_code": "IT"
            }
        }),
    )
    .await;
    let customer_id = id(&customer.1);
    let vehicle = write_json(
        &router,
        Method::POST,
        "/api/v1/vehicles",
        &session,
        &csrf,
        json!({"customer_id": customer_id, "make": "Fiat", "model": "Panda"}),
    )
    .await;
    let vehicle_id = id(&vehicle.1);

    let draft = write_json(
        &router,
        Method::POST,
        "/api/v1/invoices",
        &session,
        &csrf,
        json!({"customer_id": customer_id, "vehicle_id": vehicle_id, "currency": "EUR"}),
    )
    .await;
    assert_eq!(draft.0, StatusCode::CREATED);
    assert_eq!(draft.1["data"]["status"], "draft");
    assert_eq!(draft.1["data"]["number"], Value::Null);
    let invoice_id = id(&draft.1);

    let draft_payment = write_json(
        &router,
        Method::POST,
        &format!("/api/v1/invoices/{invoice_id}/payments"),
        &session,
        &csrf,
        payment(1, "EUR"),
    )
    .await;
    assert_eq!(draft_payment.0, StatusCode::CONFLICT);

    let line = write_json(
        &router,
        Method::POST,
        &format!("/api/v1/invoices/{invoice_id}/lines"),
        &session,
        &csrf,
        json!({
            "description": "Workshop labour",
            "quantity": "1.5",
            "unit_label": "hour",
            "unit_price_minor": 101,
            "position": 0
        }),
    )
    .await;
    assert_eq!(line.0, StatusCode::CREATED);
    assert_eq!(line.1["data"]["subtotal"]["minor_units"], 152);
    let line_id = line.1["data"]["line"]["id"]
        .as_str()
        .expect("line id")
        .to_owned();

    let issued = write_json(
        &router,
        Method::POST,
        &format!("/api/v1/invoices/{invoice_id}/issue"),
        &session,
        &csrf,
        json!({"issue_date": "2026-07-19", "due_date": "2026-08-19"}),
    )
    .await;
    assert_eq!(issued.0, StatusCode::OK, "{}", issued.1);
    assert_eq!(issued.1["data"]["status"], "issued");
    assert_eq!(issued.1["data"]["payment_status"], "unpaid");
    assert_eq!(issued.1["data"]["customer_display_snapshot"], "Mario Rossi");
    assert!(issued.1["data"]["number"]
        .as_str()
        .is_some_and(|number| number.starts_with("2026-")));

    assert_eq!(
        write_json(
            &router,
            Method::POST,
            &format!("/api/v1/invoices/{invoice_id}/issue"),
            &session,
            &csrf,
            json!({"issue_date": "2026-07-19"}),
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        write_json(
            &router,
            Method::PATCH,
            &format!("/api/v1/invoices/{invoice_id}/lines/{line_id}"),
            &session,
            &csrf,
            json!({
                "description": "Changed",
                "quantity": "1",
                "unit_label": "hour",
                "unit_price_minor": 1,
                "position": 0
            }),
        )
        .await
        .0,
        StatusCode::CONFLICT
    );

    write_json(
        &router,
        Method::PATCH,
        &format!("/api/v1/customers/{customer_id}"),
        &session,
        &csrf,
        json!({"display_name": "Changed Source Customer"}),
    )
    .await;
    let reloaded = get_json(&router, &format!("/api/v1/invoices/{invoice_id}"), &session).await;
    assert_eq!(
        reloaded.1["data"]["customer_display_snapshot"],
        "Mario Rossi"
    );
    assert_eq!(
        reloaded.1["data"]["billing_address_snapshot"]["city"],
        "Torino"
    );

    assert_eq!(
        write_json(
            &router,
            Method::POST,
            &format!("/api/v1/invoices/{invoice_id}/payments"),
            &session,
            &csrf,
            payment(10, "USD"),
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let partial = write_json(
        &router,
        Method::POST,
        &format!("/api/v1/invoices/{invoice_id}/payments"),
        &session,
        &csrf,
        payment(52, "EUR"),
    )
    .await;
    assert_eq!(partial.0, StatusCode::CREATED, "{}", partial.1);
    assert_eq!(
        partial.1["data"]["invoice"]["payment_status"],
        "partially_paid"
    );
    assert_eq!(
        partial.1["data"]["invoice"]["outstanding"]["minor_units"],
        100
    );
    let payment_id = partial.1["data"]["payment"]["id"]
        .as_str()
        .expect("payment id")
        .to_owned();
    assert_eq!(
        get_json(&router, &format!("/api/v1/payments/{payment_id}"), &session)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        write_json(
            &router,
            Method::POST,
            &format!("/api/v1/invoices/{invoice_id}/payments"),
            &session,
            &csrf,
            payment(101, "EUR"),
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let paid = write_json(
        &router,
        Method::POST,
        &format!("/api/v1/invoices/{invoice_id}/payments"),
        &session,
        &csrf,
        payment(100, "EUR"),
    )
    .await;
    assert_eq!(paid.1["data"]["invoice"]["payment_status"], "paid");
    assert_eq!(paid.1["data"]["invoice"]["outstanding"]["minor_units"], 0);
    assert_eq!(
        write_json(
            &router,
            Method::POST,
            &format!("/api/v1/invoices/{invoice_id}/void"),
            &session,
            &csrf,
            json!({"reason": "Paid invoice correction"}),
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn invoices_api_rejects_invalid_relationships_and_empty_issue() {
    let (router, session, csrf) = authenticated_app().await;
    let first = customer(&router, &session, &csrf, "First").await;
    let second = customer(&router, &session, &csrf, "Second").await;
    let vehicle = write_json(
        &router,
        Method::POST,
        "/api/v1/vehicles",
        &session,
        &csrf,
        json!({"customer_id": second, "make": "Ford", "model": "Focus"}),
    )
    .await;
    assert_eq!(
        write_json(
            &router,
            Method::POST,
            "/api/v1/invoices",
            &session,
            &csrf,
            json!({"customer_id": first, "vehicle_id": id(&vehicle.1)}),
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let draft = write_json(
        &router,
        Method::POST,
        "/api/v1/invoices",
        &session,
        &csrf,
        json!({"customer_id": second}),
    )
    .await;
    assert_eq!(
        write_json(
            &router,
            Method::POST,
            &format!("/api/v1/invoices/{}/issue", id(&draft.1)),
            &session,
            &csrf,
            json!({"issue_date": "2026-07-19"}),
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}

fn payment(amount: i64, currency: &str) -> Value {
    json!({
        "amount_minor": amount,
        "currency": currency,
        "received_at": "2026-07-19T12:00:00Z",
        "method": "card"
    })
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

async fn customer(router: &axum::Router, session: &str, csrf: &str, name: &str) -> String {
    let response = write_json(
        router,
        Method::POST,
        "/api/v1/customers",
        session,
        csrf,
        json!({"display_name": name}),
    )
    .await;
    id(&response.1)
}

fn id(body: &Value) -> String {
    body["data"]["id"].as_str().expect("resource id").to_owned()
}

async fn write_json(
    router: &axum::Router,
    method: Method,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
) -> (StatusCode, Value) {
    let request =
        authenticated_json_request(method, uri, session, csrf, Body::from(body.to_string()));
    response_json(router, request).await
}

async fn get_json(router: &axum::Router, uri: &str, session: &str) -> (StatusCode, Value) {
    let request = Request::builder()
        .uri(uri)
        .header(header::COOKIE, session)
        .body(Body::empty())
        .expect("request");
    response_json(router, request).await
}

async fn response_json(router: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(request)
        .await
        .expect("request should complete");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    let value = serde_json::from_slice(&body).unwrap_or_else(|_| {
        panic!(
            "response should be JSON: {}",
            String::from_utf8_lossy(&body)
        )
    });
    (status, value)
}
