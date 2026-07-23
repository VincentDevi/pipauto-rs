use pipauto::settings::{MULTIPART_ENVELOPE_BYTES, MULTIPART_OVERHEAD_BYTES};

const PREVIOUS_GLOBAL_LIMIT: usize = 64 * 1_024;
const _: () = assert!(MULTIPART_ENVELOPE_BYTES > PREVIOUS_GLOBAL_LIMIT);

#[test]
fn route_body_limits_cover_every_unsafe_non_upload_route() {
    let controller_sources = [
        (
            "auth",
            include_str!("../../src/controllers/browser/auth/mod.rs"),
        ),
        (
            "attachments",
            include_str!("../../src/controllers/api_v1/attachments.rs"),
        ),
        (
            "customers",
            include_str!("../../src/controllers/api_v1/customers.rs"),
        ),
        (
            "interventions",
            include_str!("../../src/controllers/api_v1/interventions.rs"),
        ),
        (
            "invoices",
            include_str!("../../src/controllers/api_v1/invoices.rs"),
        ),
        (
            "technical_notes",
            include_str!("../../src/controllers/api_v1/technical_notes.rs"),
        ),
        (
            "vehicles",
            include_str!("../../src/controllers/api_v1/vehicles.rs"),
        ),
        (
            "browser_customers",
            include_str!("../../src/controllers/browser/customers/mod.rs"),
        ),
        (
            "browser_interventions",
            include_str!("../../src/controllers/browser/interventions/mod.rs"),
        ),
        (
            "browser_invoices",
            include_str!("../../src/controllers/browser/invoices/mod.rs"),
        ),
        (
            "browser_knowledge",
            include_str!("../../src/controllers/browser/technical_notes/mod.rs"),
        ),
        (
            "browser_vehicles",
            include_str!("../../src/controllers/browser/vehicles/mod.rs"),
        ),
    ];

    for (controller, source) in controller_sources {
        let route_composition = source
            .find("pub fn routes()")
            .or_else(|| source.find("pub fn guest_routes()"))
            .unwrap_or_else(|| panic!("{controller} must expose route composition"));
        let routes = &source[route_composition..];
        for route in routes.split(".add(").skip(1) {
            let unsafe_method = ["post(", "patch(", "delete(", "put("]
                .iter()
                .any(|method| route.contains(method));
            if unsafe_method {
                assert!(
                    route.contains(".layer("),
                    "{controller} has an unsafe route without an explicit body limit"
                );
            }
        }
    }
}

#[test]
fn route_body_limits_keep_previous_ceiling_outside_multipart_uploads() {
    let api_and_browser_sources = [
        include_str!("../../src/controllers/browser/auth/mod.rs"),
        include_str!("../../src/controllers/api_v1/attachments.rs"),
        include_str!("../../src/controllers/api_v1/customers.rs"),
        include_str!("../../src/controllers/api_v1/interventions.rs"),
        include_str!("../../src/controllers/api_v1/invoices.rs"),
        include_str!("../../src/controllers/api_v1/technical_notes.rs"),
        include_str!("../../src/controllers/api_v1/vehicles.rs"),
        include_str!("../../src/controllers/browser/forms.rs"),
        include_str!("../../src/controllers/browser/technical_notes/mod.rs"),
    ]
    .join("\n");

    assert!(!api_and_browser_sources.contains("128 * 1024"));
    assert!(!api_and_browser_sources.contains("128 * 1_024"));
    assert_eq!(MULTIPART_OVERHEAD_BYTES, PREVIOUS_GLOBAL_LIMIT);
}
