use std::{fs, path::Path};

use pipauto::{
    app::{AccessClass, ROUTE_ACCESS_POLICY},
    controllers::browser::ROUTE_INVENTORY,
};

#[test]
fn browser_foundation_has_an_auditable_authenticated_route_inventory() {
    assert!(ROUTE_INVENTORY
        .iter()
        .filter(|route| route.path != "/login")
        .all(|route| route.class == AccessClass::Authenticated));
    for browser_route in ROUTE_INVENTORY {
        assert!(ROUTE_ACCESS_POLICY.contains(browser_route));
    }
}

#[test]
fn browser_route_inventory_includes_the_authenticated_calendar_read_path() {
    let calendar = pipauto::app::RouteAccess {
        method: "GET",
        path: "/calendar",
        class: AccessClass::Authenticated,
    };
    assert!(ROUTE_INVENTORY.contains(&calendar));
    assert!(ROUTE_ACCESS_POLICY.contains(&calendar));
}

#[test]
fn browser_foundation_preserves_controller_service_repository_direction() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut browser_sources = rust_sources(&root.join("src/controllers/browser"));
    browser_sources.extend(rust_sources(&root.join("src/views")));
    browser_sources.push(root.join("src/controllers/setup.rs"));
    let forbidden = [
        ["repositories", "surreal"].join("::"),
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
