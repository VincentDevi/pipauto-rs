//! Composition boundary for authenticated JSON business controllers.
//!
//! Domain controllers contribute unprefixed Loco [`Routes`]. This module applies the shared
//! `/api/v1` prefix and sensitive-response cache policy without introducing another router.

use loco_rs::controller::Routes;

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

/// Compose all API domain controllers.
///
/// VIN-45 establishes the transport boundary. Domain routes are added here by VIN-46 through
/// VIN-49 and must also be declared authenticated in `ROUTE_ACCESS_POLICY`.
#[must_use]
pub fn routes() -> Vec<Routes> {
    vec![
        mount(crate::controllers::customers::routes()),
        mount(crate::controllers::vehicles::routes()),
    ]
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
