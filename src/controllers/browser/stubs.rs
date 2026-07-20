//! Safe placeholders for planned browser pages not yet owned by an implementation issue.

use axum::{http::StatusCode, response::Response};
use loco_rs::{
    controller::{views::engines::TeraView, views::ViewEngine, Routes},
    prelude::get,
    Result,
};

use super::{context::BrowserRequestContext, responses};
use crate::views::{layout::AuthenticatedLayout, unavailable::UnavailablePage};

async fn unavailable(
    context: BrowserRequestContext,
    ViewEngine(engine): ViewEngine<TeraView>,
) -> Result<Response> {
    let view = UnavailablePage::new(AuthenticatedLayout::new(
        &context.current_user,
        context.csrf_token.expose(),
        &context.current_path,
    ));
    Ok(responses::render(
        context.response_preference,
        StatusCode::NOT_IMPLEMENTED,
        view.render_page(&engine)?,
        view.render_panel(&engine)?,
    ))
}

/// Planned routes retain safe placeholders until their owning issue implements them.
#[must_use]
pub fn routes() -> Routes {
    Routes::new()
        .add("/knowledge", get(unavailable))
        .add("/knowledge/new", get(unavailable))
        .add("/knowledge/{id}", get(unavailable))
        .add("/knowledge/{id}/edit", get(unavailable))
        .add("/invoices", get(unavailable))
        .add("/invoices/new", get(unavailable))
        .add("/invoices/{id}", get(unavailable))
        .add("/invoices/{id}/edit", get(unavailable))
}
