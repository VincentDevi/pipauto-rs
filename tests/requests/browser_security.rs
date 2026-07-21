use std::{fs, path::Path};

use pipauto::{app::AccessClass, controllers::browser::ROUTE_INVENTORY};

#[test]
fn browser_security_requires_authentication_for_every_private_route() {
    assert!(ROUTE_INVENTORY
        .iter()
        .all(|route| route.path == "/login" || route.class == AccessClass::Authenticated));
}

#[test]
fn browser_security_unsafe_forms_share_native_htmx_and_csrf_behavior() {
    for path in html_files("assets/views") {
        let html = fs::read_to_string(&path).expect("template should be readable");
        for form in html.split("<form").skip(1) {
            let form = form.split("</form>").next().expect("form should close");
            if !form.contains("method=\"post\"") {
                continue;
            }
            assert!(
                form.contains("hx-post="),
                "{} has a POST form without equivalent HTMX behavior",
                path.display()
            );
            assert!(
                form.contains("name=\"_csrf\""),
                "{} has a POST form without a standard-form CSRF value",
                path.display()
            );
        }
    }
}

#[test]
fn browser_security_client_hardening_recovers_uncertain_mutations() {
    let script = fs::read_to_string("assets/static/js/app.js").expect("app script");
    assert!(script.contains("X-CSRF-Token"));
    assert!(script.contains("htmx:sendError"));
    assert!(script.contains("htmx:timeout"));
    assert!(script.contains("Reload the latest workshop record before trying again."));
    assert!(script.contains("The current Calendar remains available"));
    assert!(script.contains("[422, 500, 503]"));
    assert!(script.contains("focusKey"));

    let calendar =
        fs::read_to_string("assets/views/fragments/calendar.html").expect("calendar template");
    assert!(calendar.contains("data-focus-key=\"calendar-next\""));

    let extractor = fs::read_to_string("src/auth/extractors.rs").expect("auth extractor");
    assert!(extractor.contains("HX-Redirect"));
    assert!(extractor.contains("clear_session()"));
    assert!(extractor.contains("no_store(response)"));
}

fn html_files(directory: &str) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(Path::new(directory)).expect("template directory") {
        let path = entry.expect("template entry").path();
        if path.is_dir() {
            files.extend(html_files(path.to_str().expect("UTF-8 path")));
        } else if path
            .extension()
            .is_some_and(|extension| extension == "html")
        {
            files.push(path);
        }
    }
    files
}
