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
            "vehicle_id": first_vehicle, "service_date": "2026-07-19"
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
async fn attachments_api_is_owner_specific_json_only_and_metadata_only() {
    let (router, session, csrf) = authenticated_app().await;
    let vehicle = create_vehicle(&router, &session, &csrf, "Golf").await;
    let created = write_json(
        &router,
        Method::POST,
        &format!("/api/v1/vehicles/{vehicle}/attachments"),
        &session,
        &csrf,
        json!({
            "display_name": "Water pump.jpg", "media_type": "image/jpeg", "byte_size": 24512,
            "caption": "Before replacement"
        }),
    )
    .await;
    assert_eq!(created.0, StatusCode::CREATED);
    assert_eq!(created.1["data"]["owner_type"], "vehicle");
    assert_eq!(created.1["data"]["storage_state"], "metadata_only");
    let id = created.1["data"]["id"]
        .as_str()
        .expect("attachment id")
        .to_owned();
    assert_eq!(
        get_json(&router, &format!("/api/v1/attachments/{id}"), &session)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        get_json(
            &router,
            &format!("/api/v1/vehicles/{vehicle}/attachments"),
            &session
        )
        .await
        .1["data"]
            .as_array()
            .expect("attachments")
            .len(),
        1
    );

    let fabricated = write_json(
        &router,
        Method::POST,
        &format!("/api/v1/vehicles/{vehicle}/attachments"),
        &session,
        &csrf,
        json!({
            "display_name": "Claim.jpg", "media_type": "image/jpeg", "storage_state": "uploaded"
        }),
    )
    .await;
    assert_eq!(fabricated.0, StatusCode::BAD_REQUEST);

    let multipart = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/v1/vehicles/{vehicle}/attachments"))
                .header(header::CONTENT_TYPE, "multipart/form-data; boundary=test")
                .header(header::COOKIE, &session)
                .header(header::ORIGIN, TEST_ORIGIN)
                .header("X-CSRF-Token", &csrf)
                .body(Body::from("--test--"))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(multipart.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let updated = write_json(&router, Method::PATCH, &format!("/api/v1/attachments/{id}"), &session, &csrf, json!({
        "display_name": "Water pump before.jpg", "media_type": "image/jpeg", "caption": "Updated"
    })).await;
    assert_eq!(updated.0, StatusCode::OK);
    assert_eq!(updated.1["data"]["storage_state"], "metadata_only");
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
    assert_eq!(
        write_json(
            &router,
            Method::POST,
            &format!("/api/v1/vehicles/{vehicle}/archive"),
            &session,
            &csrf,
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        write_json(
            &router,
            Method::POST,
            &format!("/api/v1/vehicles/{vehicle}/attachments"),
            &session,
            &csrf,
            json!({
                "display_name": "Archived.jpg", "media_type": "image/jpeg"
            })
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
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
