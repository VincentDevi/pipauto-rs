//! Access classification derived from registered Loco route groups.

use axum::http::Method;
use loco_rs::controller::{AppRoutes, Routes};

/// Access boundary declared for a homogeneous registered route group.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum AccessClass {
    /// Non-sensitive infrastructure response available without a session.
    Public,
    /// Sign-in workflow available only before authentication.
    GuestOnly,
    /// Workshop workflow requiring an active session.
    Authenticated,
}

/// Auditable registered route paired with its access class.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteAccess {
    /// HTTP method emitted by Loco's route registry.
    pub method: Method,
    /// Exact registered path after Loco prefix normalization.
    pub path: String,
    /// Required access boundary.
    pub class: AccessClass,
}

/// One Loco route group whose handlers share an access boundary.
#[derive(Clone, Debug)]
pub struct ClassifiedRoutes {
    routes: Routes,
    class: AccessClass,
}

impl ClassifiedRoutes {
    /// Pair a homogeneous route group with its access boundary.
    #[must_use]
    pub const fn new(routes: Routes, class: AccessClass) -> Self {
        Self { routes, class }
    }

    /// Expand the registered methods and normalized paths into audit entries.
    #[must_use]
    pub fn inventory(&self) -> Vec<RouteAccess> {
        inventory_for(
            AppRoutes::empty().add_route(self.routes.clone()),
            self.class,
        )
    }

    /// Return the classified Loco group for application registration.
    #[must_use]
    pub fn into_routes(self) -> Routes {
        self.routes
    }
}

/// Expand an application route registry under one access class.
#[must_use]
pub(crate) fn inventory_for(routes: AppRoutes, class: AccessClass) -> Vec<RouteAccess> {
    routes
        .collect()
        .into_iter()
        .flat_map(|route| {
            route.actions.into_iter().map(move |method| RouteAccess {
                method,
                path: route.uri.clone(),
                class,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use loco_rs::{
        controller::Routes,
        prelude::{get, post},
    };

    use super::*;

    async fn probe() {}

    #[test]
    fn inventory_uses_loco_prefix_and_path_normalization_for_each_method() {
        let group = ClassifiedRoutes::new(
            Routes::new()
                .add("/probe/", get(probe))
                .add("/probe/", post(probe))
                .prefix("/api/v1/"),
            AccessClass::Authenticated,
        );

        let inventory = group.inventory();

        assert_eq!(inventory.len(), 2);
        assert!(inventory.iter().any(|route| {
            route.method == Method::GET
                && route.path == "/api/v1/probe"
                && route.class == AccessClass::Authenticated
        }));
        assert!(inventory.iter().any(|route| {
            route.method == Method::POST
                && route.path == "/api/v1/probe"
                && route.class == AccessClass::Authenticated
        }));
    }
}
