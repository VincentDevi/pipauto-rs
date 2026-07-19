//! Safe placeholders for planned browser pages not yet owned by an implementation issue.

use axum::response::Response;
use loco_rs::{controller::Routes, prelude::get};

use super::{context::BrowserRequestContext, responses};

async fn unavailable(context: BrowserRequestContext) -> Response {
    responses::not_implemented(context.response_preference)
}

/// Planned routes are intentionally absent from active navigation until implemented.
#[must_use]
pub fn routes() -> Routes {
    Routes::new()
        .add("/customers", get(unavailable))
        .add("/customers/new", get(unavailable))
        .add("/customers/{id}", get(unavailable))
        .add("/customers/{id}/edit", get(unavailable))
        .add("/customers/{id}/vehicles/new", get(unavailable))
        .add("/vehicles", get(unavailable))
        .add("/vehicles/{id}", get(unavailable))
        .add("/vehicles/{id}/edit", get(unavailable))
        .add("/vehicles/{id}/history", get(unavailable))
        .add("/vehicles/{id}/interventions/new", get(unavailable))
        .add("/interventions", get(unavailable))
        .add("/interventions/{id}", get(unavailable))
        .add("/interventions/{id}/edit", get(unavailable))
        .add("/knowledge", get(unavailable))
        .add("/knowledge/new", get(unavailable))
        .add("/knowledge/{id}", get(unavailable))
        .add("/knowledge/{id}/edit", get(unavailable))
        .add("/invoices", get(unavailable))
        .add("/invoices/new", get(unavailable))
        .add("/invoices/{id}", get(unavailable))
        .add("/invoices/{id}/edit", get(unavailable))
}
