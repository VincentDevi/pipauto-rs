//! Shared full-page, HTMX fragment, redirect, and safe error responses.

use std::fmt::Write as _;

use axum::{
    http::{
        header::{CACHE_CONTROL, LOCATION},
        HeaderValue, StatusCode,
    },
    response::{Html, IntoResponse, Response},
};

use super::context::ResponsePreference;
use crate::auth::extractors::append_vary_hx_request;

#[must_use]
pub fn full_page(status: StatusCode, html: String) -> Response {
    sensitive((status, Html(html)).into_response())
}

#[must_use]
pub fn fragment(status: StatusCode, html: String) -> Response {
    let mut response = sensitive((status, Html(html)).into_response());
    append_vary_hx_request(response.headers_mut());
    response
}

#[must_use]
pub fn render(
    preference: ResponsePreference,
    status: StatusCode,
    page: String,
    panel: String,
) -> Response {
    match preference {
        ResponsePreference::FullPage => full_page(status, page),
        ResponsePreference::HtmxFragment => fragment(status, panel),
    }
}

#[must_use]
pub fn redirect(preference: ResponsePreference, destination: &str) -> Response {
    let mut response = match preference {
        ResponsePreference::FullPage => {
            (StatusCode::SEE_OTHER, [(LOCATION, destination)]).into_response()
        }
        ResponsePreference::HtmxFragment => {
            let mut response = StatusCode::OK.into_response();
            if let Ok(value) = HeaderValue::from_str(destination) {
                response.headers_mut().insert("HX-Redirect", value);
            }
            response
        }
    };
    append_vary_hx_request(response.headers_mut());
    sensitive(response)
}

#[must_use]
pub fn validation(preference: ResponsePreference, page: String, form: String) -> Response {
    render(preference, StatusCode::UNPROCESSABLE_ENTITY, page, form)
}

#[must_use]
pub fn conflict(preference: ResponsePreference, page: String, panel: String) -> Response {
    render(preference, StatusCode::CONFLICT, page, panel)
}

#[must_use]
pub fn not_found(preference: ResponsePreference, resource: &str) -> Response {
    let panel = safe_panel(
        "Not found",
        &format!("The requested {resource} was not found."),
        None,
    );
    let page = safe_page("Not found", &panel);
    render(preference, StatusCode::NOT_FOUND, page, panel)
}

#[must_use]
pub fn unavailable(preference: ResponsePreference, message: &str) -> Response {
    let panel = safe_panel("Temporarily unavailable", message, None);
    let page = safe_page("Temporarily unavailable", &panel);
    render(preference, StatusCode::SERVICE_UNAVAILABLE, page, panel)
}

#[must_use]
pub fn not_implemented(preference: ResponsePreference) -> Response {
    unavailable(
        preference,
        "This workshop page is not available yet. No changes were made.",
    )
    .map_status(StatusCode::NOT_IMPLEMENTED)
}

#[must_use]
pub fn unexpected(preference: ResponsePreference) -> Response {
    let reference = correlation_reference();
    tracing::error!(
        correlation_reference = reference,
        "unexpected browser response"
    );
    let panel = safe_panel(
        "Something went wrong",
        "Try again. If the problem continues, keep this reference.",
        Some(&reference),
    );
    let page = safe_page("Something went wrong", &panel);
    let mut response = render(preference, StatusCode::INTERNAL_SERVER_ERROR, page, panel);
    if let Ok(value) = HeaderValue::from_str(&reference) {
        response.headers_mut().insert("X-Correlation-ID", value);
    }
    response
}

#[must_use]
pub fn authentication_unavailable() -> Response {
    unavailable(
        ResponsePreference::FullPage,
        "Authentication is temporarily unavailable. Please try again.",
    )
}

fn sensitive(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn safe_page(title: &str, panel: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title></head><body><main>{panel}</main></body></html>",
        escape(title)
    )
}

fn safe_panel(title: &str, message: &str, reference: Option<&str>) -> String {
    let mut html = format!(
        "<section role=\"status\"><h1>{}</h1><p>{}</p>",
        escape(title),
        escape(message)
    );
    if let Some(reference) = reference {
        let _ = write!(html, "<p>Reference: <code>{}</code></p>", escape(reference));
    }
    html.push_str("</section>");
    html
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn correlation_reference() -> String {
    let mut random = [0_u8; 8];
    if getrandom::fill(&mut random).is_err() {
        return "browser-unexpected".to_owned();
    }
    let mut reference = String::from("browser-");
    for byte in random {
        let _ = write!(reference, "{byte:02x}");
    }
    reference
}

trait MapStatus {
    fn map_status(self, status: StatusCode) -> Self;
}

impl MapStatus for Response {
    fn map_status(mut self, status: StatusCode) -> Self {
        *self.status_mut() = status;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_and_htmx_helpers_share_sensitive_response_rules() {
        let standard = redirect(ResponsePreference::FullPage, "/vehicles");
        let htmx = redirect(ResponsePreference::HtmxFragment, "/vehicles");
        assert_eq!(standard.status(), StatusCode::SEE_OTHER);
        assert_eq!(htmx.status(), StatusCode::OK);
        assert_eq!(standard.headers()[CACHE_CONTROL], "no-store");
        assert_eq!(htmx.headers()[CACHE_CONTROL], "no-store");
        assert_eq!(htmx.headers()["HX-Redirect"], "/vehicles");
    }
}
