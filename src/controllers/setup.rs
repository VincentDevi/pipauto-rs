//! Authenticated database-status fragment retained for development diagnostics.

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
    auth::extractors::CurrentUser, services::health::HealthService, views::setup::SetupStatus,
};

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
    Routes::new().add("/setup/status", get(status))
}
