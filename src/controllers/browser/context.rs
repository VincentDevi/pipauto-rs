//! Presentation-safe context extracted once for authenticated browser requests.

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{request::Parts, HeaderMap},
    response::Response,
};
use axum_extra::extract::cookie::CookieJar;
use loco_rs::app::AppContext;
use serde::Serialize;

use crate::{
    auth::{
        csrf::{CsrfService, SecretCsrfToken},
        extractors::CurrentUser,
        settings::AuthSettings,
    },
    models::auth::{AuthenticationModel as AuthService, UserId},
    views::{context::PresentationUser, layout::AuthenticatedLayout},
};

/// Validated local absolute path suitable for a same-origin redirect.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct LocalReturnPath(String);

impl LocalReturnPath {
    /// Parse a local absolute path, rejecting cross-origin and malformed values.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let decoded = percent_decode(value)?;
        let valid = decoded.starts_with('/')
            && !decoded.starts_with("//")
            && !decoded.contains('\\')
            && !decoded.chars().any(char::is_control)
            && !matches!(decoded.as_str(), "/login" | "/logout")
            && !decoded.starts_with("/login/")
            && !decoded.starts_with("/logout/");
        valid.then_some(Self(decoded))
    }

    /// Validated redirect value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Whether a browser expects a complete document or an HTMX fragment.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponsePreference {
    FullPage,
    HtmxFragment,
}

impl ResponsePreference {
    #[must_use]
    pub fn from_headers(headers: &HeaderMap) -> Self {
        if headers
            .get("HX-Request")
            .is_some_and(|value| value == "true")
        {
            Self::HtmxFragment
        } else {
            Self::FullPage
        }
    }
}

/// Complete presentation-safe state shared by authenticated browser controllers.
#[derive(Debug, Serialize)]
pub struct BrowserRequestContext {
    #[serde(skip)]
    pub actor_id: UserId,
    pub current_user: PresentationUser,
    pub csrf_token: SecretCsrfToken,
    pub current_path: String,
    pub return_path: Option<LocalReturnPath>,
    pub response_preference: ResponsePreference,
}

impl BrowserRequestContext {
    /// Build the common authenticated page layout from presentation-safe request state.
    #[must_use]
    pub fn layout(&self) -> AuthenticatedLayout<'_> {
        AuthenticatedLayout::new(
            &self.current_user,
            self.csrf_token.expose(),
            &self.current_path,
        )
    }
}

impl<S> FromRequestParts<S> for BrowserRequestContext
where
    AppContext: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let CurrentUser(user) = CurrentUser::from_request_parts(parts, state).await?;
        let ctx = AppContext::from_ref(state);
        let settings = shared::<AuthSettings>(&ctx)?;
        let service = shared::<AuthService>(&ctx)?;
        let csrf = shared::<CsrfService>(&ctx)?;
        let jar = CookieJar::from_headers(&parts.headers);
        let encoded = jar
            .get(settings.session_cookie_name())
            .map(axum_extra::extract::cookie::Cookie::value)
            .ok_or_else(crate::controllers::browser::responses::authentication_unavailable)?;
        let session = service
            .authenticate_session(encoded)
            .await
            .map_err(|_| crate::controllers::browser::responses::authentication_unavailable())?;
        let token = csrf
            .issue_authenticated(&session.jti, user.session_expires_at)
            .map_err(|_| crate::controllers::browser::responses::authentication_unavailable())?;
        let return_path = raw_query_value(parts.uri.query(), "return_to")
            .and_then(|value| LocalReturnPath::parse(&value));

        Ok(Self {
            actor_id: user.id,
            current_user: PresentationUser {
                display_name: user.display_name,
            },
            csrf_token: token,
            current_path: parts.uri.path().to_owned(),
            return_path,
            response_preference: ResponsePreference::from_headers(&parts.headers),
        })
    }
}

#[allow(clippy::result_large_err)]
fn shared<T: Clone + Send + Sync + 'static>(ctx: &AppContext) -> Result<T, Response> {
    ctx.shared_store
        .get::<T>()
        .ok_or_else(crate::controllers::browser::responses::authentication_unavailable)
}

fn raw_query_value(query: Option<&str>, key: &str) -> Option<String> {
    query?
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(candidate, value)| (candidate == key).then(|| value.replace('+', " ")))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_return_paths_reject_external_and_malformed_values() {
        assert_eq!(
            LocalReturnPath::parse("/vehicles%3Frecent%3Dtrue").map(|path| path.0),
            Some("/vehicles?recent=true".to_owned())
        );
        for value in [
            "https://attacker.example/path",
            "//attacker.example/path",
            "%2F%2Fattacker.example/path",
            "/%5Cattacker.example",
            "/bad%ZZpath",
            "/bad%0Aheader",
            "vehicles",
        ] {
            assert_eq!(LocalReturnPath::parse(value), None, "accepted {value}");
        }
    }
}
