use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
};
use loco_rs::testing::request::boot_test;
use pipauto::{
    app::App, database::client::AppDatabase, models::auth::AuthenticationModel as AuthService,
    settings::MAX_ATTACHMENT_FILE_BYTES,
};
use serde_json::{json, Value};
use surrealdb::types::RecordId;
use tower::ServiceExt;

use crate::support::{
    authenticated_csrf, authenticated_json_request, authenticated_session, TEST_ORIGIN,
};

const PASSWORD: &str = "Workshop-password-123";

#[tokio::test]
async fn technical_notes_api_supports_crud_search_archive_and_source_conflicts() {
    let (router, session, csrf) = authenticated_app().await;
    let first_vehicle = create_vehicle(&router, &session, &csrf, "Golf").await;
    let second_vehicle = create_vehicle(&router, &session, &csrf, "Polo").await;
    let intervention = write_json(
        &router,
        Method::POST,
        "/api/v1/interventions",
        &session,
        &csrf,
        json!({
            "vehicle_id": first_vehicle, "service_date": "2026-07-19T09:00",
            "estimated_duration_minutes": 60
        }),
    )
    .await
    .1["data"]["id"]
        .as_str()
        .expect("intervention id")
        .to_owned();

    let conflict = write_json(
        &router,
        Method::POST,
        "/api/v1/technical-notes",
        &session,
        &csrf,
        json!({
            "title": "Mismatch", "body": "Conflicting sources", "vehicle_id": second_vehicle,
            "source_intervention_id": intervention
        }),
    )
    .await;
    assert_eq!(conflict.0, StatusCode::CONFLICT);

    let created = write_json(
        &router,
        Method::POST,
        "/api/v1/technical-notes",
        &session,
        &csrf,
        json!({
            "title": "Water pump replacement", "body": "Use the locking tool", "tags": ["Cooling"],
            "vehicle_id": first_vehicle, "source_intervention_id": intervention,
            "make": "Volkswagen", "model": "Golf", "engine": "1.4 TSI"
        }),
    )
    .await;
    assert_eq!(created.0, StatusCode::CREATED);
    let id = created.1["data"]["id"]
        .as_str()
        .expect("note id")
        .to_owned();
    assert_eq!(
        get_json(&router, &format!("/api/v1/technical-notes/{id}"), &session)
            .await
            .0,
        StatusCode::OK
    );

    let searched = get_json(
        &router,
        "/api/v1/technical-notes?q=water&tags=cooling&make=VOLKSWAGEN&model=golf&engine=1.4%20TSI",
        &session,
    )
    .await;
    assert_eq!(searched.0, StatusCode::OK);
    assert_eq!(searched.1["data"][0]["id"], id);

    let updated = write_json(&router, Method::PATCH, &format!("/api/v1/technical-notes/{id}"), &session, &csrf, json!({
        "title": "Water pump procedure", "body": "Lock the crankshaft first", "tags": ["cooling"],
        "make": "Volkswagen", "model": "Golf", "engine": "1.4 TSI"
    })).await;
    assert_eq!(updated.0, StatusCode::OK);
    assert_eq!(updated.1["data"]["title"], "Water pump procedure");

    assert_eq!(
        write_json(
            &router,
            Method::POST,
            &format!("/api/v1/technical-notes/{id}/archive"),
            &session,
            &csrf,
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
    assert!(get_json(&router, "/api/v1/technical-notes", &session)
        .await
        .1["data"]
        .as_array()
        .expect("notes")
        .is_empty());
    assert_eq!(
        get_json(&router, "/api/v1/technical-notes?archived=all", &session)
            .await
            .1["data"][0]["id"],
        id
    );
    assert_eq!(
        write_json(
            &router,
            Method::POST,
            &format!("/api/v1/technical-notes/{id}/restore"),
            &session,
            &csrf,
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn technical_note_attachments_api_supports_all_owners_and_transport_safe_patch() {
    let (router, session, csrf) = authenticated_app().await;
    let vehicle = create_vehicle(&router, &session, &csrf, "Golf").await;
    let intervention = write_json(
        &router,
        Method::POST,
        "/api/v1/interventions",
        &session,
        &csrf,
        json!({"vehicle_id": vehicle, "service_date": "2026-07-21T09:00", "estimated_duration_minutes": 60}),
    )
    .await
    .1["data"]["id"]
        .as_str()
        .expect("intervention id")
        .to_owned();
    let note = write_json(
        &router,
        Method::POST,
        "/api/v1/technical-notes",
        &session,
        &csrf,
        json!({"title": "Stored procedure", "body": "Use the locking tool"}),
    )
    .await
    .1["data"]["id"]
        .as_str()
        .expect("technical note id")
        .to_owned();

    let owners = [
        (
            format!("/api/v1/vehicles/{vehicle}/attachments"),
            "vehicle",
            "vehicle_id",
            vehicle.as_str(),
        ),
        (
            format!("/api/v1/interventions/{intervention}/attachments"),
            "intervention",
            "intervention_id",
            intervention.as_str(),
        ),
        (
            format!("/api/v1/technical-notes/{note}/attachments"),
            "technical_note",
            "technical_note_id",
            note.as_str(),
        ),
    ];
    let mut ids = Vec::new();
    for (uri, owner_type, owner_field, owner_id) in owners {
        let created = multipart_json(
            &router,
            &uri,
            Some(&session),
            Some(&csrf),
            vec![
                TestPart::file("file", "proof.jpg", jpeg_bytes()),
                TestPart::text("display_name", "Water pump.jpg"),
                TestPart::text("caption", "Before replacement"),
            ],
        )
        .await;
        assert_eq!(created.0, StatusCode::CREATED);
        assert_eq!(created.1["data"]["owner_type"], owner_type);
        assert_eq!(created.1["data"][owner_field], owner_id);
        assert_eq!(created.1["data"]["storage_state"], "stored");
        assert_eq!(created.1["data"]["media_type"], "image/jpeg");
        assert_eq!(created.1["data"]["byte_size"], 4);
        let serialized = created.1.to_string();
        for private in ["sha256", "bucket", "key", "pointer", "digest"] {
            assert!(!serialized.contains(private));
        }
        ids.push(
            created.1["data"]["id"]
                .as_str()
                .expect("attachment id")
                .to_owned(),
        );
        assert_eq!(
            get_json(&router, &uri, &session).await.1["data"]
                .as_array()
                .map(Vec::len),
            Some(1)
        );
    }

    let id = &ids[0];
    let shown = get_json(&router, &format!("/api/v1/attachments/{id}"), &session).await;
    assert_eq!(shown.0, StatusCode::OK);
    assert_eq!(
        shown.1["data"]["content_url"],
        format!("/api/v1/attachments/{id}/content")
    );
    assert_eq!(
        shown.1["data"]["download_url"],
        format!("/api/v1/attachments/{id}/download")
    );

    let immutable = write_json(
        &router,
        Method::PATCH,
        &format!("/api/v1/attachments/{id}"),
        &session,
        &csrf,
        json!({"media_type": "application/pdf"}),
    )
    .await;
    assert_eq!(immutable.0, StatusCode::BAD_REQUEST);
    let updated = write_json(
        &router,
        Method::PATCH,
        &format!("/api/v1/attachments/{id}"),
        &session,
        &csrf,
        json!({"display_name": "Water pump before.jpg", "caption": null}),
    )
    .await;
    assert_eq!(updated.0, StatusCode::OK);
    assert_eq!(updated.1["data"]["caption"], Value::Null);

    assert_eq!(
        write_json(
            &router,
            Method::DELETE,
            &format!("/api/v1/attachments/{id}"),
            &session,
            &csrf,
            Value::Null
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        get_json(&router, &format!("/api/v1/attachments/{id}"), &session)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn attachment_security_multipart_rejects_unsafe_edges_and_enforces_25_mib_boundary() {
    let (router, session, csrf) = authenticated_app().await;
    let vehicle = create_vehicle(&router, &session, &csrf, "Boundary").await;
    let uri = format!("/api/v1/vehicles/{vehicle}/attachments");

    let unauthenticated = multipart_json(
        &router,
        &uri,
        None,
        Some(&csrf),
        vec![TestPart::file("file", "proof.jpg", jpeg_bytes())],
    )
    .await;
    assert_eq!(unauthenticated.0, StatusCode::UNAUTHORIZED);

    let missing_csrf = multipart_json(
        &router,
        &uri,
        Some(&session),
        None,
        vec![TestPart::file("file", "proof.jpg", jpeg_bytes())],
    )
    .await;
    assert_eq!(missing_csrf.0, StatusCode::FORBIDDEN);

    for parts in [
        vec![
            TestPart::file("file", "one.jpg", jpeg_bytes()),
            TestPart::file("file", "two.jpg", jpeg_bytes()),
        ],
        vec![
            TestPart::file("file", "proof.jpg", jpeg_bytes()),
            TestPart::text("unknown", "value"),
        ],
        vec![
            TestPart::file("file", "proof.jpg", jpeg_bytes()),
            TestPart::text("display_name", "one"),
            TestPart::text("display_name", "two"),
        ],
        vec![TestPart::file("file", "empty.jpg", Vec::new())],
    ] {
        assert_eq!(
            multipart_json(&router, &uri, Some(&session), Some(&csrf), parts)
                .await
                .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    let invalid_text = multipart_json(
        &router,
        &uri,
        Some(&session),
        Some(&csrf),
        vec![
            TestPart::file("file", "proof.jpg", jpeg_bytes()),
            TestPart::bytes("caption", vec![0xff]),
        ],
    )
    .await;
    assert_eq!(invalid_text.0, StatusCode::BAD_REQUEST);

    let form_csrf = multipart_json(
        &router,
        &uri,
        Some(&session),
        None,
        vec![
            TestPart::file("file", "form-token.jpg", jpeg_bytes()),
            TestPart::text("_csrf", &csrf),
        ],
    )
    .await;
    assert_eq!(form_csrf.0, StatusCode::CREATED);

    let mismatch = multipart_json(
        &router,
        &uri,
        Some(&session),
        Some(&csrf),
        vec![
            TestPart::file("file", "mismatch.jpg", jpeg_bytes()),
            TestPart::text("_csrf", "wrong"),
        ],
    )
    .await;
    assert_eq!(mismatch.0, StatusCode::FORBIDDEN);

    let duplicate_form_csrf = multipart_json(
        &router,
        &uri,
        Some(&session),
        None,
        vec![
            TestPart::file("file", "duplicate-token.jpg", jpeg_bytes()),
            TestPart::text("_csrf", &csrf),
            TestPart::text("_csrf", &csrf),
        ],
    )
    .await;
    assert_eq!(duplicate_form_csrf.0, StatusCode::FORBIDDEN);

    let mut nested = TestPart::bytes("caption", b"nested".to_vec());
    nested.content_type = Some("multipart/mixed; boundary=nested");
    assert_eq!(
        multipart_json(
            &router,
            &uri,
            Some(&session),
            Some(&csrf),
            vec![TestPart::file("file", "proof.jpg", jpeg_bytes()), nested],
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );

    let malformed = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(&uri)
                .header(header::CONTENT_TYPE, "multipart/form-data; boundary=broken")
                .header(header::COOKIE, &session)
                .header(header::ORIGIN, TEST_ORIGIN)
                .header("X-CSRF-Token", &csrf)
                .body(Body::from("--broken\r\nnot-a-header"))
                .expect("malformed request"),
        )
        .await
        .expect("malformed response");
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);

    let mut maximum = vec![0_u8; MAX_ATTACHMENT_FILE_BYTES];
    maximum[..8].copy_from_slice(b"%PDF-1.7");
    let accepted = multipart_json(
        &router,
        &uri,
        Some(&session),
        Some(&csrf),
        vec![TestPart::file("file", "maximum.pdf", maximum)],
    )
    .await;
    assert_eq!(accepted.0, StatusCode::CREATED);
    assert_eq!(accepted.1["data"]["byte_size"], MAX_ATTACHMENT_FILE_BYTES);

    let mut oversized = vec![0_u8; MAX_ATTACHMENT_FILE_BYTES + 1];
    oversized[..8].copy_from_slice(b"%PDF-1.7");
    assert_eq!(
        multipart_json(
            &router,
            &uri,
            Some(&session),
            Some(&csrf),
            vec![TestPart::file("file", "oversized.pdf", oversized)],
        )
        .await
        .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
}

#[tokio::test]
async fn attachment_security_content_is_authenticated_exact_and_corruption_safe() {
    let (router, session, csrf, database) = authenticated_app_with_database().await;
    let vehicle = create_vehicle(&router, &session, &csrf, "Content").await;
    let uri = format!("/api/v1/vehicles/{vehicle}/attachments");
    let bytes = b"%PDF-1.7\nproof".to_vec();
    let created = multipart_json(
        &router,
        &uri,
        Some(&session),
        Some(&csrf),
        vec![
            TestPart::file("file", "ignored.bin", bytes.clone()),
            TestPart::text("display_name", "Rapport été.pdf"),
        ],
    )
    .await;
    let id = created.1["data"]["id"]
        .as_str()
        .expect("attachment id")
        .to_owned();

    let unauthenticated = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/attachments/{id}/content"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

    let content = authenticated_get(
        &router,
        &format!("/api/v1/attachments/{id}/content"),
        &session,
    )
    .await;
    assert_eq!(content.status(), StatusCode::OK);
    assert_eq!(content.headers()[header::CONTENT_TYPE], "application/pdf");
    assert_eq!(
        content.headers()[header::CONTENT_LENGTH],
        bytes.len().to_string()
    );
    assert_eq!(
        content.headers()[header::CACHE_CONTROL],
        "private, no-store"
    );
    assert_eq!(content.headers()["X-Content-Type-Options"], "nosniff");
    let disposition = content.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .expect("disposition");
    assert!(disposition.starts_with("inline;"));
    assert!(disposition.contains("filename*=UTF-8''Rapport%20%C3%A9t%C3%A9.pdf"));
    assert_eq!(
        to_bytes(content.into_body(), usize::MAX)
            .await
            .expect("body")
            .as_ref(),
        bytes
    );

    let download = authenticated_get(
        &router,
        &format!("/api/v1/attachments/{id}/download"),
        &session,
    )
    .await;
    assert!(download.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .expect("disposition")
        .starts_with("attachment;"));

    let client = database.client().expect("database client");
    client
        .query("RETURN file::delete((SELECT VALUE file FROM ONLY $record));")
        .bind(("record", RecordId::new("attachment", id.clone())))
        .await
        .expect("delete object query")
        .check()
        .expect("delete object");
    let corrupt = authenticated_get(
        &router,
        &format!("/api/v1/attachments/{id}/content"),
        &session,
    )
    .await;
    assert_eq!(corrupt.status(), StatusCode::SERVICE_UNAVAILABLE);
    let corrupt_body = to_bytes(corrupt.into_body(), usize::MAX)
        .await
        .expect("body");
    assert!(!corrupt_body.is_empty());
}

async fn authenticated_app() -> (axum::Router, String, String) {
    let (router, session, csrf, _) = authenticated_app_with_database().await;
    (router, session, csrf)
}

async fn authenticated_app_with_database() -> (axum::Router, String, String, AppDatabase) {
    let boot = boot_test::<App>().await.expect("application should boot");
    boot.app_context
        .shared_store
        .get::<AuthService>()
        .expect("auth service")
        .create_user("filippo@example.com", "Filippo", PASSWORD)
        .await
        .expect("fixture user");
    let database = boot
        .app_context
        .shared_store
        .get::<AppDatabase>()
        .expect("application database");
    let router = boot.router.expect("router");
    let session = authenticated_session(&router, PASSWORD).await;
    let csrf = authenticated_csrf(&router, &session).await;
    (router, session, csrf, database)
}

struct TestPart {
    name: String,
    filename: Option<String>,
    content_type: Option<&'static str>,
    bytes: Vec<u8>,
}

impl TestPart {
    fn file(name: &str, filename: &str, bytes: Vec<u8>) -> Self {
        Self {
            name: name.to_owned(),
            filename: Some(filename.to_owned()),
            content_type: Some("application/octet-stream"),
            bytes,
        }
    }

    fn text(name: &str, value: &str) -> Self {
        Self::bytes(name, value.as_bytes().to_vec())
    }

    fn bytes(name: &str, bytes: Vec<u8>) -> Self {
        Self {
            name: name.to_owned(),
            filename: None,
            content_type: None,
            bytes,
        }
    }
}

fn jpeg_bytes() -> Vec<u8> {
    vec![0xff, 0xd8, 0xff, 0xe0]
}

async fn multipart_json(
    router: &axum::Router,
    uri: &str,
    session: Option<&str>,
    csrf_header: Option<&str>,
    parts: Vec<TestPart>,
) -> (StatusCode, Value) {
    const BOUNDARY: &str = "pipauto-vin-67-boundary";
    let mut body = Vec::new();
    for part in parts {
        body.extend_from_slice(format!("--{BOUNDARY}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"{}\"", part.name).as_bytes(),
        );
        if let Some(filename) = part.filename {
            body.extend_from_slice(format!("; filename=\"{filename}\"").as_bytes());
        }
        body.extend_from_slice(b"\r\n");
        if let Some(content_type) = part.content_type {
            body.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
        }
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(&part.bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{BOUNDARY}--\r\n").as_bytes());
    let mut request = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .header(header::ORIGIN, TEST_ORIGIN);
    if let Some(session) = session {
        request = request.header(header::COOKIE, session);
    }
    if let Some(csrf) = csrf_header {
        request = request.header("X-CSRF-Token", csrf);
    }
    response(
        router
            .clone()
            .oneshot(request.body(Body::from(body)).expect("multipart request"))
            .await
            .expect("multipart response"),
    )
    .await
}

async fn authenticated_get(
    router: &axum::Router,
    uri: &str,
    session: &str,
) -> axum::response::Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(header::COOKIE, session)
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response")
}

async fn create_vehicle(router: &axum::Router, session: &str, csrf: &str, model: &str) -> String {
    let customer = write_json(
        router,
        Method::POST,
        "/api/v1/customers",
        session,
        csrf,
        json!({"display_name": format!("{model} Owner")}),
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
        json!({"customer_id": customer, "make": "Volkswagen", "model": model}),
    )
    .await
    .1["data"]["id"]
        .as_str()
        .expect("vehicle id")
        .to_owned()
}

async fn get_json(router: &axum::Router, uri: &str, session: &str) -> (StatusCode, Value) {
    response(
        router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(uri)
                    .header(header::COOKIE, session)
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
