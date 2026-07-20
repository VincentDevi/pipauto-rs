//! Server-rendered browser boundary, mounted independently from `/api/v1`.
//!
//! Browser controllers call application services directly. They never make loopback HTTP requests
//! and never depend on database clients or concrete persistence adapters. Business pages remain
//! safe unavailable placeholders until their owning frontend issue implements them.

pub mod context;
mod customers;
pub mod forms;
mod interventions;
mod invoices;
mod knowledge;
pub mod responses;
mod vehicles;

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
        path: "/vehicles/new",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/vehicles",
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
        method: "POST",
        path: "/vehicles/{id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/vehicles/{id}/archive",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/vehicles/{id}/restore",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/vehicles/{id}/reassign",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/vehicles/{id}/reassign",
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
        method: "POST",
        path: "/vehicles/{id}/interventions",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/vehicles/{id}/attachments/new",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/vehicles/{id}/attachments",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/attachments/{id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/attachments/{id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/attachments/{id}/delete",
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
        method: "POST",
        path: "/interventions/{id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/interventions/{id}/lines/new",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/interventions/{id}/lines",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/interventions/{id}/lines/{line_id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/interventions/{id}/lines/{line_id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/interventions/{id}/lines/{line_id}/delete",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/interventions/{id}/lines/{line_id}/move-up",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/interventions/{id}/lines/{line_id}/move-down",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/interventions/{id}/attachments/new",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/interventions/{id}/attachments",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/interventions/{id}/attachments/{attachment_id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/interventions/{id}/attachments/{attachment_id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/interventions/{id}/attachments/{attachment_id}/delete",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/interventions/{id}/complete",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/interventions/{id}/complete",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/interventions/{id}/cancel",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/interventions/{id}/cancel",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/knowledge",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
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
        method: "POST",
        path: "/knowledge/{id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/knowledge/{id}/archive",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/knowledge/{id}/restore",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/invoices",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
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
    RouteAccess {
        method: "POST",
        path: "/invoices/{id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/invoices/{id}/lines/new",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/invoices/{id}/lines",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/invoices/{id}/lines/{line_id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/invoices/{id}/lines/{line_id}/edit",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/invoices/{id}/lines/{line_id}/delete",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/invoices/{id}/lines/{line_id}/move-up",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/invoices/{id}/lines/{line_id}/move-down",
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
        mount(interventions::routes()),
        mount(invoices::routes()),
        mount(knowledge::routes()),
        mount(vehicles::routes()),
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
