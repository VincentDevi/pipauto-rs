//! Typed, presentation-safe data shared by authenticated application pages.

use serde::Serialize;

use crate::views::context::PresentationUser;

/// Authenticated shell data with no record, session, email, or credential identifiers.
#[derive(Debug, Serialize)]
pub struct AuthenticatedLayout<'layout> {
    current_user: &'layout PresentationUser,
    csrf_token: &'layout str,
    current_path: &'layout str,
    active_navigation: &'static str,
    current_area: &'static str,
}

impl<'layout> AuthenticatedLayout<'layout> {
    /// Project an authenticated request into fields safe for shell rendering.
    #[must_use]
    pub fn new(
        user: &'layout PresentationUser,
        csrf_token: &'layout str,
        current_path: &'layout str,
    ) -> Self {
        let (active_navigation, current_area) = navigation_area(current_path);
        Self {
            current_user: user,
            csrf_token,
            current_path,
            active_navigation,
            current_area,
        }
    }
}

fn navigation_area(path: &str) -> (&'static str, &'static str) {
    if path.starts_with("/customers") {
        ("customers", "Customers")
    } else if path.starts_with("/vehicles") {
        ("vehicles", "Vehicles")
    } else if path.starts_with("/interventions") {
        ("interventions", "Interventions")
    } else if path.starts_with("/knowledge") {
        ("knowledge", "Knowledge")
    } else if path.starts_with("/invoices") {
        ("invoices", "Invoices")
    } else {
        ("dashboard", "Dashboard")
    }
}

#[cfg(test)]
mod tests {
    use super::navigation_area;

    #[test]
    fn record_paths_keep_their_owning_navigation_area_active() {
        assert_eq!(
            navigation_area("/vehicles/vehicle%3A1/edit"),
            ("vehicles", "Vehicles")
        );
        assert_eq!(
            navigation_area("/interventions/intervention%3A1"),
            ("interventions", "Interventions")
        );
        assert_eq!(navigation_area("/"), ("dashboard", "Dashboard"));
    }
}
