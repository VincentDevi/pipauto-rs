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

use crate::routing::{AccessClass, ClassifiedRoutes, RouteAccess};

#[cfg(test)]
const PINNED_FRAMEWORK_DEFAULT_ROUTES: &[(&str, &str)] = &[
    ("GET", "/_health"),
    ("GET", "/_ping"),
    ("GET", "/_readiness"),
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
        let workshop_time = crate::domain::WorkshopTime::system(business.workshop_timezone());
        ctx.shared_store.insert(business);
        ctx.shared_store.insert(workshop_time);
        let attachments = crate::settings::AttachmentSettings::from_config(&ctx.config)
            .map_err(loco_rs::Error::msg)?;
        ctx.shared_store.insert(attachments);
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
        tasks.register(crate::tasks::attachment_reconciliation::ReconcileAttachments);
    }
}

fn application_route_groups() -> Vec<ClassifiedRoutes> {
    let mut groups = crate::controllers::browser::route_groups();
    groups.push(ClassifiedRoutes::new(
        crate::controllers::health::routes(),
        AccessClass::Public,
    ));
    groups.extend(crate::controllers::api_v1::route_groups());
    groups
}

fn framework_default_route_inventory() -> Vec<RouteAccess> {
    crate::routing::inventory_for(AppRoutes::with_default_routes(), AccessClass::Public)
}

/// Compose the route registry used by Loco.
#[must_use]
pub fn app_routes() -> AppRoutes {
    application_route_groups()
        .into_iter()
        .fold(AppRoutes::with_default_routes(), |routes, group| {
            routes.add_route(group.into_routes())
        })
}

/// Generate the access-policy inventory for every Loco-managed route.
#[must_use]
pub fn route_access_inventory() -> Vec<RouteAccess> {
    let mut inventory = framework_default_route_inventory();
    inventory.extend(
        application_route_groups()
            .iter()
            .flat_map(ClassifiedRoutes::inventory),
    );
    inventory
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    fn signatures(routes: &[RouteAccess]) -> BTreeSet<(String, String)> {
        routes
            .iter()
            .map(|route| (route.method.to_string(), route.path.clone()))
            .collect()
    }

    #[test]
    fn framework_default_routes_are_explicitly_pinned() {
        let actual = signatures(&framework_default_route_inventory());
        let expected = PINNED_FRAMEWORK_DEFAULT_ROUTES
            .iter()
            .map(|(method, path)| ((*method).to_owned(), (*path).to_owned()))
            .collect();

        assert_eq!(
            actual, expected,
            "review Loco framework route changes before classifying them as public"
        );
    }

    #[test]
    fn generated_policy_matches_every_registered_route_once() {
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
        let inventory = route_access_inventory();
        let mut classifications = BTreeMap::new();

        for route in &inventory {
            let signature = (route.method.to_string(), route.path.clone());
            if let Some(existing) = classifications.insert(signature.clone(), route.class) {
                assert_eq!(
                    existing, route.class,
                    "conflicting access classes for {} {}",
                    signature.0, signature.1
                );
                panic!(
                    "duplicate route classification for {} {}",
                    signature.0, signature.1
                );
            }
        }

        assert_eq!(registered, signatures(&inventory));
    }

    #[test]
    fn generated_policy_preserves_access_and_documentation_boundaries() {
        let inventory = route_access_inventory();
        let public = inventory
            .iter()
            .filter(|route| route.class == AccessClass::Public)
            .map(|route| (route.method.to_string(), route.path.clone()))
            .collect::<BTreeSet<_>>();
        let expected_public = [
            ("GET".to_owned(), "/_health".to_owned()),
            ("GET".to_owned(), "/_health/surrealdb".to_owned()),
            ("GET".to_owned(), "/_ping".to_owned()),
            ("GET".to_owned(), "/_readiness".to_owned()),
        ]
        .into_iter()
        .collect();

        assert_eq!(public, expected_public);

        let authentication_documentation = include_str!("../docs/authentication.md");
        let api_documentation = include_str!("../docs/api-v1.md");
        for route in &inventory {
            if route.path == "/api/v1" || route.path.starts_with("/api/v1/") {
                assert_eq!(
                    route.class,
                    AccessClass::Authenticated,
                    "every /api/v1 route must be authenticated"
                );
            }
            let signature = format!("`{} {}`", route.method, route.path);
            let documentation = if route.path.starts_with("/api/v1/") {
                api_documentation
            } else {
                authentication_documentation
            };
            assert!(
                documentation.contains(&signature),
                "the route documentation must classify {signature}"
            );
        }
    }
}
