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
async fn customers_api_crud_archive_search_and_pagination_are_stable() {
    let (router, session, csrf) = authenticated_app().await;

    let first = post_json(
        &router,
        "/api/v1/customers",
        &session,
        &csrf,
        json!({
            "display_name": "  Filippo  Straße ",
            "email": " Filippo@Example.COM ",
            "phone": "+32 (0) 475-12.34.56"
        }),
    )
    .await;
    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(first.1["data"]["display_name"], "Filippo  Straße");
    let first_id = first.1["data"]["id"].as_str().expect("id").to_owned();

    for name in ["Ada Lovelace", "Grace Hopper"] {
        assert_eq!(
            post_json(
                &router,
                "/api/v1/customers",
                &session,
                &csrf,
                json!({
                    "display_name": name
                })
            )
            .await
            .0,
            StatusCode::OK
        );
    }

    let page = get_json(&router, "/api/v1/customers?limit=2", Some(&session)).await;
    assert_eq!(page.0, StatusCode::OK);
    assert_eq!(page.1["data"].as_array().expect("items").len(), 2);
    let cursor = page.1["next_cursor"].as_str().expect("cursor");
    let next = get_json(
        &router,
        &format!("/api/v1/customers?limit=2&cursor={cursor}"),
        Some(&session),
    )
    .await;
    assert_eq!(next.0, StatusCode::OK);
    assert_eq!(next.1["data"].as_array().expect("items").len(), 1);

    let searched = get_json(&router, "/api/v1/customers?q=STRASSE", Some(&session)).await;
    assert_eq!(searched.0, StatusCode::OK);
    assert_eq!(searched.1["data"][0]["id"], first_id);

    let archived = post_json(
        &router,
        &format!("/api/v1/customers/{first_id}/archive"),
        &session,
        &csrf,
        Value::Null,
    )
    .await;
    assert_eq!(archived.0, StatusCode::OK);
    let archived_at = archived.1["data"]["archived_at"].clone();
    let repeated = post_json(
        &router,
        &format!("/api/v1/customers/{first_id}/archive"),
        &session,
        &csrf,
        Value::Null,
    )
    .await;
    assert_eq!(repeated.1["data"]["archived_at"], archived_at);
    assert_eq!(
        get_json(
            &router,
            &format!("/api/v1/customers/{first_id}"),
            Some(&session)
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        post_json(
            &router,
            &format!("/api/v1/customers/{first_id}/restore"),
            &session,
            &csrf,
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );

    assert_eq!(
        get_json(&router, "/api/v1/customers?unknown=true", Some(&session))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        get_json(&router, "/api/v1/customers", None).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn vehicles_api_normalizes_conflicts_and_preserves_current_relationships() {
    let (router, session, csrf) = authenticated_app().await;
    let owner_a = create_customer(&router, &session, &csrf, "Owner A").await;
    let owner_b = create_customer(&router, &session, &csrf, "Owner B").await;

    let created = post_json(
        &router,
        "/api/v1/vehicles",
        &session,
        &csrf,
        json!({
            "customer_id": owner_a,
            "make": " Volkswagen ",
            "model": " Golf  GTE ",
            "registration": " 1-abc-234 ",
            "vin": " wvwzzz1jzxw000001 ",
            "current_mileage": 125000
        }),
    )
    .await;
    assert_eq!(created.0, StatusCode::OK);
    let vehicle_id = created.1["data"]["id"]
        .as_str()
        .expect("vehicle id")
        .to_owned();

    let duplicate = post_json(
        &router,
        "/api/v1/vehicles",
        &session,
        &csrf,
        json!({
            "customer_id": owner_b, "make": "VW", "model": "Golf",
            "registration": "1 ABC 234"
        }),
    )
    .await;
    assert_eq!(duplicate.0, StatusCode::CONFLICT);
    assert!(duplicate.1["error"].get("existing_record").is_none());

    let reassigned = patch_json(
        &router,
        &format!("/api/v1/vehicles/{vehicle_id}"),
        &session,
        &csrf,
        json!({"customer_id": owner_b}),
    )
    .await;
    assert_eq!(reassigned.0, StatusCode::OK);
    assert_eq!(reassigned.1["data"]["customer_id"], owner_b);
    assert_eq!(
        get_json(
            &router,
            &format!("/api/v1/customers/{owner_a}/vehicles?archived=all"),
            Some(&session)
        )
        .await
        .1["data"]
            .as_array()
            .expect("items")
            .len(),
        0
    );
    assert_eq!(
        get_json(
            &router,
            &format!("/api/v1/customers/{owner_b}/vehicles?archived=all"),
            Some(&session)
        )
        .await
        .1["data"][0]["id"],
        vehicle_id
    );

    assert_eq!(
        post_json(
            &router,
            &format!("/api/v1/customers/{owner_a}/archive"),
            &session,
            &csrf,
            Value::Null
        )
        .await
        .0,
        StatusCode::OK
    );
    let rejected = patch_json(
        &router,
        &format!("/api/v1/vehicles/{vehicle_id}"),
        &session,
        &csrf,
        json!({"customer_id": owner_a}),
    )
    .await;
    assert_eq!(rejected.0, StatusCode::CONFLICT);
    assert_eq!(
        get_json(
            &router,
            &format!("/api/v1/vehicles/{vehicle_id}"),
            Some(&session)
        )
        .await
        .1["data"]["customer_id"],
        owner_b
    );

    let exact = get_json(
        &router,
        "/api/v1/vehicles?registration=1-ABC-234",
        Some(&session),
    )
    .await;
    assert_eq!(exact.0, StatusCode::OK);
    assert_eq!(exact.1["data"][0]["id"], vehicle_id);

    let mut no_csrf = authenticated_json_request(
        Method::PATCH,
        &format!("/api/v1/vehicles/{vehicle_id}"),
        &session,
        &csrf,
        json!({"model": "Changed"}).to_string(),
    );
    no_csrf.headers_mut().remove("X-CSRF-Token");
    assert_eq!(
        router
            .clone()
            .oneshot(no_csrf)
            .await
            .expect("request")
            .status(),
        StatusCode::FORBIDDEN
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

async fn create_customer(router: &axum::Router, session: &str, csrf: &str, name: &str) -> String {
    post_json(
        router,
        "/api/v1/customers",
        session,
        csrf,
        json!({"display_name": name}),
    )
    .await
    .1["data"]["id"]
        .as_str()
        .expect("customer id")
        .to_owned()
}

async fn get_json(router: &axum::Router, uri: &str, session: Option<&str>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(Method::GET).uri(uri);
    if let Some(session) = session {
        builder = builder.header("Cookie", session);
    }
    response(
        router
            .clone()
            .oneshot(builder.body(Body::empty()).expect("request"))
            .await
            .expect("response"),
    )
    .await
}

async fn post_json(
    router: &axum::Router,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
) -> (StatusCode, Value) {
    response(
        router
            .clone()
            .oneshot(authenticated_json_request(
                Method::POST,
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

async fn patch_json(
    router: &axum::Router,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
) -> (StatusCode, Value) {
    response(
        router
            .clone()
            .oneshot(authenticated_json_request(
                Method::PATCH,
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
