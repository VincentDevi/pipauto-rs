//! Composition boundary for authenticated JSON business controllers.
//!
//! Domain controllers contribute unprefixed Loco [`Routes`]. This module applies the shared
//! `/api/v1` prefix and sensitive-response cache policy without introducing another router.

use loco_rs::controller::Routes;

use crate::routing::{AccessClass, ClassifiedRoutes};

mod attachments;
mod customers;
mod interventions;
mod invoices;
mod technical_notes;
mod vehicles;

pub const API_V1_PREFIX: &str = "/api/v1";

/// Apply the shared API prefix and response policy to one domain controller's routes.
#[must_use]
pub fn mount(routes: Routes) -> Routes {
    routes
        .prefix(API_V1_PREFIX)
        .layer(axum::middleware::from_fn(
            crate::auth::responses::no_store_layer,
        ))
}

/// Compose all API domain controllers as authenticated groups.
#[must_use]
pub fn route_groups() -> Vec<ClassifiedRoutes> {
    vec![
        ClassifiedRoutes::new(mount(customers::routes()), AccessClass::Authenticated),
        ClassifiedRoutes::new(mount(vehicles::routes()), AccessClass::Authenticated),
        ClassifiedRoutes::new(mount(interventions::routes()), AccessClass::Authenticated),
        ClassifiedRoutes::new(mount(technical_notes::routes()), AccessClass::Authenticated),
        ClassifiedRoutes::new(mount(attachments::routes()), AccessClass::Authenticated),
        ClassifiedRoutes::new(mount(invoices::routes()), AccessClass::Authenticated),
    ]
}

/// Return the API route registry used by Loco.
#[must_use]
pub fn routes() -> Vec<Routes> {
    route_groups()
        .into_iter()
        .map(ClassifiedRoutes::into_routes)
        .collect()
}

#[cfg(test)]
mod tests {
    use loco_rs::{controller::Routes, prelude::get};

    use super::*;

    async fn probe() {}

    #[test]
    fn api_foundation_mounts_loco_routes_under_the_versioned_prefix() {
        let routes = mount(Routes::new().add("/probe", get(probe)));

        assert_eq!(routes.prefix.as_deref(), Some("/api/v1"));
        assert_eq!(routes.handlers.len(), 1);
        assert_eq!(routes.handlers[0].uri, "/probe");
    }
}
