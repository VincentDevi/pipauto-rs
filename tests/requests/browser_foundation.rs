use std::{fs, path::Path};

use axum::http::Method;
use pipauto::{
    app::route_access_inventory,
    controllers::browser::route_inventory,
    routing::{AccessClass, RouteAccess},
};

#[test]
fn browser_foundation_has_an_auditable_authenticated_route_inventory() {
    let browser_inventory = route_inventory();
    let login_routes = browser_inventory
        .iter()
        .filter(|route| route.path == "/login")
        .collect::<Vec<_>>();
    assert_eq!(login_routes.len(), 2);
    assert!(login_routes
        .iter()
        .all(|route| route.class == AccessClass::GuestOnly));
    assert!(browser_inventory
        .iter()
        .filter(|route| route.path != "/login")
        .all(|route| route.class == AccessClass::Authenticated));
    let complete_inventory = route_access_inventory();
    for browser_route in browser_inventory {
        assert!(complete_inventory.contains(&browser_route));
    }
}

#[test]
fn browser_route_inventory_includes_the_authenticated_calendar_read_path() {
    let calendar = RouteAccess {
        method: Method::GET,
        path: "/calendar".to_owned(),
        class: AccessClass::Authenticated,
    };
    assert!(route_inventory().contains(&calendar));
    assert!(route_access_inventory().contains(&calendar));
}

#[test]
fn browser_foundation_preserves_controller_model_persistence_direction() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut browser_sources = rust_sources(&root.join("src/controllers/browser"));
    browser_sources.extend(rust_sources(&root.join("src/views")));
    let forbidden = [
        ["models", "persistence"].join("::"),
        ["database", ""].join("::"),
        ["surrealdb", ""].join("::"),
        ["http", "//127.0.0.1"].join(":"),
        ["http", "//localhost"].join(":"),
    ];

    for path in browser_sources {
        let source = fs::read_to_string(&path).expect("source should be readable");
        for dependency in &forbidden {
            assert!(
                !source.contains(dependency),
                "{} crosses browser boundary through {dependency}",
                path.display()
            );
        }
    }
}

fn rust_sources(directory: &Path) -> Vec<std::path::PathBuf> {
    let mut sources = Vec::new();
    for entry in fs::read_dir(directory).expect("source directory should be readable") {
        let path = entry.expect("directory entry should be readable").path();
        if path.is_dir() {
            sources.extend(rust_sources(&path));
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path);
        }
    }
    sources
}
