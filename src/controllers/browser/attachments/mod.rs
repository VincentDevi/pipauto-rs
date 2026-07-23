//! Global browser attachment route composition.

use loco_rs::controller::Routes;
use loco_rs::prelude::get;

mod content;
pub(super) mod forms;

/// Compose global attachment routes.
#[must_use]
pub fn routes() -> Routes {
    Routes::new()
        .add("/attachments/{id}/content", get(content::content))
        .add("/attachments/{id}/download", get(content::download))
}
