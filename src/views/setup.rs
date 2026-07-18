//! Presentation model and rendering for the application setup page.

use loco_rs::{
    controller::views::{engines::TeraView, ViewRenderer},
    Result,
};
use serde::Serialize;

const TEMPLATE: &str = "pages/setup.html";

/// Typed data supplied to the setup page template.
#[derive(Debug, Serialize)]
pub struct SetupPage<'page> {
    title: &'page str,
    heading: &'page str,
    summary: &'page str,
    detail: &'page str,
}

impl<'page> SetupPage<'page> {
    /// Creates setup-page presentation data without application workflow concerns.
    #[must_use]
    pub const fn new(
        title: &'page str,
        heading: &'page str,
        summary: &'page str,
        detail: &'page str,
    ) -> Self {
        Self {
            title,
            heading,
            summary,
            detail,
        }
    }

    /// Renders the setup page through the application Tera engine.
    ///
    /// # Errors
    ///
    /// Returns an error when the template cannot be rendered.
    pub fn render(&self, engine: &TeraView) -> Result<String> {
        engine.render(TEMPLATE, self)
    }
}
