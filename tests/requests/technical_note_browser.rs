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
async fn technical_note_browser_supports_safe_search_forms_context_and_lifecycle() {
    let (router, session, csrf) = authenticated_app().await;
    let empty = text(&router, get("/knowledge", &session, false)).await;
    assert_eq!(empty.0, StatusCode::OK);
    assert!(empty.1.contains("No active technical notes yet"));

    let vehicle = create_vehicle(&router, &session, &csrf, "Golf", "1-ABC-234").await;
    let intervention = write_json(
        &router,
        Method::POST,
        "/api/v1/interventions",
        &session,
        &csrf,
        json!({"vehicle_id": vehicle, "service_date": "2026-07-19T09:00", "estimated_duration_minutes": 60}),
    )
    .await["data"]["id"]
        .as_str()
        .expect("intervention id")
        .to_owned();

    let prefill = text(
        &router,
        get(
            &format!("/knowledge/new?source_intervention={intervention}"),
            &session,
            false,
        ),
    )
    .await;
    assert_eq!(prefill.0, StatusCode::OK, "{}", prefill.1);
    assert!(prefill.1.contains("Volkswagen"));
    assert!(prefill.1.contains("Golf"));
    assert!(prefill.1.contains(&intervention));

    let created = router
        .clone()
        .oneshot(form_request(
            Method::POST,
            "/knowledge",
            &session,
            note_form(
                &csrf,
                "Water pump <procedure>",
                "Use the locking tool.\n<script>alert('workshop')</script>",
                " Cooling \nVW\nvw\nBrakes",
                &vehicle,
                &intervention,
            ),
            false,
        ))
        .await
        .expect("create technical note");
    assert_eq!(created.status(), StatusCode::SEE_OTHER);
    let location = created.headers()[header::LOCATION]
        .to_str()
        .expect("location")
        .to_owned();

    let detail = text(&router, get(&location, &session, false)).await.1;
    assert!(detail.contains("Water pump &lt;procedure&gt;"));
    assert!(detail.contains("&lt;script&gt;alert"));
    assert!(!detail.contains("<script>alert"));
    let cooling = detail.find(">cooling<").expect("cooling chip");
    let vw = detail.find(">vw<").expect("vw chip");
    let brakes = detail.find(">brakes<").expect("brakes chip");
    assert!(cooling < vw && vw < brakes);

    let searched = text(
        &router,
        get(
            "/knowledge?q=water&tags=cooling%0Abrakes&make=VOLKSWAGEN&model=golf",
            &session,
            false,
        ),
    )
    .await;
    assert_eq!(searched.0, StatusCode::OK, "{}", searched.1);
    assert!(searched.1.contains("Water pump &lt;procedure&gt;"));
    assert!(!searched.1.contains("<script>alert"));

    let invalid = text(
        &router,
        form_request(
            Method::POST,
            &format!("{location}/edit"),
            &session,
            note_form(&csrf, " Safe title ", "", "vw\ncooling", &vehicle, ""),
            true,
        ),
    )
    .await;
    assert_eq!(invalid.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(invalid.1.starts_with("<form id=\"knowledge-form\""));
    assert!(invalid.1.contains("value=\" Safe title \""));

    let archived = text(
        &router,
        form_request(
            Method::POST,
            &format!("{location}/archive"),
            &session,
            csrf_form(&csrf),
            false,
        ),
    )
    .await;
    assert_eq!(archived.0, StatusCode::SEE_OTHER);
    let archived_detail = text(&router, get(&location, &session, false)).await.1;
    assert!(archived_detail.contains("Archived technical note"));
    assert!(archived_detail.contains("Restore technical note"));
    assert!(!archived_detail.contains("Edit technical note</a>"));

    let archived_list = text(
        &router,
        get("/knowledge?archived=archived", &session, false),
    )
    .await
    .1;
    assert!(archived_list.contains("Water pump &lt;procedure&gt;"));

    let restored = text(
        &router,
        form_request(
            Method::POST,
            &format!("{location}/restore"),
            &session,
            csrf_form(&csrf),
            true,
        ),
    )
    .await;
    assert_eq!(restored.0, StatusCode::OK);
    assert_eq!(restored.2.as_deref(), Some(location.as_str()));
}

#[tokio::test]
async fn technical_note_browser_source_conflicts_offer_explicit_corrections() {
    let (router, session, csrf) = authenticated_app().await;
    let golf = create_vehicle(&router, &session, &csrf, "Golf", "1-GOLF-1").await;
    let polo = create_vehicle(&router, &session, &csrf, "Polo", "1-POLO-1").await;
    let source = write_json(
        &router,
        Method::POST,
        "/api/v1/interventions",
        &session,
        &csrf,
        json!({"vehicle_id": golf, "service_date": "2026-07-20T09:00", "estimated_duration_minutes": 60}),
    )
    .await["data"]["id"]
        .as_str()
        .expect("source id")
        .to_owned();
    let conflict = text(
        &router,
        form_request(
            Method::POST,
            "/knowledge",
            &session,
            note_form(&csrf, "Conflict", "Safe body", "diagnosis", &polo, &source),
            true,
        ),
    )
    .await;
    assert_eq!(conflict.0, StatusCode::CONFLICT, "{}", conflict.1);
    assert!(conflict.1.contains("Use source vehicle"));
    assert!(conflict.1.contains("Remove source intervention"));
    assert!(conflict.1.contains("Reload latest"));
    assert!(conflict.1.contains("Safe body"));
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

async fn create_vehicle(
    router: &axum::Router,
    session: &str,
    csrf: &str,
    model: &str,
    registration: &str,
) -> String {
    let customer = write_json(
        router,
        Method::POST,
        "/api/v1/customers",
        session,
        csrf,
        json!({"display_name": format!("{model} Owner")}),
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
            "customer_id": customer, "make": "Volkswagen", "model": model,
            "registration": registration, "engine_type": "1.4 TSI"
        }),
    )
    .await["data"]["id"]
        .as_str()
        .expect("vehicle id")
        .to_owned()
}

fn note_form(
    csrf: &str,
    title: &str,
    body: &str,
    tags: &str,
    vehicle: &str,
    source: &str,
) -> String {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in [
        ("_csrf", csrf),
        ("title", title),
        ("body", body),
        ("tags", tags),
        ("make", "Volkswagen"),
        ("model", "Golf"),
        ("engine", "1.4 TSI"),
        ("vehicle_id", vehicle),
        ("source_intervention_id", source),
    ] {
        form.append_pair(key, value);
    }
    form.finish()
}

fn csrf_form(csrf: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("_csrf", csrf)
        .finish()
}

fn get(uri: &str, session: &str, htmx: bool) -> Request<Body> {
    let mut builder = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header(header::COOKIE, session);
    if htmx {
        builder = builder.header("HX-Request", "true");
    }
    builder.body(Body::empty()).expect("GET request")
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
            body.to_string(),
        ))
        .await
        .expect("JSON response");
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("JSON body");
    serde_json::from_slice(&bytes).expect("JSON value")
}

async fn text(
    router: &axum::Router,
    request: Request<Body>,
) -> (StatusCode, String, Option<String>) {
    let response = router.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let redirect = response
        .headers()
        .get("HX-Redirect")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body");
    (
        status,
        String::from_utf8(bytes.to_vec()).expect("UTF-8 response"),
        redirect,
    )
}
