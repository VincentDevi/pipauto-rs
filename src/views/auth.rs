//! Typed presentation data for login pages and HTMX form fragments.

use loco_rs::{
    controller::views::{engines::TeraView, ViewRenderer},
    Result,
};
use serde::Serialize;

const PAGE_TEMPLATE: &str = "pages/login.html";
const FORM_TEMPLATE: &str = "fragments/login_form.html";
const UNAVAILABLE_PAGE_TEMPLATE: &str = "pages/auth_unavailable.html";
const UNAVAILABLE_FRAGMENT_TEMPLATE: &str = "fragments/auth_unavailable.html";

/// Login form presentation state. It never contains a password.
#[derive(Debug, Serialize)]
pub struct LoginView<'view> {
    title: &'static str,
    email: &'view str,
    next: &'view str,
    csrf_token: &'view str,
    error_summary: Option<&'view str>,
    email_error: Option<&'view str>,
    password_error: Option<&'view str>,
}

/// Safe authentication outage state with an operator-searchable correlation identifier.
#[derive(Debug, Serialize)]
pub struct AuthenticationUnavailableView<'view> {
    title: &'static str,
    heading: &'static str,
    message: &'view str,
    correlation_id: &'view str,
}

impl<'view> AuthenticationUnavailableView<'view> {
    /// Construct an authentication outage response without carrying an internal error.
    #[must_use]
    pub const fn new(message: &'view str, correlation_id: &'view str) -> Self {
        Self {
            title: "Authentication unavailable | Pipauto",
            heading: "Authentication is temporarily unavailable",
            message,
            correlation_id,
        }
    }

    /// Render a complete outage page.
    ///
    /// # Errors
    ///
    /// Returns a rendering error when the committed template is invalid.
    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(UNAVAILABLE_PAGE_TEMPLATE, self)
    }

    /// Render only the progressively enhanced outage region.
    ///
    /// # Errors
    ///
    /// Returns a rendering error when the committed template is invalid.
    pub fn render_fragment(&self, engine: &TeraView) -> Result<String> {
        engine.render(UNAVAILABLE_FRAGMENT_TEMPLATE, self)
    }
}

impl<'view> LoginView<'view> {
    /// Construct a login form state.
    #[must_use]
    pub const fn new(
        email: &'view str,
        next: &'view str,
        csrf_token: &'view str,
        error_summary: Option<&'view str>,
        email_error: Option<&'view str>,
        password_error: Option<&'view str>,
    ) -> Self {
        Self {
            title: "Sign in to Pipauto",
            email,
            next,
            csrf_token,
            error_summary,
            email_error,
            password_error,
        }
    }

    /// Render a complete login page.
    ///
    /// # Errors
    ///
    /// Returns a rendering error when the committed template is invalid.
    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(PAGE_TEMPLATE, self)
    }

    /// Render only the progressively enhanced form region.
    ///
    /// # Errors
    ///
    /// Returns a rendering error when the committed template is invalid.
    pub fn render_form(&self, engine: &TeraView) -> Result<String> {
        engine.render(FORM_TEMPLATE, self)
    }
}
