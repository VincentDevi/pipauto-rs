//! Server-rendered browser boundary, mounted independently from `/api/v1`.
//!
//! Browser controllers call application services directly. They never make loopback HTTP requests
//! and never depend on database clients or concrete persistence adapters. Business pages remain
//! safe unavailable placeholders until their owning frontend issue implements them.

pub mod context;
mod customers;
pub mod forms;
pub mod responses;
mod stubs;

use loco_rs::controller::Routes;

use crate::app::{AccessClass, RouteAccess};

/// Auditable inventory for every server-rendered browser route.
pub const ROUTE_INVENTORY: &[RouteAccess] = &[
    RouteAccess {
        method: "GET",
        path: "/",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/dashboard/recent-interventions",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/dashboard/draft-interventions",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/login",
        class: AccessClass::GuestOnly,
    },
    RouteAccess {
        method: "POST",
        path: "/login",
        class: AccessClass::GuestOnly,
    },
    RouteAccess {
        method: "POST",
        path: "/logout",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/setup/status",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/customers",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/customers",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/customers/new",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/customers/{id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/customers/{id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/customers/{id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/customers/{id}/archive",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/customers/{id}/restore",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/customers/{id}/vehicles/new",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/vehicles",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/vehicles/{id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/vehicles/{id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/vehicles/{id}/history",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/vehicles/{id}/interventions/new",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/interventions",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/interventions/{id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/interventions/{id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/knowledge",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/knowledge/new",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/knowledge/{id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/knowledge/{id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/invoices",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/invoices/new",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/invoices/{id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/invoices/{id}/edit",
        class: AccessClass::Authenticated,
    },
];

/// Apply browser-wide sensitive-response policy without changing API routing.
#[must_use]
pub fn mount(routes: Routes) -> Routes {
    routes.layer(axum::middleware::from_fn(
        crate::auth::responses::no_store_layer,
    ))
}

/// Compose guest, shell, and planned browser routes.
#[must_use]
pub fn routes() -> Vec<Routes> {
    vec![
        mount(crate::controllers::auth::routes()),
        mount(crate::controllers::dashboard::routes()),
        mount(crate::controllers::setup::routes()),
        mount(customers::routes()),
        mount(stubs::routes()),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn route_access_policy_classifies_every_browser_route() {
        let registered = routes()
            .into_iter()
            .flat_map(|routes| {
                let prefix = routes.prefix.unwrap_or_default();
                routes.handlers.into_iter().flat_map(move |handler| {
                    let path = format!("{prefix}{}", handler.uri);
                    handler
                        .actions
                        .into_iter()
                        .map(move |method| (method.to_string(), path.clone()))
                })
            })
            .collect::<BTreeSet<_>>();
        let declared = ROUTE_INVENTORY
            .iter()
            .map(|route| (route.method.to_owned(), route.path.to_owned()))
            .collect::<BTreeSet<_>>();

        assert_eq!(registered, declared);
        assert!(ROUTE_INVENTORY.iter().all(|route| matches!(
            route.class,
            AccessClass::Authenticated | AccessClass::GuestOnly
        )));
    }
}
