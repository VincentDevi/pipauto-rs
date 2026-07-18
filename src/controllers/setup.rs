//! Server-rendered application setup page.

use loco_rs::{
    controller::{format, views::engines::TeraView, views::ViewEngine, Routes},
    prelude::get,
    Result,
};

use crate::views::setup::SetupPage;

async fn show(ViewEngine(engine): ViewEngine<TeraView>) -> Result<axum::response::Response> {
    let page = SetupPage::new(
        "Pipauto setup",
        "Pipauto setup",
        "The application foundation is running.",
        "Server-rendered pages and static assets are ready for the next workshop workflows.",
    );
    format::html(&page.render(&engine)?)
}

/// Routes exposed by the setup controller.
#[must_use]
pub fn routes() -> Routes {
    Routes::new().add("/", get(show))
}
