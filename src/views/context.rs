//! Presentation-only values shared by browser request context and templates.

use serde::Serialize;

/// User fields approved for templates and browser presentation models.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PresentationUser {
    pub display_name: String,
}
