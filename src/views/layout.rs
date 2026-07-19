//! Typed, presentation-safe data shared by authenticated application pages.

use serde::Serialize;

use crate::views::context::PresentationUser;

/// Authenticated shell data with no record, session, email, or credential identifiers.
#[derive(Debug, Serialize)]
pub struct AuthenticatedLayout<'layout> {
    current_user: &'layout PresentationUser,
    csrf_token: &'layout str,
    current_path: &'layout str,
}

impl<'layout> AuthenticatedLayout<'layout> {
    /// Project an authenticated request into fields safe for shell rendering.
    #[must_use]
    pub fn new(
        user: &'layout PresentationUser,
        csrf_token: &'layout str,
        current_path: &'layout str,
    ) -> Self {
        Self {
            current_user: user,
            csrf_token,
            current_path,
        }
    }
}
