use pipauto::settings::{MULTIPART_ENVELOPE_BYTES, MULTIPART_OVERHEAD_BYTES};

const PREVIOUS_GLOBAL_LIMIT: usize = 64 * 1_024;

#[test]
fn route_body_limits_cover_every_unsafe_non_upload_route() {
    let controller_sources = [
        ("auth", include_str!("../../src/controllers/auth.rs")),
        (
            "attachments",
            include_str!("../../src/controllers/attachments.rs"),
        ),
        (
            "customers",
            include_str!("../../src/controllers/customers.rs"),
        ),
        (
            "interventions",
            include_str!("../../src/controllers/interventions.rs"),
        ),
        (
            "invoices",
            include_str!("../../src/controllers/invoices.rs"),
        ),
        (
            "technical_notes",
            include_str!("../../src/controllers/technical_notes.rs"),
        ),
        (
            "vehicles",
            include_str!("../../src/controllers/vehicles.rs"),
        ),
        (
            "browser_customers",
            include_str!("../../src/controllers/browser/customers.rs"),
        ),
        (
            "browser_interventions",
            include_str!("../../src/controllers/browser/interventions.rs"),
        ),
        (
            "browser_invoices",
            include_str!("../../src/controllers/browser/invoices.rs"),
        ),
        (
            "browser_knowledge",
            include_str!("../../src/controllers/browser/knowledge.rs"),
        ),
        (
            "browser_vehicles",
            include_str!("../../src/controllers/browser/vehicles.rs"),
        ),
    ];

    for (controller, source) in controller_sources {
        let routes = source
            .split_once("pub fn routes()")
            .unwrap_or_else(|| panic!("{controller} must expose a routes function"))
            .1;
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
        include_str!("../../src/controllers/auth.rs"),
        include_str!("../../src/controllers/attachments.rs"),
        include_str!("../../src/controllers/customers.rs"),
        include_str!("../../src/controllers/interventions.rs"),
        include_str!("../../src/controllers/invoices.rs"),
        include_str!("../../src/controllers/technical_notes.rs"),
        include_str!("../../src/controllers/vehicles.rs"),
        include_str!("../../src/controllers/browser/forms.rs"),
        include_str!("../../src/controllers/browser/knowledge.rs"),
    ]
    .join("\n");

    assert!(!api_and_browser_sources.contains("128 * 1024"));
    assert!(!api_and_browser_sources.contains("128 * 1_024"));
    assert_eq!(MULTIPART_OVERHEAD_BYTES, PREVIOUS_GLOBAL_LIMIT);
    assert!(MULTIPART_ENVELOPE_BYTES > PREVIOUS_GLOBAL_LIMIT);
}
