//! Server-rendered application setup page and database-status fragment.

use axum::{
    http::{
        header::{CACHE_CONTROL, VARY},
        HeaderValue,
    },
    response::Response,
};
use loco_rs::{
    controller::extractor::shared_store::SharedStore,
    controller::{format, views::engines::TeraView, views::ViewEngine, Routes},
    prelude::get,
    Result,
};

use crate::{
    auth::extractors::CurrentUser,
    controllers::browser::context::BrowserRequestContext,
    services::health::HealthService,
    views::{
        layout::AuthenticatedLayout,
        setup::{SetupPage, SetupStatus},
    },
};

async fn show(
    context: BrowserRequestContext,
    ViewEngine(engine): ViewEngine<TeraView>,
) -> Result<axum::response::Response> {
    let page = SetupPage::new(
        AuthenticatedLayout::new(
            &context.current_user,
            context.csrf_token.expose(),
            &context.current_path,
        ),
        "Pipauto workshop",
        "Pipauto workshop",
        "Your authenticated workshop workspace is ready.",
        "Customer, vehicle, and intervention workflows will be added in the next milestones.",
    );
    let mut response = format::html(&page.render(&engine)?)?;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn status(
    CurrentUser(_user): CurrentUser,
    SharedStore(service): SharedStore<HealthService>,
    ViewEngine(engine): ViewEngine<TeraView>,
) -> Result<Response> {
    let status = if service.available().await {
        SetupStatus::connected()
    } else {
        SetupStatus::unavailable()
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
