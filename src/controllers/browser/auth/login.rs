use super::*;
#[derive(Default, Deserialize)]
pub(super) struct LoginQuery {
    next: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct LoginForm {
    #[serde(default)]
    email: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    next: String,
}

#[derive(Deserialize)]
pub(super) struct LogoutForm {}

pub(super) async fn show_login(
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
        Err(_) => {
            let response = render_unavailable(
                &engine,
                "We could not prepare sign-in. Please try again shortly.",
                false,
            )?;
            return Ok(with_optional_stale_cookie(
                response,
                &jar,
                &cookies,
                matches!(optional_user, OptionalCurrentUser::StaleCredential),
            ));
        }
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
pub(super) async fn login(
    headers: HeaderMap,
    jar: CookieJar,
    connect_info: Option<Extension<ConnectInfo<SocketAddr>>>,
    SharedStore(cookies): SharedStore<AuthCookies>,
    SharedStore(service): SharedStore<AuthService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    validated: GuestLoginCsrfForm<LoginForm>,
) -> Result<Response> {
    let htmx = is_htmx(&headers);
    let stale_session = validated.stale_session();
    let submitted_token = validated.token().expose().to_owned();
    let form = validated.fields;
    let next = safe_next_destination(Some(&form.next));

    let client_network = connect_info.map_or_else(
        || "socket:unknown".to_owned(),
        |Extension(ConnectInfo(address))| format!("socket:{}", address.ip()),
    );
    let result = service
        .login(LoginCommand {
            email: form.email.clone(),
            password: form.password,
            client_network,
        })
        .await;
    let response = match result {
        Ok(success) => {
            let jar = jar
                .add(cookies.session(success.encoded_jwt().to_owned()))
                .add(cookies.clear_login_csrf());
            return Ok(with_no_store((jar, redirect(&next, htmx)).into_response()));
        }
        Err(LoginError::InvalidInput(errors)) => render_login_error(
            &engine,
            &form.email,
            &next,
            &submitted_token,
            "Check the highlighted fields and try again.",
            errors.email.then_some("Enter a valid email address."),
            errors.password.then_some("Enter your password."),
            StatusCode::UNPROCESSABLE_ENTITY,
            htmx,
        )?,
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
        )?,
        Err(LoginError::Throttled { until }) => {
            let retry = (until - Utc::now())
                .num_seconds()
                .clamp(1, MAX_RETRY_AFTER_SECONDS);
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
            response
        }
        Err(LoginError::Unavailable) => render_unavailable(
            &engine,
            "We could not confirm your sign-in. Please try again shortly.",
            htmx,
        )?,
    };
    Ok(with_optional_stale_cookie(
        response,
        &jar,
        &cookies,
        stale_session,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_login_error(
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

pub(super) fn is_htmx(headers: &HeaderMap) -> bool {
    headers
        .get("HX-Request")
        .is_some_and(|value| value == "true")
}

pub(super) fn redirect(destination: &str, htmx: bool) -> Response {
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

pub(super) fn with_no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub(super) fn with_optional_stale_cookie(
    response: Response,
    jar: &CookieJar,
    cookies: &AuthCookies,
    stale_session: bool,
) -> Response {
    if stale_session {
        with_no_store((jar.clone().add(cookies.clear_session()), response).into_response())
    } else {
        response
    }
}

pub(super) fn render_unavailable(engine: &TeraView, message: &str, htmx: bool) -> Result<Response> {
    let correlation_id = correlation_id();
    tracing::error!(correlation_id, "authentication request unavailable");
    let view = AuthenticationUnavailableView::new(message, &correlation_id);
    let html = if htmx {
        view.render_fragment(engine)?
    } else {
        view.render_page(engine)?
    };
    let mut response = (StatusCode::SERVICE_UNAVAILABLE, format::html(&html)?).into_response();
    response.headers_mut().insert(
        "X-Correlation-ID",
        HeaderValue::from_str(&correlation_id)
            .unwrap_or_else(|_| HeaderValue::from_static("auth-unavailable")),
    );
    append_vary_hx_request(response.headers_mut());
    Ok(with_no_store(response))
}

pub(super) fn correlation_id() -> String {
    let mut random = [0_u8; 8];
    if getrandom::fill(&mut random).is_err() {
        return "auth-unavailable".to_owned();
    }
    let mut identifier = String::with_capacity(21);
    identifier.push_str("auth-");
    for byte in random {
        let _ = write!(&mut identifier, "{byte:02x}");
    }
    identifier
}

/// Authentication routes.
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
