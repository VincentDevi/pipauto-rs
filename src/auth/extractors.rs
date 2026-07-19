//! Reusable current-user request extractors and browser-aware authentication responses.

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{
        header::{CACHE_CONTROL, COOKIE, LOCATION, VARY},
        request::Parts,
        HeaderMap, HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use loco_rs::app::AppContext;

use crate::{
    auth::{cookies::AuthCookies, settings::AuthSettings},
    errors::AppError,
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
        let settings = shared::<AuthSettings>(&ctx, parts)?;
        let service = shared::<AuthService>(&ctx, parts)?;
        let token = session_cookie(&parts.headers, settings.session_cookie_name())
            .ok_or_else(|| unauthenticated_response(parts, &settings, false))?;
        match service.authenticate(&token).await {
            Ok(user) => Ok(Self(user)),
            Err(AuthError::Unauthenticated) => {
                Err(unauthenticated_response(parts, &settings, true))
            }
            Err(AuthError::Unavailable) => Err(unavailable_response(parts)),
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
        let settings = shared::<AuthSettings>(&ctx, parts)?;
        let service = shared::<AuthService>(&ctx, parts)?;
        let token = match session_cookie(&parts.headers, settings.session_cookie_name()) {
            Some(token) => token,
            None if cookie_was_present(&parts.headers, settings.session_cookie_name()) => {
                return Ok(Self::StaleCredential);
            }
            None => return Ok(Self::Absent),
        };
        match service.authenticate(&token).await {
            Ok(user) => Ok(Self::Authenticated(user)),
            Err(AuthError::Unauthenticated) => Ok(Self::StaleCredential),
            Err(AuthError::Unavailable) => Err(unavailable_response(parts)),
        }
    }
}

#[allow(clippy::result_large_err)]
fn shared<T: Clone + Send + Sync + 'static>(
    ctx: &AppContext,
    parts: &Parts,
) -> Result<T, Response> {
    ctx.shared_store
        .get::<T>()
        .ok_or_else(|| unavailable_response(parts))
}

fn unauthenticated_response(parts: &Parts, settings: &AuthSettings, stale: bool) -> Response {
    if is_api_path(parts.uri.path()) {
        let response = AppError::Unauthenticated.into_response();
        if stale {
            let jar = CookieJar::from_headers(&parts.headers)
                .add(AuthCookies::new(settings.clone()).clear_session());
            return (jar, response).into_response();
        }
        return response;
    }
    let next = safe_next_destination(Some(
        parts
            .uri
            .path_and_query()
            .map_or("/", |value| value.as_str()),
    ));
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
        append_vary_hx_request(response.headers_mut());
        no_store(response)
    } else {
        let mut response = (StatusCode::SEE_OTHER, [(LOCATION, destination)]).into_response();
        append_vary_hx_request(response.headers_mut());
        no_store(response)
    }
}

fn unavailable_response(parts: &Parts) -> Response {
    if is_api_path(parts.uri.path()) {
        return AppError::Unavailable.into_response();
    }
    no_store((
        StatusCode::SERVICE_UNAVAILABLE,
        "Authentication is temporarily unavailable. Please try again.",
    ))
}

fn is_api_path(path: &str) -> bool {
    path == "/api/v1" || path.starts_with("/api/v1/")
}

fn no_store(response: impl IntoResponse) -> Response {
    let mut response = response.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// Validate a local absolute-path redirect destination without consulting request host headers.
#[must_use]
pub fn safe_next_destination(value: Option<&str>) -> String {
    value
        .filter(|candidate| is_safe_next(candidate))
        .unwrap_or("/")
        .to_owned()
}

fn is_safe_next(value: &str) -> bool {
    let Some(decoded) = percent_decode(value) else {
        return false;
    };
    if !decoded.starts_with('/')
        || decoded.starts_with("//")
        || decoded.contains('\\')
        || decoded.chars().any(char::is_control)
    {
        return false;
    }
    let path = decoded.split(['?', '#']).next().unwrap_or("/");
    !matches!(path, "/login" | "/logout")
        && !path.starts_with("/login/")
        && !path.starts_with("/logout/")
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = *bytes.get(index + 1)?;
            let low = *bytes.get(index + 2)?;
            decoded.push(hex_value(high)? * 16 + hex_value(low)?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

const fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn session_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    CookieJar::from_headers(headers)
        .get(name)
        .map(|cookie| cookie.value().to_owned())
}

fn cookie_was_present(headers: &HeaderMap, name: &str) -> bool {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .any(|pair| {
            let pair = pair.trim();
            pair == name
                || pair
                    .split_once('=')
                    .is_some_and(|(candidate, _)| candidate.trim() == name)
        })
}

/// Append the HTMX request header to `Vary` while preserving other cache keys.
pub fn append_vary_hx_request(headers: &mut HeaderMap) {
    let already_varies = headers
        .get_all(VARY)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|value| value.trim().eq_ignore_ascii_case("HX-Request"));
    if !already_varies {
        headers.append(VARY, HeaderValue::from_static("HX-Request"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_next_destination_accepts_only_non_looping_local_absolute_paths() {
        assert_eq!(
            safe_next_destination(Some("/vehicles?recent=true#top")),
            "/vehicles?recent=true#top"
        );
        for unsafe_value in [
            "https://attacker.example",
            "//attacker.example",
            "/\\attacker.example",
            "/%5cattacker.example",
            "/%2fattacker.example",
            "/login",
            "/login?next=/vehicles",
            "/logout/confirm",
            "/bad%0Aheader",
            "/bad%ZZpath",
        ] {
            assert_eq!(safe_next_destination(Some(unsafe_value)), "/");
        }
        assert_eq!(safe_next_destination(None), "/");
    }

    #[test]
    fn vary_helper_preserves_existing_values() {
        let mut headers = HeaderMap::new();
        headers.insert(VARY, HeaderValue::from_static("Accept-Encoding"));
        append_vary_hx_request(&mut headers);
        let values = headers
            .get_all(VARY)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .collect::<Vec<_>>();
        assert_eq!(values, vec!["Accept-Encoding", "HX-Request"]);
    }
}
