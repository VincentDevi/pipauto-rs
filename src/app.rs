//! Loco application composition for routes, initializers, middleware, and shared services.

use async_trait::async_trait;
use loco_rs::{
    app::{AppContext, Hooks, Initializer},
    bgworker::Queue,
    boot::{create_app, BootResult, StartMode},
    config::Config,
    controller::AppRoutes,
    environment::Environment,
    task::Tasks,
    Result,
};

/// Server-enforced access class declared for every registered application route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessClass {
    /// Non-sensitive infrastructure response available without a session.
    Public,
    /// Sign-in workflow available only before authentication.
    GuestOnly,
    /// Workshop workflow requiring an active session.
    Authenticated,
}

/// Auditable route declaration paired with its access class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteAccess {
    /// HTTP method emitted by Loco's route registry.
    pub method: &'static str,
    /// Exact registered path.
    pub path: &'static str,
    /// Required access boundary.
    pub class: AccessClass,
}

/// Complete access policy for Loco-managed routes. Static assets are middleware-managed and public.
pub const ROUTE_ACCESS_POLICY: &[RouteAccess] = &[
    RouteAccess {
        method: "GET",
        path: "/",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/_health",
        class: AccessClass::Public,
    },
    RouteAccess {
        method: "GET",
        path: "/_health/surrealdb",
        class: AccessClass::Public,
    },
    RouteAccess {
        method: "GET",
        path: "/_ping",
        class: AccessClass::Public,
    },
    RouteAccess {
        method: "GET",
        path: "/_readiness",
        class: AccessClass::Public,
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
        path: "/api/v1/customers",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/api/v1/customers",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/api/v1/customers/{id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "PATCH",
        path: "/api/v1/customers/{id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/api/v1/customers/{id}/archive",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/api/v1/customers/{id}/restore",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/api/v1/customers/{id}/vehicles",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/api/v1/vehicles",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/api/v1/vehicles",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/api/v1/vehicles/{id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "PATCH",
        path: "/api/v1/vehicles/{id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/api/v1/vehicles/{id}/archive",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/api/v1/vehicles/{id}/restore",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/api/v1/vehicles/{id}/service-history",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/api/v1/interventions",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/api/v1/interventions",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/api/v1/interventions/{id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "PATCH",
        path: "/api/v1/interventions/{id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/api/v1/interventions/{id}/complete",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/api/v1/interventions/{id}/cancel",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/api/v1/interventions/{id}/lines",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/api/v1/interventions/{id}/lines",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "PATCH",
        path: "/api/v1/interventions/{id}/lines/{line_id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "DELETE",
        path: "/api/v1/interventions/{id}/lines/{line_id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/api/v1/technical-notes",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/api/v1/technical-notes",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/api/v1/technical-notes/{id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "PATCH",
        path: "/api/v1/technical-notes/{id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/api/v1/technical-notes/{id}/archive",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/api/v1/technical-notes/{id}/restore",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/api/v1/vehicles/{id}/attachments",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/api/v1/vehicles/{id}/attachments",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/api/v1/interventions/{id}/attachments",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "POST",
        path: "/api/v1/interventions/{id}/attachments",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "GET",
        path: "/api/v1/attachments/{id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "PATCH",
        path: "/api/v1/attachments/{id}",
        class: AccessClass::Authenticated,
    },
    RouteAccess {
        method: "DELETE",
        path: "/api/v1/attachments/{id}",
        class: AccessClass::Authenticated,
    },
];

/// Pipauto's Loco application definition.
pub struct App;

#[async_trait]
impl Hooks for App {
    fn app_name() -> &'static str {
        env!("CARGO_CRATE_NAME")
    }

    fn app_version() -> String {
        format!(
            "{} ({})",
            env!("CARGO_PKG_VERSION"),
            option_env!("BUILD_SHA")
                .or(option_env!("GITHUB_SHA"))
                .unwrap_or("dev")
        )
    }

    async fn boot(
        mode: StartMode,
        environment: &Environment,
        config: Config,
    ) -> Result<BootResult> {
        create_app::<Self>(mode, environment, config).await
    }

    async fn after_context(ctx: AppContext) -> Result<AppContext> {
        let business = crate::settings::BusinessSettings::from_config(&ctx.config)
            .map_err(loco_rs::Error::msg)?;
        ctx.shared_store.insert(business);
        crate::initializers::surrealdb::install(&ctx).await?;
        crate::initializers::auth::install(&ctx).await?;
        crate::initializers::business::install(&ctx).await?;
        Ok(ctx)
    }

    async fn initializers(_ctx: &AppContext) -> Result<Vec<Box<dyn Initializer>>> {
        Ok(vec![Box::new(
            crate::initializers::view_engine::ViewEngineInitializer,
        )])
    }

    fn routes(_ctx: &AppContext) -> AppRoutes {
        app_routes()
    }

    async fn connect_workers(_ctx: &AppContext, _queue: &Queue) -> Result<()> {
        Ok(())
    }

    fn register_tasks(tasks: &mut Tasks) {
        tasks.register(crate::tasks::auth::CreateUser);
        tasks.register(crate::tasks::auth_persistence::PurgeExpiredAuthSessions);
    }
}

/// Compose the route registry used by both Loco and the access-policy regression test.
#[must_use]
pub fn app_routes() -> AppRoutes {
    AppRoutes::with_default_routes()
        .add_route(crate::controllers::auth::routes())
        .add_route(crate::controllers::setup::routes())
        .add_route(crate::controllers::surrealdb_health::routes())
        .add_routes(crate::controllers::api_v1::routes())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn protected_routes_require_an_access_class_for_every_registered_route() {
        let registered = app_routes()
            .collect()
            .into_iter()
            .flat_map(|route| {
                route
                    .actions
                    .into_iter()
                    .map(move |method| (method.to_string(), route.uri.clone()))
            })
            .collect::<BTreeSet<_>>();
        let declared = ROUTE_ACCESS_POLICY
            .iter()
            .map(|route| (route.method.to_owned(), route.path.to_owned()))
            .collect::<BTreeSet<_>>();

        assert_eq!(
            registered, declared,
            "update ROUTE_ACCESS_POLICY for every route"
        );

        let documentation = include_str!("../docs/authentication.md");
        for route in ROUTE_ACCESS_POLICY {
            if route.path == "/api/v1" || route.path.starts_with("/api/v1/") {
                assert_eq!(
                    route.class,
                    AccessClass::Authenticated,
                    "every /api/v1 route must be authenticated"
                );
            }
            let signature = format!("`{} {}`", route.method, route.path);
            assert!(
                documentation.contains(&signature),
                "docs/authentication.md must classify {signature}"
            );
        }
    }
}
