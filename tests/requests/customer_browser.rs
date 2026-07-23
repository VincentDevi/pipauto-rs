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
async fn customer_browser_create_edit_archive_restore_and_vehicle_context() {
    let (router, session, csrf) = authenticated_app().await;

    let empty = send(&router, get("/customers", &session, false)).await;
    assert_eq!(empty.0, StatusCode::OK, "{}", empty.1);
    assert!(empty.1.contains("Add your first customer"));
    assert!(empty.1.contains("href=\"/customers/new\">New customer"));

    let create_page = send(&router, get("/customers/new", &session, false)).await;
    assert_eq!(create_page.0, StatusCode::OK, "{}", create_page.1);
    assert!(create_page.1.contains("autocomplete=\"address-line1\""));
    assert!(create_page.1.contains("maxlength=\"10000\""));

    let body = customer_form(
        &csrf,
        "  Jean Dupont  ",
        "Jean.Dupont@Example.COM",
        "+32 (0) 475 12 34 56",
        "Rue du Garage 1",
        "1000",
        "Bruxelles",
        "BE",
        "Prefers morning calls",
    );
    let created = router
        .clone()
        .oneshot(form_request(
            Method::POST,
            "/customers",
            &session,
            body,
            false,
        ))
        .await
        .expect("customer create");
    assert_eq!(created.status(), StatusCode::SEE_OTHER);
    let location = created.headers()[header::LOCATION]
        .to_str()
        .expect("location")
        .to_owned();

    let detail = send(&router, get(&location, &session, false)).await;
    assert_eq!(detail.0, StatusCode::OK);
    for display_value in [
        "Jean Dupont",
        "Jean.Dupont@Example.COM",
        "+32 (0) 475 12 34 56",
        "Rue du Garage 1",
        "Prefers morning calls",
    ] {
        assert!(detail.1.contains(display_value), "missing {display_value}");
    }
    assert!(!detail.1.contains("jean.dupont@example.com"));

    let customer_id = location.trim_start_matches("/customers/");
    let vehicle = write_json(
        &router,
        Method::POST,
        "/api/v1/vehicles",
        &session,
        &csrf,
        json!({
            "customer_id": customer_id,
            "make": "Volkswagen",
            "model": "Golf",
            "registration": "1-ABC-234"
        }),
    )
    .await;
    let vehicle_id = vehicle["data"]["id"].as_str().expect("vehicle id");
    let detail = send(&router, get(&location, &session, false)).await.1;
    assert!(detail.contains("Volkswagen Golf"));
    assert!(detail.contains(&format!("href=\"&#x2F;vehicles&#x2F;{vehicle_id}\"")));
    assert!(detail.contains(&format!(
        "href=\"&#x2F;customers&#x2F;{customer_id}&#x2F;vehicles&#x2F;new\""
    )));

    let edit_body = customer_form(
        &csrf,
        "Jean Dupont-Smith",
        "Jean.Dupont@Example.COM",
        "+32 (0) 475 12 34 56",
        "Rue du Garage 1",
        "1000",
        "Bruxelles",
        "BE",
        "Updated workshop note",
    );
    let edited = router
        .clone()
        .oneshot(form_request(
            Method::POST,
            &format!("{location}/edit"),
            &session,
            edit_body,
            true,
        ))
        .await
        .expect("customer edit");
    assert_eq!(edited.status(), StatusCode::OK);
    assert_eq!(edited.headers()["HX-Redirect"], location);

    for path in [format!("{location}/archive"), format!("{location}/archive")] {
        let response = router
            .clone()
            .oneshot(form_request(
                Method::POST,
                &path,
                &session,
                csrf_only(&csrf),
                false,
            ))
            .await
            .expect("archive");
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
    }
    let archived = send(&router, get(&location, &session, false)).await.1;
    assert!(archived.contains("Archived customer"));
    assert!(archived.contains("Restore customer"));
    assert!(!archived.contains(">Edit customer</a>"));
    assert!(archived.contains("Volkswagen Golf"));

    let archived_edit = send(
        &router,
        form_request(
            Method::POST,
            &format!("{location}/edit"),
            &session,
            customer_form(
                &csrf,
                "Must not replace authoritative state",
                "replacement@example.com",
                "+32 499 00 00 00",
                "Other address",
                "1000",
                "Bruxelles",
                "BE",
                "Must not be saved",
            ),
            false,
        ),
    )
    .await;
    assert_eq!(archived_edit.0, StatusCode::CONFLICT);
    assert!(archived_edit.1.contains("This customer was archived"));
    assert!(archived_edit.1.contains("Jean Dupont-Smith"));
    assert!(!archived_edit
        .1
        .contains("Must not replace authoritative state"));

    for path in [format!("{location}/restore"), format!("{location}/restore")] {
        let response = router
            .clone()
            .oneshot(form_request(
                Method::POST,
                &path,
                &session,
                csrf_only(&csrf),
                false,
            ))
            .await
            .expect("restore");
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
    }
    let restored = send(&router, get(&location, &session, false)).await.1;
    assert!(restored.contains("Jean Dupont-Smith"));
    assert!(restored.contains(">Edit customer</a>"));
}

#[tokio::test]
async fn customer_browser_validation_keeps_safe_display_values() {
    let (router, session, csrf) = authenticated_app().await;
    let invalid = customer_form(
        &csrf,
        "  Safe Submitted Name  ",
        "MixedCase@Example.COM",
        "+32 475 12 34 56",
        "",
        "",
        "",
        "be",
        "Safe submitted note",
    );
    let response = send(
        &router,
        form_request(Method::POST, "/customers", &session, invalid, true),
    )
    .await;
    assert_eq!(
        response.0,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{}",
        response.1
    );
    assert!(response.1.starts_with("<form id=\"customer-form\""));
    assert!(response.1.contains("value=\"  Safe Submitted Name  \""));
    assert!(response.1.contains("value=\"MixedCase@Example.COM\""));
    assert!(response.1.contains("Enter address line 1."));
    assert!(response.1.contains("Enter a postal code."));
    assert!(response.1.contains("Enter a city."));
    assert!(response
        .1
        .contains("Use a two-letter uppercase country code."));
    assert!(!response.1.contains("mixedcase@example.com"));

    let invalid_email = customer_form(
        &csrf,
        "Email display name",
        "Not A Valid Email",
        "+32 470 00 00 01",
        "Workshopstraat 1",
        "9000",
        "Gent",
        "BE",
        "Keep this note",
    );
    let email_error = send(
        &router,
        form_request(Method::POST, "/customers", &session, invalid_email, true),
    )
    .await;
    assert_eq!(email_error.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(email_error.1.contains("Email display name"));
    assert!(email_error.1.contains("Not A Valid Email"));
    assert!(email_error.1.contains("Keep this note"));
    assert!(email_error.1.contains("Enter a valid email address."));
}

#[tokio::test]
async fn customer_browser_search_archive_filter_and_cursor_links_preserve_filters() {
    let (router, session, csrf) = authenticated_app().await;
    for index in 0..26 {
        write_json(
            &router,
            Method::POST,
            "/api/v1/customers",
            &session,
            &csrf,
            json!({"display_name": format!("Cursor Customer {index:02}")}),
        )
        .await;
    }
    write_json(
        &router,
        Method::POST,
        "/api/v1/customers",
        &session,
        &csrf,
        json!({"display_name": "Different customer"}),
    )
    .await;

    let first = send(
        &router,
        get(
            "/customers?q=Cursor+Customer&archived=active",
            &session,
            false,
        ),
    )
    .await;
    assert_eq!(first.0, StatusCode::OK);
    assert!(first.1.contains("value=\"Cursor Customer\""));
    assert!(first.1.contains("archived=active&amp;cursor="));
    assert!(first.1.contains("q=Cursor+Customer"));
    assert!(!first.1.contains("Different customer"));

    let next_href = decode_html_url(&attribute_before_label(&first.1, "href=\"", "Next page"));
    let next = send(&router, get(&next_href, &session, false)).await;
    assert_eq!(next.0, StatusCode::OK);
    assert!(next.1.contains("Cursor Customer"));
    assert!(!next.1.contains("This page link is no longer valid"));

    let mismatch = next_href.replace("archived=active", "archived=archived");
    let mismatch = send(&router, get(&mismatch, &session, false)).await;
    assert_eq!(mismatch.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(mismatch.1.contains("does not match the current filters"));

    let no_match = send(
        &router,
        get("/customers?q=Nobody&archived=archived", &session, false),
    )
    .await;
    assert_eq!(no_match.0, StatusCode::OK);
    assert!(no_match.1.contains("No customers match these filters"));
    assert!(no_match.1.contains("value=\"Nobody\""));
    assert!(no_match.1.contains("Clear filters"));
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

#[allow(clippy::too_many_arguments)]
fn customer_form(
    csrf: &str,
    name: &str,
    email: &str,
    phone: &str,
    address: &str,
    postal_code: &str,
    city: &str,
    country_code: &str,
    notes: &str,
) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in [
        ("_csrf", csrf),
        ("display_name", name),
        ("email", email),
        ("phone", phone),
        ("address_line_1", address),
        ("address_line_2", ""),
        ("postal_code", postal_code),
        ("city", city),
        ("country_code", country_code),
        ("notes", notes),
    ] {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

fn csrf_only(csrf: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .append_pair("_csrf", csrf)
        .finish()
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

fn attribute_before_label(html: &str, marker: &str, label: &str) -> String {
    let before = html.split_once(label).expect("label").0;
    let anchor = before.rsplit_once("<a ").expect("anchor before label").1;
    anchor
        .rsplit_once(marker)
        .expect("attribute")
        .1
        .split_once('"')
        .expect("attribute end")
        .0
        .to_owned()
}

fn decode_html_url(value: &str) -> String {
    value
        .replace("&#x2F;", "/")
        .replace("&#x3F;", "?")
        .replace("&#x3D;", "=")
        .replace("&#x2B;", "+")
        .replace("&amp;", "&")
}
