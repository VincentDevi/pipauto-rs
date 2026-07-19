//! Typed, presentation-safe data shared by authenticated application pages.

use serde::Serialize;

use crate::models::auth::AuthenticatedUser;

/// User fields approved for display in the application shell.
#[derive(Debug, Serialize)]
pub struct PresentationUser<'user> {
    display_name: &'user str,
}

/// Authenticated shell data with no record, session, email, or credential identifiers.
#[derive(Debug, Serialize)]
pub struct AuthenticatedLayout<'layout> {
    current_user: PresentationUser<'layout>,
    csrf_token: &'layout str,
    current_path: &'layout str,
}

impl<'layout> AuthenticatedLayout<'layout> {
    /// Project an authenticated request into fields safe for shell rendering.
    #[must_use]
    pub fn new(
        user: &'layout AuthenticatedUser,
        csrf_token: &'layout str,
        current_path: &'layout str,
    ) -> Self {
        Self {
            current_user: PresentationUser {
                display_name: &user.display_name,
            },
            csrf_token,
            current_path,
        }
    }
}
