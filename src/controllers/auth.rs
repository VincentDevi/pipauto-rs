//! Server-rendered login and logout browser flow.

use axum::{
    extract::{ConnectInfo, Extension, Form, Query},
    http::{
        header::{CACHE_CONTROL, LOCATION, ORIGIN, REFERER, RETRY_AFTER, VARY},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
};
use axum_extra::extract::cookie::CookieJar;
use chrono::Utc;
use loco_rs::{
    controller::extractor::shared_store::SharedStore,
    controller::{format, views::engines::TeraView, views::ViewEngine, Routes},
    prelude::{get, post},
    Result,
};
use serde::Deserialize;
use std::net::SocketAddr;
use url::Url;

use crate::{
    auth::{
        cookies::AuthCookies,
        csrf::CsrfService,
        extractors::{CurrentUser, OptionalCurrentUser},
        settings::AuthSettings,
    },
    services::auth::{AuthError, AuthService, LoginCommand, LoginError},
    views::auth::LoginView,
};

#[derive(Default, Deserialize)]
struct LoginQuery {
    next: Option<String>,
}

#[derive(Deserialize)]
struct LoginForm {
    email: String,
    password: String,
    #[serde(rename = "_csrf")]
    csrf: String,
    next: String,
}

#[derive(Deserialize)]
struct LogoutForm {
    #[serde(rename = "_csrf")]
    csrf: String,
}

async fn show_login(
    optional_user: OptionalCurrentUser,
    Query(query): Query<LoginQuery>,
    jar: CookieJar,
    SharedStore(csrf): SharedStore<CsrfService>,
    SharedStore(cookies): SharedStore<AuthCookies>,
    ViewEngine(engine): ViewEngine<TeraView>,
) -> Result<Response> {
    let next = safe_next(query.next.as_deref());
    if matches!(optional_user, OptionalCurrentUser::Authenticated(_)) {
        return Ok(redirect(&next, false));
    }
    let state = csrf.issue_login(Utc::now()).map_err(loco_rs::Error::msg)?;
    let view = LoginView::new("", &next, &state.token, None, None, None);
    let response = format::html(&view.render_page(&engine)?)?;
    let mut jar = jar.add(cookies.login_csrf(state.nonce, state.expires_at));
    if matches!(optional_user, OptionalCurrentUser::StaleCredential) {
        jar = jar.add(cookies.clear_session());
    }
    Ok(with_no_store((jar, response).into_response()))
}

#[allow(clippy::too_many_arguments)]
async fn login(
    headers: HeaderMap,
    jar: CookieJar,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    SharedStore(settings): SharedStore<AuthSettings>,
    SharedStore(csrf): SharedStore<CsrfService>,
    SharedStore(cookies): SharedStore<AuthCookies>,
    SharedStore(service): SharedStore<AuthService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Form(form): Form<LoginForm>,
) -> Result<Response> {
    let htmx = is_htmx(&headers);
    let next = safe_next(Some(&form.next));
    let Some(request_origin) = request_origin(&headers, &settings) else {
        return Ok(forbidden_login());
    };
    let Some(submitted_token) = submitted_csrf(&headers, &form.csrf) else {
        return Ok(forbidden_login());
    };
    let Some(nonce) = jar
        .get(settings.login_csrf_cookie_name())
        .map(axum_extra::extract::cookie::Cookie::value)
    else {
        return Ok(forbidden_login());
    };
    if csrf
        .validate_login(submitted_token, nonce, &request_origin, Utc::now())
        .is_err()
    {
        return Ok(forbidden_login());
    }

    let client_network = connect_info.map_or_else(
        || "socket:unknown".to_owned(),
        |Extension(ConnectInfo(address))| format!("socket:{}", address.ip()),
    );
    match service
        .login(LoginCommand {
            email: form.email.clone(),
            password: form.password,
            client_network,
        })
        .await
    {
        Ok(success) => {
            let jar = jar
                .add(cookies.session(success.encoded_jwt().to_owned()))
                .add(cookies.clear_login_csrf());
            Ok(with_no_store((jar, redirect(&next, htmx)).into_response()))
        }
        Err(LoginError::InvalidInput) => render_login_error(
            &engine,
            &form.email,
            &next,
            submitted_token,
            "Check the highlighted fields and try again.",
            Some("Enter a valid email address."),
            Some("Use a password of at least 12 characters."),
            StatusCode::UNPROCESSABLE_ENTITY,
            htmx,
        ),
        Err(LoginError::InvalidCredentials) => render_login_error(
            &engine,
            &form.email,
            &next,
            submitted_token,
            "Invalid credentials.",
            None,
            None,
            StatusCode::UNAUTHORIZED,
            htmx,
        ),
        Err(LoginError::Throttled { until }) => {
            let retry = (until - Utc::now()).num_seconds().max(1);
            let mut response = render_login_error(
                &engine,
                &form.email,
                &next,
                submitted_token,
                "Too many attempts. Wait briefly and try again.",
                None,
                None,
                StatusCode::TOO_MANY_REQUESTS,
                htmx,
            )?;
            response.headers_mut().insert(
                RETRY_AFTER,
                HeaderValue::from_str(&retry.to_string()).unwrap_or(HeaderValue::from_static("60")),
            );
            Ok(response)
        }
        Err(LoginError::Unavailable) => render_login_error(
            &engine,
            &form.email,
            &next,
            submitted_token,
            "Authentication is temporarily unavailable. Try again shortly.",
            None,
            None,
            StatusCode::SERVICE_UNAVAILABLE,
            htmx,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
async fn logout(
    CurrentUser(_user): CurrentUser,
    headers: HeaderMap,
    jar: CookieJar,
    SharedStore(settings): SharedStore<AuthSettings>,
    SharedStore(csrf): SharedStore<CsrfService>,
    SharedStore(cookies): SharedStore<AuthCookies>,
    SharedStore(service): SharedStore<AuthService>,
    Form(form): Form<LogoutForm>,
) -> Result<Response> {
    let htmx = is_htmx(&headers);
    let token = jar
        .get(settings.session_cookie_name())
        .map(axum_extra::extract::cookie::Cookie::value);
    let request_origin = request_origin(&headers, &settings);
    let session = match token {
        Some(token) => service.authenticate_session(token).await,
        None => Err(AuthError::Unauthenticated),
    };
    let valid_csrf = match (&session, request_origin) {
        (Ok(session), Some(origin)) => csrf
            .validate_authenticated(&form.csrf, &session.jti, &origin, Utc::now())
            .is_ok(),
        _ => false,
    };
    if !valid_csrf {
        return Ok(with_no_store(
            (StatusCode::FORBIDDEN, "Invalid logout request.").into_response(),
        ));
    }
    let outcome = service.logout(token).await;
    let jar = jar.add(cookies.clear_session());
    match outcome {
        Ok(_) => Ok(with_no_store(
            (jar, redirect("/login", htmx)).into_response(),
        )),
        Err(_) => Ok(with_no_store((jar, unavailable()).into_response())),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_login_error(
    engine: &TeraView,
    email: &str,
    next: &str,
    csrf: &str,
    summary: &str,
    email_error: Option<&str>,
    password_error: Option<&str>,
    status: StatusCode,
    htmx: bool,
) -> Result<Response> {
    let view = LoginView::new(
        email,
        next,
        csrf,
        Some(summary),
        email_error,
        password_error,
    );
    let html = if htmx {
        view.render_form(engine)?
    } else {
        view.render_page(engine)?
    };
    let mut response = (status, format::html(&html)?).into_response();
    response
        .headers_mut()
        .insert(VARY, HeaderValue::from_static("HX-Request"));
    Ok(with_no_store(response))
}

/// Reject open redirects and authentication loops.
#[must_use]
pub fn safe_next(value: Option<&str>) -> String {
    let Some(value) = value else {
        return "/".to_owned();
    };
    let valid = value.starts_with('/')
        && !value.starts_with("//")
        && !value.contains('\\')
        && !value.chars().any(char::is_control)
        && !matches!(value, "/login" | "/logout")
        && !value.starts_with("/login?")
        && !value.starts_with("/logout?");
    if valid {
        value.to_owned()
    } else {
        "/".to_owned()
    }
}

fn submitted_csrf<'request>(
    headers: &'request HeaderMap,
    form: &'request str,
) -> Option<&'request str> {
    let header = headers
        .get("X-CSRF-Token")
        .and_then(|value| value.to_str().ok());
    match header {
        Some(header) if header != form => None,
        Some(header) => Some(header),
        None if !form.is_empty() => Some(form),
        None => None,
    }
}

fn request_origin(headers: &HeaderMap, settings: &AuthSettings) -> Option<String> {
    if let Some(origin) = headers.get(ORIGIN).and_then(|value| value.to_str().ok()) {
        return Some(origin.trim_end_matches('/').to_owned());
    }
    let referer = headers.get(REFERER)?.to_str().ok()?;
    let referer = Url::parse(referer).ok()?;
    let origin = referer.origin().ascii_serialization();
    (origin == settings.canonical_origin().origin().ascii_serialization()).then_some(origin)
}

fn is_htmx(headers: &HeaderMap) -> bool {
    headers
        .get("HX-Request")
        .is_some_and(|value| value == "true")
}

fn redirect(destination: &str, htmx: bool) -> Response {
    if htmx {
        let mut response = StatusCode::OK.into_response();
        if let Ok(value) = HeaderValue::from_str(destination) {
            response.headers_mut().insert("HX-Redirect", value);
        }
        response
            .headers_mut()
            .insert(VARY, HeaderValue::from_static("HX-Request"));
        response
    } else {
        (StatusCode::SEE_OTHER, [(LOCATION, destination)]).into_response()
    }
}

fn with_no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn unavailable() -> Response {
    with_no_store(
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Authentication is temporarily unavailable. Reference: auth-unavailable",
        )
            .into_response(),
    )
}

fn forbidden_login() -> Response {
    with_no_store(
        (
            StatusCode::FORBIDDEN,
            "The form expired. Reload the login page and try again.",
        )
            .into_response(),
    )
}

/// Authentication routes.
#[must_use]
pub fn routes() -> Routes {
    Routes::new()
        .add("/login", get(show_login))
        .add("/login", post(login))
        .add("/logout", post(logout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_next_allows_local_paths_and_rejects_redirect_attacks() {
        assert_eq!(
            safe_next(Some("/vehicles?recent=true")),
            "/vehicles?recent=true"
        );
        for unsafe_value in [
            "https://example.com",
            "//example.com",
            "/\\example.com",
            "/login",
            "/logout?next=/",
            "/bad\nheader",
        ] {
            assert_eq!(safe_next(Some(unsafe_value)), "/");
        }
    }
}
