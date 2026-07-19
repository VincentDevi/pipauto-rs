//! Server-rendered login and logout browser flow.

use crate::{
    auth::{
        cookies::AuthCookies,
        csrf::{AuthenticatedCsrfForm, CsrfService, LoginCsrfForm},
        extractors::{append_vary_hx_request, safe_next_destination, OptionalCurrentUser},
    },
    services::auth::{AuthService, LoginCommand, LoginError},
    views::auth::LoginView,
};
use axum::{
    extract::{ConnectInfo, Extension, Query},
    http::{
        header::{CACHE_CONTROL, LOCATION, RETRY_AFTER},
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

#[derive(Default, Deserialize)]
struct LoginQuery {
    next: Option<String>,
}

#[derive(Deserialize)]
struct LoginForm {
    email: String,
    password: String,
    next: String,
}

#[derive(Deserialize)]
struct LogoutForm {}

async fn show_login(
    optional_user: OptionalCurrentUser,
    Query(query): Query<LoginQuery>,
    headers: HeaderMap,
    jar: CookieJar,
    SharedStore(csrf): SharedStore<CsrfService>,
    SharedStore(cookies): SharedStore<AuthCookies>,
    ViewEngine(engine): ViewEngine<TeraView>,
) -> Result<Response> {
    let next = safe_next_destination(query.next.as_deref());
    if matches!(optional_user, OptionalCurrentUser::Authenticated(_)) {
        return Ok(with_no_store(redirect(&next, is_htmx(&headers))));
    }
    let state = match csrf.issue_login(Utc::now()) {
        Ok(state) => state,
        Err(_) => return Ok(unavailable()),
    };
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
    SharedStore(cookies): SharedStore<AuthCookies>,
    SharedStore(service): SharedStore<AuthService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    validated: LoginCsrfForm<LoginForm>,
) -> Result<Response> {
    let htmx = is_htmx(&headers);
    let submitted_token = validated.token().expose().to_owned();
    let form = validated.fields;
    let next = safe_next_destination(Some(&form.next));

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
            &submitted_token,
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
            &submitted_token,
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
                &submitted_token,
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
            &submitted_token,
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
    headers: HeaderMap,
    jar: CookieJar,
    SharedStore(cookies): SharedStore<AuthCookies>,
    SharedStore(service): SharedStore<AuthService>,
    validated: AuthenticatedCsrfForm<LogoutForm>,
) -> Result<Response> {
    let htmx = is_htmx(&headers);
    let outcome = service.logout(Some(validated.encoded_jwt())).await;
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
    append_vary_hx_request(response.headers_mut());
    Ok(with_no_store(response))
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
        append_vary_hx_request(response.headers_mut());
        response
    } else {
        let mut response = (StatusCode::SEE_OTHER, [(LOCATION, destination)]).into_response();
        append_vary_hx_request(response.headers_mut());
        response
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
    fn safe_next_destination_allows_local_paths_and_rejects_redirect_attacks() {
        assert_eq!(
            safe_next_destination(Some("/vehicles?recent=true")),
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
            assert_eq!(safe_next_destination(Some(unsafe_value)), "/");
        }
    }
}
