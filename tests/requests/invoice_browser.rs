use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
};
use loco_rs::testing::request::boot_test;
use pipauto::{app::App, models::auth::AuthenticationModel as AuthService};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::support::{
    authenticated_csrf, authenticated_json_request, authenticated_session, TEST_ORIGIN,
};

const PASSWORD: &str = "Workshop-password-123";

#[tokio::test]
async fn invoice_draft_browser_relationships_cursor_and_unnumbered_state() {
    let (router, session, csrf) = authenticated_app().await;
    let first = customer(&router, &session, &csrf, "Mario Rossi").await;
    let second = customer(&router, &session, &csrf, "Giulia Bianchi").await;
    let vehicle = write_json(
        &router,
        Method::POST,
        "/api/v1/vehicles",
        &session,
        &csrf,
        json!({
            "customer_id": first,
            "make": "Fiat",
            "model": "Panda",
            "registration": "1-INV-060"
        }),
    )
    .await;
    let vehicle_id = id(&vehicle);

    let prefill = send(
        &router,
        get(
            &format!("/invoices/new?vehicle={vehicle_id}"),
            &session,
            false,
        ),
    )
    .await;
    assert_eq!(prefill.0, StatusCode::OK, "{}", prefill.1);
    assert!(prefill.1.contains("New invoice draft"));
    assert!(prefill.1.contains(&format!("value=\"{first}\" selected")));
    assert!(prefill.1.contains(&format!("value=\"{vehicle_id}\"")));
    assert!(prefill.1.contains("value=\"EUR\" readonly"));
    assert!(!prefill.1.contains("name=\"due_date\""));
    assert!(!prefill.1.contains("name=\"number\""));

    let created = router
        .clone()
        .oneshot(form_request(
            Method::POST,
            "/invoices",
            &session,
            invoice_form(
                &csrf,
                &first,
                &vehicle_id,
                "",
                "EUR",
                "Preserved workshop note",
            ),
            false,
        ))
        .await
        .expect("create invoice draft");
    assert_eq!(created.status(), StatusCode::SEE_OTHER);
    let location = created.headers()[header::LOCATION]
        .to_str()
        .expect("location")
        .to_owned();
    let detail = send(&router, get(&location, &session, false)).await;
    assert_eq!(detail.0, StatusCode::OK, "{}", detail.1);
    assert!(detail.1.contains("Invoice draft"));
    assert!(detail.1.contains("Unnumbered until issuance"));
    assert!(detail.1.contains("Preserved workshop note"));
    assert!(detail.1.contains("EUR 0.00"));
    assert!(!detail.1.contains("Record payment"));
    assert!(!detail.1.contains("Issue date"));
    assert!(!detail.1.contains("Due date"));

    let conflict = send(
        &router,
        form_request(
            Method::POST,
            "/invoices",
            &session,
            invoice_form(
                &csrf,
                &second,
                &vehicle_id,
                "",
                "EUR",
                "Safe relationship data",
            ),
            true,
        ),
    )
    .await;
    assert_eq!(conflict.0, StatusCode::CONFLICT, "{}", conflict.1);
    assert!(conflict.1.starts_with("<form id=\"invoice-form\""));
    assert!(conflict.1.contains("Your selections were preserved"));
    assert!(conflict.1.contains(&format!("value=\"{second}\" selected")));
    assert!(conflict.1.contains(&format!("value=\"{vehicle_id}\"")));
    assert!(conflict.1.contains("Safe relationship data"));

    let currency = send(
        &router,
        form_request(
            Method::POST,
            "/invoices",
            &session,
            invoice_form(&csrf, &first, "", "", "USD", "Currency safe note"),
            true,
        ),
    )
    .await;
    assert_eq!(currency.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(currency.1.contains("authoritative workshop currency"));
    assert!(currency.1.contains("Currency safe note"));

    write_json(
        &router,
        Method::POST,
        &format!("/api/v1/customers/{second}/archive"),
        &session,
        &csrf,
        json!(null),
    )
    .await;
    let archived = send(
        &router,
        form_request(
            Method::POST,
            "/invoices",
            &session,
            invoice_form(&csrf, &second, "", "", "EUR", "Archived safe note"),
            true,
        ),
    )
    .await;
    assert_eq!(archived.0, StatusCode::CONFLICT);
    assert!(archived.1.contains("Archived safe note"));

    let list = send(&router, get("/invoices?status=draft", &session, false)).await;
    assert_eq!(list.0, StatusCode::OK, "{}", list.1);
    assert!(list.1.contains("Draft"));
    assert!(list.1.contains("Preserved workshop note") || list.1.contains("Mario Rossi"));
    assert!(!list.1.contains("name=\"q\""));
    assert!(!list.1.contains("name=\"payment"));
    assert!(!list.1.contains("name=\"customer"));

    let malformed = send(
        &router,
        get("/invoices?status=draft&cursor=not%20opaque", &session, true),
    )
    .await;
    assert_eq!(malformed.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(malformed.1.contains("Check the invoice filters"));
}

#[tokio::test]
async fn invoice_line_browser_authoritative_totals_sources_and_order() {
    let (router, session, csrf) = authenticated_app().await;
    let customer = customer(&router, &session, &csrf, "Line Owner").await;
    let vehicle = write_json(
        &router,
        Method::POST,
        "/api/v1/vehicles",
        &session,
        &csrf,
        json!({"customer_id": customer, "make": "Ford", "model": "Focus"}),
    )
    .await;
    let vehicle_id = id(&vehicle);
    let intervention = write_json(
        &router,
        Method::POST,
        "/api/v1/interventions",
        &session,
        &csrf,
        json!({"vehicle_id": vehicle_id, "service_date": "2026-07-20T09:00", "estimated_duration_minutes": 60}),
    )
    .await;
    let intervention_id = id(&intervention);
    let source = write_json(
        &router,
        Method::POST,
        &format!("/api/v1/interventions/{intervention_id}/lines"),
        &session,
        &csrf,
        json!({
            "category": "labour",
            "description": "Source labour",
            "quantity": "1",
            "unit_label": "hour",
            "unit_price_minor": 1000,
            "position": 0
        }),
    )
    .await;
    let source_id = source["data"]["line"]["id"]
        .as_str()
        .expect("source line id");
    let draft = write_json(
        &router,
        Method::POST,
        "/api/v1/invoices",
        &session,
        &csrf,
        json!({
            "customer_id": customer,
            "vehicle_id": vehicle_id,
            "intervention_id": intervention_id
        }),
    )
    .await;
    let invoice_id = id(&draft);
    let line_url = format!("/invoices/{invoice_id}/lines");

    let form_page = send(&router, get(&format!("{line_url}/new"), &session, false)).await;
    assert!(form_page.1.contains("Source labour"));
    assert!(form_page.1.contains("Currency EUR"));
    assert!(!form_page.1.contains("name=\"currency\""));
    assert!(!form_page.1.contains("name=\"line_total\""));

    for (description, source, position, price) in [
        ("First invoice line", source_id, "10", "10.01"),
        ("Second invoice line", "", "20", "3.50"),
    ] {
        let created = send(
            &router,
            form_request(
                Method::POST,
                &line_url,
                &session,
                invoice_line_form(&csrf, source, description, "1.005", "hour", price, position),
                false,
            ),
        )
        .await;
        assert_eq!(created.0, StatusCode::SEE_OTHER, "{}", created.1);
    }

    let lines = read_json(
        &router,
        &format!("/api/v1/invoices/{invoice_id}/lines"),
        &session,
    )
    .await;
    let second_id = lines["data"][1]["id"].as_str().expect("second id");
    let detail = send(
        &router,
        get(&format!("/invoices/{invoice_id}"), &session, false),
    )
    .await;
    assert!(detail.1.contains("EUR 10.06"));
    assert!(detail.1.contains("EUR 3.52"));
    assert!(detail.1.contains("EUR 13.58"));
    assert!(detail.1.contains("Move up"));

    let moved = send(
        &router,
        form_request(
            Method::POST,
            &format!("/invoices/{invoice_id}/lines/{second_id}/move-up"),
            &session,
            csrf_only(&csrf),
            false,
        ),
    )
    .await;
    assert_eq!(moved.0, StatusCode::SEE_OTHER, "{}", moved.1);
    let reordered = send(
        &router,
        get(&format!("/invoices/{invoice_id}"), &session, false),
    )
    .await;
    assert!(reordered.1.find("Second invoice line") < reordered.1.find("First invoice line"));
    assert!(reordered.1.contains("EUR 13.58"));

    let invalid = send(
        &router,
        form_request(
            Method::POST,
            &line_url,
            &session,
            invoice_line_form(
                &csrf,
                source_id,
                "Preserved & safe line",
                "0",
                "hour",
                "1.999",
                "not-a-position",
            ),
            true,
        ),
    )
    .await;
    assert_eq!(invalid.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(invalid.1.starts_with("<form id=\"invoice-line-form\""));
    for value in ["Preserved &amp; safe line", "1.999", "not-a-position"] {
        assert!(invalid.1.contains(value), "missing {value}");
    }
}

#[tokio::test]
async fn invoice_lifecycle_browser_issues_once_locks_snapshots_and_retains_void_history() {
    let (router, session, csrf) = authenticated_app().await;
    let (invoice_id, customer_id) =
        invoice_ready_to_issue(&router, &session, &csrf, "Lifecycle Owner").await;

    let confirmation = send(
        &router,
        get(&format!("/invoices/{invoice_id}/issue"), &session, false),
    )
    .await;
    assert_eq!(confirmation.0, StatusCode::OK, "{}", confirmation.1);
    for expected in [
        "Lifecycle Owner",
        "Line count</dt><dd>1",
        "EUR 125.00",
        "Issue and lock invoice",
        "cannot return to Draft",
    ] {
        assert!(confirmation.1.contains(expected), "missing {expected}");
    }

    let issued = send(
        &router,
        form_request(
            Method::POST,
            &format!("/invoices/{invoice_id}/issue"),
            &session,
            issue_invoice_form(&csrf, "2026-07-20", "2026-08-20"),
            false,
        ),
    )
    .await;
    assert_eq!(issued.0, StatusCode::SEE_OTHER, "{}", issued.1);

    write_json(
        &router,
        Method::PATCH,
        &format!("/api/v1/customers/{customer_id}"),
        &session,
        &csrf,
        json!({"display_name": "Changed source customer"}),
    )
    .await;
    let detail = send(
        &router,
        get(&format!("/invoices/{invoice_id}"), &session, false),
    )
    .await;
    assert_eq!(detail.0, StatusCode::OK, "{}", detail.1);
    assert!(detail.1.contains("Invoice 2026-"));
    assert!(detail.1.contains("Lifecycle Owner"));
    assert!(detail.1.contains("Workshopstraat 61"));
    assert!(detail.1.contains("2026-07-20"));
    assert!(detail.1.contains("2026-08-20"));
    for forbidden in ["Edit header", "Add line", ">Edit<", ">Remove<"] {
        assert!(!detail.1.contains(forbidden), "found {forbidden}");
    }

    let repeat = send(
        &router,
        form_request(
            Method::POST,
            &format!("/invoices/{invoice_id}/issue"),
            &session,
            issue_invoice_form(&csrf, "2026-07-20", ""),
            true,
        ),
    )
    .await;
    assert_eq!(repeat.0, StatusCode::CONFLICT, "{}", repeat.1);
    assert!(repeat.1.contains("no invoice number was requested again"));
    assert!(repeat.1.contains("Invoice 2026-"));

    let voided = send(
        &router,
        form_request(
            Method::POST,
            &format!("/invoices/{invoice_id}/void"),
            &session,
            void_invoice_form(&csrf, "Customer cancelled duplicate work"),
            false,
        ),
    )
    .await;
    assert_eq!(voided.0, StatusCode::SEE_OTHER, "{}", voided.1);
    let detail = send(
        &router,
        get(&format!("/invoices/{invoice_id}"), &session, false),
    )
    .await;
    assert!(detail.1.contains("Void record"));
    assert!(detail.1.contains("Customer cancelled duplicate work"));
    assert!(detail.1.contains("Invoice 2026-"));
    assert!(!detail.1.contains("Record payment"));
    assert!(detail.1.contains("Invoice export is unavailable"));
    assert!(!detail.1.contains("type=\"submit\">Export"));
}

#[tokio::test]
async fn payment_browser_is_append_only_balance_aware_and_removes_ineligible_actions() {
    let (router, session, csrf) = authenticated_app().await;
    let (invoice_id, _) = invoice_ready_to_issue(&router, &session, &csrf, "Payment Owner").await;
    send(
        &router,
        form_request(
            Method::POST,
            &format!("/invoices/{invoice_id}/issue"),
            &session,
            issue_invoice_form(&csrf, "2026-07-20", ""),
            false,
        ),
    )
    .await;

    let form = send(
        &router,
        get(
            &format!("/invoices/{invoice_id}/payments/new"),
            &session,
            false,
        ),
    )
    .await;
    assert_eq!(form.0, StatusCode::OK, "{}", form.1);
    for expected in [
        "EUR 125.00",
        "Currency is fixed to EUR",
        "Cash",
        "Bank transfer",
        "Card",
        "Other",
        "append-only",
        "never retried automatically",
    ] {
        assert!(form.1.contains(expected), "missing {expected}");
    }
    assert!(!form.1.contains("name=\"currency\""));

    let overpayment = send(
        &router,
        form_request(
            Method::POST,
            &format!("/invoices/{invoice_id}/payments"),
            &session,
            payment_form(
                &csrf,
                "126.00",
                "2026-07-20T12:30",
                "card",
                "OVER",
                "Preserve me",
            ),
            true,
        ),
    )
    .await;
    assert_eq!(overpayment.0, StatusCode::CONFLICT, "{}", overpayment.1);
    assert!(overpayment
        .1
        .contains("Latest outstanding balance: EUR 125.00"));
    assert!(overpayment.1.contains("value=\"126.00\""));
    assert!(overpayment.1.contains("Preserve me"));

    let partial = send(
        &router,
        form_request(
            Method::POST,
            &format!("/invoices/{invoice_id}/payments"),
            &session,
            payment_form(
                &csrf,
                "25.00",
                "2026-07-20T12:30",
                "bank_transfer",
                "TRX-61",
                "Deposit",
            ),
            false,
        ),
    )
    .await;
    assert_eq!(partial.0, StatusCode::SEE_OTHER, "{}", partial.1);
    let detail = send(
        &router,
        get(&format!("/invoices/{invoice_id}"), &session, false),
    )
    .await;
    for expected in [
        "Partially paid",
        "EUR 25.00",
        "EUR 100.00",
        "Bank transfer",
        "TRX-61",
        "Deposit",
        "by Filippo",
    ] {
        assert!(detail.1.contains(expected), "missing {expected}");
    }
    assert!(!detail.1.contains("Void invoice"));
    assert!(!detail.1.contains("Edit payment"));
    assert!(!detail.1.contains("Delete payment"));

    let void_attempt = send(
        &router,
        get(&format!("/invoices/{invoice_id}/void"), &session, false),
    )
    .await;
    assert_eq!(void_attempt.0, StatusCode::SEE_OTHER);

    let paid = send(
        &router,
        form_request(
            Method::POST,
            &format!("/invoices/{invoice_id}/payments"),
            &session,
            payment_form(&csrf, "100.00", "2026-07-20T13:00", "cash", "", "Balance"),
            false,
        ),
    )
    .await;
    assert_eq!(paid.0, StatusCode::SEE_OTHER, "{}", paid.1);
    let detail = send(
        &router,
        get(&format!("/invoices/{invoice_id}"), &session, false),
    )
    .await;
    assert!(detail.1.contains(">Paid<"));
    assert!(detail.1.contains("EUR 0.00"));
    assert!(!detail.1.contains("Record payment"));
}

async fn invoice_ready_to_issue(
    router: &axum::Router,
    session: &str,
    csrf: &str,
    owner: &str,
) -> (String, String) {
    let customer = write_json(
        router,
        Method::POST,
        "/api/v1/customers",
        session,
        csrf,
        json!({
            "display_name": owner,
            "address": {
                "line_1": "Workshopstraat 61",
                "postal_code": "9000",
                "city": "Gent",
                "country_code": "BE"
            }
        }),
    )
    .await;
    let customer_id = id(&customer);
    let draft = write_json(
        router,
        Method::POST,
        "/api/v1/invoices",
        session,
        csrf,
        json!({"customer_id": customer_id}),
    )
    .await;
    let invoice_id = id(&draft);
    write_json(
        router,
        Method::POST,
        &format!("/api/v1/invoices/{invoice_id}/lines"),
        session,
        csrf,
        json!({
            "description": "Lifecycle labour",
            "quantity": "1",
            "unit_label": "job",
            "unit_price_minor": 12500,
            "position": 0
        }),
    )
    .await;
    (invoice_id, customer_id)
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
    let value = write_json(
        router,
        Method::POST,
        "/api/v1/customers",
        session,
        csrf,
        json!({"display_name": name}),
    )
    .await;
    id(&value)
}

fn id(value: &Value) -> String {
    value["data"]["id"].as_str().expect("record id").to_owned()
}

fn invoice_form(
    csrf: &str,
    customer: &str,
    vehicle: &str,
    intervention: &str,
    currency: &str,
    notes: &str,
) -> String {
    let mut body = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in [
        ("_csrf", csrf),
        ("customer_id", customer),
        ("vehicle_id", vehicle),
        ("intervention_id", intervention),
        ("currency", currency),
        ("notes", notes),
    ] {
        body.append_pair(key, value);
    }
    body.finish()
}

#[allow(clippy::too_many_arguments)]
fn invoice_line_form(
    csrf: &str,
    source: &str,
    description: &str,
    quantity: &str,
    unit_label: &str,
    unit_price: &str,
    position: &str,
) -> String {
    let mut body = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in [
        ("_csrf", csrf),
        ("source_intervention_line_id", source),
        ("description", description),
        ("quantity", quantity),
        ("unit_label", unit_label),
        ("unit_price", unit_price),
        ("position", position),
    ] {
        body.append_pair(key, value);
    }
    body.finish()
}

fn issue_invoice_form(csrf: &str, issue_date: &str, due_date: &str) -> String {
    encoded_form(&[
        ("_csrf", csrf),
        ("issue_date", issue_date),
        ("due_date", due_date),
    ])
}

fn payment_form(
    csrf: &str,
    amount: &str,
    received_at: &str,
    method: &str,
    reference: &str,
    notes: &str,
) -> String {
    encoded_form(&[
        ("_csrf", csrf),
        ("amount", amount),
        ("received_at", received_at),
        ("method", method),
        ("reference", reference),
        ("notes", notes),
    ])
}

fn void_invoice_form(csrf: &str, reason: &str) -> String {
    encoded_form(&[("_csrf", csrf), ("reason", reason)])
}

fn encoded_form(values: &[(&str, &str)]) -> String {
    let mut body = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in values {
        body.append_pair(key, value);
    }
    body.finish()
}

fn csrf_only(csrf: &str) -> String {
    let mut body = url::form_urlencoded::Serializer::new(String::new());
    body.append_pair("_csrf", csrf);
    body.finish()
}

fn get(uri: &str, session: &str, htmx: bool) -> Request<Body> {
    let mut request = Request::builder().uri(uri).header(header::COOKIE, session);
    if htmx {
        request = request.header("HX-Request", "true");
    }
    request.body(Body::empty()).expect("GET request")
}

fn form_request(
    method: Method,
    uri: &str,
    session: &str,
    body: String,
    htmx: bool,
) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::COOKIE, session)
        .header(header::ORIGIN, TEST_ORIGIN);
    if htmx {
        request = request.header("HX-Request", "true");
    }
    request.body(Body::from(body)).expect("form request")
}

async fn send(router: &axum::Router, request: Request<Body>) -> (StatusCode, String) {
    let response = router.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, String::from_utf8(body.to_vec()).expect("UTF-8"))
}

async fn write_json(
    router: &axum::Router,
    method: Method,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
) -> Value {
    let response = router
        .clone()
        .oneshot(authenticated_json_request(
            method,
            uri,
            session,
            csrf,
            Body::from(body.to_string()),
        ))
        .await
        .expect("JSON response");
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("JSON body");
    serde_json::from_slice(&bytes).expect("JSON")
}

async fn read_json(router: &axum::Router, uri: &str, session: &str) -> Value {
    let response = router
        .clone()
        .oneshot(get(uri, session, false))
        .await
        .expect("JSON response");
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("JSON body");
    serde_json::from_slice(&bytes).expect("JSON")
}
