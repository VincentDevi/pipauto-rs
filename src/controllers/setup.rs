//! Server-rendered application setup page and database-status fragment.

use axum::{
    http::{header::VARY, HeaderValue},
    response::Response,
};
use loco_rs::{
    controller::extractor::shared_store::SharedStore,
    controller::{format, views::engines::TeraView, views::ViewEngine, Routes},
    prelude::get,
    Result,
};

use crate::{
    database::client::AppDatabase,
    views::setup::{SetupPage, SetupStatus},
};

async fn show(ViewEngine(engine): ViewEngine<TeraView>) -> Result<axum::response::Response> {
    let page = SetupPage::new(
        "Pipauto setup",
        "Pipauto setup",
        "The application foundation is running.",
        "Server-rendered pages and static assets are ready for the next workshop workflows.",
    );
    format::html(&page.render(&engine)?)
}

async fn status(
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
    Ok(response)
}

/// Routes exposed by the setup controller.
#[must_use]
pub fn routes() -> Routes {
    Routes::new()
        .add("/", get(show))
        .add("/setup/status", get(status))
}
