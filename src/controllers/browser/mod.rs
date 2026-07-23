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
mod setup;
mod technical_notes;
mod vehicles;

use loco_rs::controller::Routes;

use crate::routing::{AccessClass, ClassifiedRoutes, RouteAccess};

/// Apply browser-wide sensitive-response policy without changing API routing.
#[must_use]
pub fn mount(routes: Routes) -> Routes {
    routes.layer(axum::middleware::from_fn(
        crate::auth::responses::no_store_layer,
    ))
}

/// Compose classified guest, shell, and implemented browser route groups.
#[must_use]
pub fn route_groups() -> Vec<ClassifiedRoutes> {
    vec![
        ClassifiedRoutes::new(mount(auth::guest_routes()), AccessClass::GuestOnly),
        ClassifiedRoutes::new(
            mount(auth::authenticated_routes()),
            AccessClass::Authenticated,
        ),
        ClassifiedRoutes::new(mount(dashboard::routes()), AccessClass::Authenticated),
        ClassifiedRoutes::new(mount(setup::routes()), AccessClass::Authenticated),
        ClassifiedRoutes::new(mount(calendar::routes()), AccessClass::Authenticated),
        ClassifiedRoutes::new(mount(attachments::routes()), AccessClass::Authenticated),
        ClassifiedRoutes::new(mount(customers::routes()), AccessClass::Authenticated),
        ClassifiedRoutes::new(mount(interventions::routes()), AccessClass::Authenticated),
        ClassifiedRoutes::new(mount(invoices::routes()), AccessClass::Authenticated),
        ClassifiedRoutes::new(mount(technical_notes::routes()), AccessClass::Authenticated),
        ClassifiedRoutes::new(mount(vehicles::routes()), AccessClass::Authenticated),
    ]
}

/// Return the browser route registry used by Loco.
#[must_use]
pub fn routes() -> Vec<Routes> {
    route_groups()
        .into_iter()
        .map(ClassifiedRoutes::into_routes)
        .collect()
}

/// Generate the auditable browser route inventory.
#[must_use]
pub fn route_inventory() -> Vec<RouteAccess> {
    route_groups()
        .iter()
        .flat_map(ClassifiedRoutes::inventory)
        .collect()
}
