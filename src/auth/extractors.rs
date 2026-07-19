//! Reusable current-user request extractors and browser-aware authentication responses.

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header::LOCATION, request::Parts, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use loco_rs::{app::AppContext, controller::extractor::auth::extract_token_from_cookie};

use crate::{
    auth::settings::AuthSettings,
    models::auth::AuthenticatedUser,
    services::auth::{AuthError, AuthService},
};

/// Required authenticated user, safe for controller and view consumption.
pub struct CurrentUser(pub AuthenticatedUser);

impl<S> FromRequestParts<S> for CurrentUser
where
    AppContext: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let ctx = AppContext::from_ref(state);
        let settings = shared::<AuthSettings>(&ctx)?;
        let service = shared::<AuthService>(&ctx)?;
        let token = extract_token_from_cookie(settings.session_cookie_name(), parts)
            .map_err(|_| unauthenticated_response(parts))?;
        match service.authenticate(&token).await {
            Ok(user) => Ok(Self(user)),
            Err(AuthError::Unauthenticated) => Err(unauthenticated_response(parts)),
            Err(AuthError::Unavailable) => Err(unavailable_response()),
        }
    }
}

/// Guest-page authentication state that distinguishes a stale presented cookie from absence.
pub enum OptionalCurrentUser {
    /// No session cookie was presented.
    Absent,
    /// A complete valid session was presented.
    Authenticated(AuthenticatedUser),
    /// A malformed, expired, revoked, or inactive credential was presented.
    StaleCredential,
}

impl<S> FromRequestParts<S> for OptionalCurrentUser
where
    AppContext: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let ctx = AppContext::from_ref(state);
        let settings = shared::<AuthSettings>(&ctx)?;
        let service = shared::<AuthService>(&ctx)?;
        let token = match extract_token_from_cookie(settings.session_cookie_name(), parts) {
            Ok(token) => token,
            Err(_) => return Ok(Self::Absent),
        };
        match service.authenticate(&token).await {
            Ok(user) => Ok(Self::Authenticated(user)),
            Err(AuthError::Unauthenticated) => Ok(Self::StaleCredential),
            Err(AuthError::Unavailable) => Err(unavailable_response()),
        }
    }
}

#[allow(clippy::result_large_err)]
fn shared<T: Clone + Send + Sync + 'static>(ctx: &AppContext) -> Result<T, Response> {
    ctx.shared_store.get::<T>().ok_or_else(unavailable_response)
}

fn unauthenticated_response(parts: &Parts) -> Response {
    let next = safe_next_from_uri(
        parts
            .uri
            .path_and_query()
            .map_or("/", |value| value.as_str()),
    );
    let destination = format!("/login?next={}", percent_encode_path(&next));
    if parts
        .headers
        .get("HX-Request")
        .is_some_and(|value| value == "true")
    {
        let mut response = StatusCode::UNAUTHORIZED.into_response();
        if let Ok(value) = HeaderValue::from_str(&destination) {
            response.headers_mut().insert("HX-Redirect", value);
        }
        response
    } else {
        (StatusCode::SEE_OTHER, [(LOCATION, destination)]).into_response()
    }
}

fn unavailable_response() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        "Authentication is temporarily unavailable. Please try again.",
    )
        .into_response()
}

fn safe_next_from_uri(value: &str) -> String {
    if value.starts_with('/') && !value.starts_with("//") && !value.contains('\\') {
        value.to_owned()
    } else {
        "/".to_owned()
    }
}

fn percent_encode_path(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(&mut encoded, "%{byte:02X}");
        }
    }
    encoded
}
