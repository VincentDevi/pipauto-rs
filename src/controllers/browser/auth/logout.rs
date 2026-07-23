use super::login::*;
use super::*;
pub(super) async fn logout(
    headers: HeaderMap,
    jar: CookieJar,
    SharedStore(cookies): SharedStore<AuthCookies>,
    SharedStore(service): SharedStore<AuthService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    validated: LogoutCsrfForm<LogoutForm>,
) -> Result<Response> {
    let htmx = is_htmx(&headers);
    let outcome = service.logout(validated.encoded_jwt()).await;
    let jar = jar.add(cookies.clear_session());
    match outcome {
        Ok(_) => Ok(with_no_store(
            (jar, redirect("/login", htmx)).into_response(),
        )),
        Err(_) => {
            let response = render_unavailable(
                &engine,
                "Your browser was signed out, but server-side logout could not be confirmed. Please try again before signing back in.",
                htmx,
            )?;
            Ok(with_no_store((jar, response).into_response()))
        }
    }
}
