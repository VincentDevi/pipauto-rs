//! Server-rendered browser boundary, mounted independently from `/api/v1`.
//!
//! Browser controllers call application services directly. They never make loopback HTTP requests
//! and never depend on database clients or concrete persistence adapters. Business pages remain
//! safe unavailable placeholders until their owning frontend issue implements them.

mod attachments;
mod auth;
mod calendar;
pub mod context;
mod customers;
mod dashboard;
pub mod forms;
mod interventions;
mod invoices;
pub mod responses;
mod route_inventory;
mod setup;
mod technical_notes;
mod vehicles;

pub use route_inventory::ROUTE_INVENTORY;

use loco_rs::controller::Routes;

/// Apply browser-wide sensitive-response policy without changing API routing.
#[must_use]
pub fn mount(routes: Routes) -> Routes {
    routes.layer(axum::middleware::from_fn(
        crate::auth::responses::no_store_layer,
    ))
}

/// Compose guest, shell, and implemented browser routes.
#[must_use]
pub fn routes() -> Vec<Routes> {
    vec![
        mount(auth::routes()),
        mount(dashboard::routes()),
        mount(setup::routes()),
        mount(calendar::routes()),
        mount(attachments::routes()),
        mount(customers::routes()),
        mount(interventions::routes()),
        mount(invoices::routes()),
        mount(technical_notes::routes()),
        mount(vehicles::routes()),
    ]
}
