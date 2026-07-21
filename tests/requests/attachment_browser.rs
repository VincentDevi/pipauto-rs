use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
    response::Response,
};
use loco_rs::testing::request::boot_test;
use pipauto::{app::App, services::auth::AuthService};
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::support::{
    authenticated_csrf, authenticated_json_request, authenticated_session, TEST_ORIGIN,
};

const PASSWORD: &str = "Workshop-password-123";
const BOUNDARY: &str = "pipauto-vin-68-boundary";

#[tokio::test]
async fn vehicle_attachment_browser_uploads_edits_reads_and_respects_archive_lock() {
    let (router, session, csrf) = authenticated_app().await;
    let vehicle = vehicle_fixture(&router, &session, &csrf, "1-VIN-068").await;

    let form = html(
        &router,
        get(&format!("/vehicles/{vehicle}/attachments/new"), &session),
    )
    .await;
    assert_eq!(form.0, StatusCode::OK);
    for expected in [
        "enctype=\"multipart/form-data\"",
        "type=\"file\"",
        "up to 25 MiB",
        "detects its type and size from the file content",
    ] {
        assert!(form.1.contains(expected), "missing {expected}");
    }
    assert!(!form.1.contains("name=\"media_type\""));
    assert!(!form.1.contains("name=\"byte_size\""));

    let invalid = html(
        &router,
        multipart(
            &format!("/vehicles/{vehicle}/attachments"),
            &session,
            &csrf,
            "safe display name",
            "safe caption",
            b"not an approved file",
            true,
        ),
    )
    .await;
    assert_eq!(invalid.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(invalid.1.contains("safe display name"));
    assert!(invalid.1.contains("safe caption"));
    assert!(invalid.1.contains("Please reselect it"));
    assert!(!invalid.1.contains("not an approved file"));

    let created = response(
        &router,
        multipart(
            &format!("/vehicles/{vehicle}/attachments"),
            &session,
            &csrf,
            "Inspection photo",
            "Before repair",
            &jpeg_bytes(),
            false,
        ),
    )
    .await;
    assert_eq!(created.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        created.headers()[header::LOCATION],
        format!("/vehicles/{vehicle}")
    );

    let attachment = first_attachment(&router, &session, &vehicle, false).await;
    let attachment_id = attachment["id"].as_str().expect("attachment id");
    let detail = html(&router, get(&format!("/vehicles/{vehicle}"), &session))
        .await
        .1;
    for expected in [
        "Inspection photo",
        "image&#x2F;jpeg",
        "4 bytes",
        "Open",
        "Download",
        "Edit details",
    ] {
        assert!(detail.contains(expected), "missing {expected}");
    }
    assert!(!detail.contains("METADATA ONLY"));

    let open = response(
        &router,
        get(&format!("/attachments/{attachment_id}/content"), &session),
    )
    .await;
    assert_eq!(open.status(), StatusCode::OK);
    assert_eq!(open.headers()[header::CONTENT_TYPE], "image/jpeg");
    assert!(open.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .expect("disposition")
        .starts_with("inline"));
    let download = response(
        &router,
        get(&format!("/attachments/{attachment_id}/download"), &session),
    )
    .await;
    assert!(download.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .expect("disposition")
        .starts_with("attachment"));

    let edited = response(
        &router,
        urlencoded(
            Method::POST,
            &format!("/attachments/{attachment_id}/edit"),
            &session,
            &csrf,
            &[
                ("display_name", "Inspection before repair"),
                ("caption", "Updated caption"),
            ],
        ),
    )
    .await;
    assert_eq!(edited.status(), StatusCode::SEE_OTHER);

    write_json(
        &router,
        Method::POST,
        &format!("/api/v1/vehicles/{vehicle}/archive"),
        &session,
        &csrf,
        Value::Null,
    )
    .await;
    let archived = html(&router, get(&format!("/vehicles/{vehicle}"), &session))
        .await
        .1;
    assert!(archived.contains("Inspection before repair"));
    assert!(archived.contains("Open"));
    assert!(archived.contains("Download"));
    assert!(!archived.contains("Upload attachment"));
    assert!(!archived.contains("Edit details"));
    assert!(!archived.contains("Delete attachment"));

    write_json(
        &router,
        Method::POST,
        &format!("/api/v1/vehicles/{vehicle}/restore"),
        &session,
        &csrf,
        Value::Null,
    )
    .await;
    let deleted = response(
        &router,
        urlencoded(
            Method::POST,
            &format!("/attachments/{attachment_id}/delete"),
            &session,
            &csrf,
            &[],
        ),
    )
    .await;
    assert_eq!(deleted.status(), StatusCode::SEE_OTHER);
    assert!(
        first_attachment_optional(&router, &session, &vehicle, false)
            .await
            .is_none()
    );
}

#[tokio::test]
async fn intervention_attachment_browser_uploads_and_keeps_terminal_files_read_only() {
    let (router, session, csrf) = authenticated_app().await;
    let vehicle = vehicle_fixture(&router, &session, &csrf, "2-VIN-068").await;
    let intervention = write_json(
        &router,
        Method::POST,
        "/api/v1/interventions",
        &session,
        &csrf,
        json!({
            "vehicle_id": vehicle,
            "service_date": "2026-07-21",
            "mileage": 100_000,
            "performed_work": "Attachment browser verification"
        }),
    )
    .await["data"]["id"]
        .as_str()
        .expect("intervention id")
        .to_owned();

    let created = response(
        &router,
        multipart(
            &format!("/interventions/{intervention}/attachments"),
            &session,
            &csrf,
            "Brake photo",
            "Before replacement",
            &jpeg_bytes(),
            true,
        ),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    assert_eq!(
        created.headers()["HX-Redirect"],
        format!("/interventions/{intervention}")
    );

    let attachment = first_attachment(&router, &session, &intervention, true).await;
    assert_eq!(attachment["storage_state"], "stored");
    let draft = html(
        &router,
        get(&format!("/interventions/{intervention}"), &session),
    )
    .await
    .1;
    for expected in [
        "Brake photo",
        "Open",
        "Download",
        "Edit details",
        "Delete attachment",
    ] {
        assert!(draft.contains(expected), "missing {expected}");
    }

    write_json(
        &router,
        Method::POST,
        &format!("/api/v1/interventions/{intervention}/complete"),
        &session,
        &csrf,
        Value::Null,
    )
    .await;
    let terminal = html(
        &router,
        get(&format!("/interventions/{intervention}"), &session),
    )
    .await
    .1;
    assert!(terminal.contains("Brake photo"));
    assert!(terminal.contains("Open"));
    assert!(terminal.contains("Download"));
    assert!(!terminal.contains("Upload attachment"));
    assert!(!terminal.contains("Edit details"));
    assert!(!terminal.contains("Delete attachment"));
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

async fn vehicle_fixture(
    router: &axum::Router,
    session: &str,
    csrf: &str,
    registration: &str,
) -> String {
    let customer = write_json(
        router,
        Method::POST,
        "/api/v1/customers",
        session,
        csrf,
        json!({"display_name": "VIN-68 owner"}),
    )
    .await;
    write_json(
        router,
        Method::POST,
        "/api/v1/vehicles",
        session,
        csrf,
        json!({
            "customer_id": customer["data"]["id"],
            "make": "Volkswagen",
            "model": "Golf",
            "registration": registration,
            "current_mileage": 100_000
        }),
    )
    .await["data"]["id"]
        .as_str()
        .expect("vehicle id")
        .to_owned()
}

fn multipart(
    uri: &str,
    session: &str,
    csrf: &str,
    display_name: &str,
    caption: &str,
    bytes: &[u8],
    htmx: bool,
) -> Request<Body> {
    let mut body = Vec::new();
    for (name, value) in [("display_name", display_name), ("caption", caption)] {
        body.extend_from_slice(format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    }
    body.extend_from_slice(format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"workshop.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    let mut request = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::COOKIE, session)
        .header(header::ORIGIN, TEST_ORIGIN)
        .header("X-CSRF-Token", csrf)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        );
    if htmx {
        request = request.header("HX-Request", "true");
    }
    request.body(Body::from(body)).expect("multipart request")
}

fn urlencoded(
    method: Method,
    uri: &str,
    session: &str,
    csrf: &str,
    fields: &[(&str, &str)],
) -> Request<Body> {
    let mut body = url::form_urlencoded::Serializer::new(String::new());
    body.append_pair("_csrf", csrf);
    for (name, value) in fields {
        body.append_pair(name, value);
    }
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, session)
        .header(header::ORIGIN, TEST_ORIGIN)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body.finish()))
        .expect("form request")
}

fn get(uri: &str, session: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(header::COOKIE, session)
        .body(Body::empty())
        .expect("get request")
}

async fn first_attachment(
    router: &axum::Router,
    session: &str,
    owner: &str,
    intervention: bool,
) -> Value {
    let kind = if intervention {
        "interventions"
    } else {
        "vehicles"
    };
    let response = response(
        router,
        get(&format!("/api/v1/{kind}/{owner}/attachments"), session),
    )
    .await;
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("JSON body");
    serde_json::from_slice::<Value>(&body).expect("JSON response")["data"][0].clone()
}

async fn first_attachment_optional(
    router: &axum::Router,
    session: &str,
    owner: &str,
    intervention: bool,
) -> Option<Value> {
    let kind = if intervention {
        "interventions"
    } else {
        "vehicles"
    };
    let response = response(
        router,
        get(&format!("/api/v1/{kind}/{owner}/attachments"), session),
    )
    .await;
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("JSON body");
    serde_json::from_slice::<Value>(&body).expect("JSON response")["data"]
        .as_array()
        .and_then(|items| items.first().cloned())
}

async fn write_json(
    router: &axum::Router,
    method: Method,
    uri: &str,
    session: &str,
    csrf: &str,
    value: Value,
) -> Value {
    let response = response(
        router,
        authenticated_json_request(method, uri, session, csrf, value.to_string()),
    )
    .await;
    assert!(response.status().is_success(), "request failed: {uri}");
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("JSON body");
    serde_json::from_slice(&body).expect("JSON response")
}

async fn html(router: &axum::Router, request: Request<Body>) -> (StatusCode, String) {
    let response = response(router, request).await;
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("HTML body");
    (
        status,
        String::from_utf8(body.to_vec()).expect("UTF-8 HTML"),
    )
}

async fn response(router: &axum::Router, request: Request<Body>) -> Response {
    router.clone().oneshot(request).await.expect("request")
}

fn jpeg_bytes() -> Vec<u8> {
    vec![0xff, 0xd8, 0xff, 0xe0]
}
