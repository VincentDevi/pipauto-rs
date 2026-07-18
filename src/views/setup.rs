//! Presentation model and rendering for the application setup page.

use loco_rs::{
    controller::views::{engines::TeraView, ViewRenderer},
    Result,
};
use serde::Serialize;

const TEMPLATE: &str = "pages/setup.html";
const STATUS_TEMPLATE: &str = "fragments/setup_status.html";

/// Typed data supplied to the setup page template.
#[derive(Debug, Serialize)]
pub struct SetupPage<'page> {
    title: &'page str,
    heading: &'page str,
    summary: &'page str,
    detail: &'page str,
}

/// Database-status presentation data supplied to the setup-status fragment.
#[derive(Debug, Serialize)]
pub struct SetupStatus<'status> {
    state: &'status str,
    label: &'status str,
    detail: &'status str,
}

impl SetupStatus<'static> {
    /// Creates presentation data for an available application database.
    #[must_use]
    pub const fn connected() -> Self {
        Self {
            state: "connected",
            label: "Connected",
            detail: "The application database responded to the setup check.",
        }
    }

    /// Creates presentation data for an unavailable application database.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            state: "unavailable",
            label: "Unavailable",
            detail: "The application database did not respond. Check the database service and try again.",
        }
    }
}

impl SetupStatus<'_> {
    /// Renders the setup-status fragment through the application Tera engine.
    ///
    /// # Errors
    ///
    /// Returns an error when the template cannot be rendered.
    pub fn render(&self, engine: &TeraView) -> Result<String> {
        engine.render(STATUS_TEMPLATE, self)
    }
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
