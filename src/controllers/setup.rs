//! Server-rendered application setup page and database-status fragment.

use axum::{
    http::{
        header::{CACHE_CONTROL, VARY},
        HeaderValue,
    },
    response::Response,
};
use axum_extra::extract::cookie::CookieJar;
use loco_rs::{
    controller::extractor::shared_store::SharedStore,
    controller::{format, views::engines::TeraView, views::ViewEngine, Routes},
    prelude::get,
    Result,
};

use crate::{
    auth::{csrf::CsrfService, extractors::CurrentUser, settings::AuthSettings},
    database::client::AppDatabase,
    services::auth::AuthService,
    views::setup::{SetupPage, SetupStatus},
};

async fn show(
    CurrentUser(user): CurrentUser,
    jar: CookieJar,
    SharedStore(settings): SharedStore<AuthSettings>,
    SharedStore(service): SharedStore<AuthService>,
    SharedStore(csrf): SharedStore<CsrfService>,
    ViewEngine(engine): ViewEngine<TeraView>,
) -> Result<axum::response::Response> {
    let encoded = jar
        .get(settings.session_cookie_name())
        .map(axum_extra::extract::cookie::Cookie::value)
        .ok_or_else(|| loco_rs::Error::string("session cookie is unavailable"))?;
    let session = service
        .authenticate_session(encoded)
        .await
        .map_err(loco_rs::Error::msg)?;
    let csrf_token = csrf
        .issue_authenticated(&session.jti, user.session_expires_at)
        .map_err(loco_rs::Error::msg)?;
    let page = SetupPage::new(
        "Pipauto workshop",
        "Pipauto workshop",
        "Your authenticated workshop workspace is ready.",
        "Customer, vehicle, and intervention workflows will be added in the next milestones.",
        &user.display_name,
        csrf_token.expose(),
    );
    let mut response = format::html(&page.render(&engine)?)?;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn status(
    CurrentUser(_user): CurrentUser,
    SharedStore(database): SharedStore<AppDatabase>,
    ViewEngine(engine): ViewEngine<TeraView>,
) -> Result<Response> {
    let status = match database.health().await {
        Ok(()) => SetupStatus::connected(),
        Err(_) => SetupStatus::unavailable(),
    };
    let mut response = format::html(&status.render(&engine)?)?;
    response
        .headers_mut()
        .insert(VARY, HeaderValue::from_static("HX-Request"));
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

/// Routes exposed by the setup controller.
#[must_use]
pub fn routes() -> Routes {
    Routes::new()
        .add("/", get(show))
        .add("/setup/status", get(status))
}
