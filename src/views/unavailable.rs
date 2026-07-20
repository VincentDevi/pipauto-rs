//! Presentation model for authenticated, not-yet-implemented workshop pages.

use loco_rs::{
    controller::views::{engines::TeraView, ViewRenderer},
    Result,
};
use serde::Serialize;

use super::layout::AuthenticatedLayout;

const PAGE_TEMPLATE: &str = "pages/unavailable.html";
const PANEL_TEMPLATE: &str = "fragments/unavailable_panel.html";

#[derive(Debug, Serialize)]
pub struct UnavailablePage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
}

impl<'page> UnavailablePage<'page> {
    #[must_use]
    pub const fn new(layout: AuthenticatedLayout<'page>) -> Self {
        Self {
            layout,
            title: "Page unavailable | Pipauto",
        }
    }

    /// Renders the complete authenticated placeholder page.
    ///
    /// # Errors
    ///
    /// Returns an error when the committed template is invalid.
    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(PAGE_TEMPLATE, self)
    }

    /// Renders the placeholder panel for an HTMX request.
    ///
    /// # Errors
    ///
    /// Returns an error when the committed template is invalid.
    pub fn render_panel(&self, engine: &TeraView) -> Result<String> {
        engine.render(PANEL_TEMPLATE, self)
    }
}
