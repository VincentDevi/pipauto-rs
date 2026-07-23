//! Intervention data, lifecycle, lines, calendar queries, and private persistence.

pub(crate) mod calendar_repository;
mod domain;
pub mod line;
mod operations;
pub(crate) mod persistence;
pub(crate) mod repository;

pub(crate) use crate::models::ModelError as WorkflowError;
pub use domain::*;
pub use operations::{CreateIntervention, InterventionModel, UpdateIntervention, WriteLine};
pub use repository::{InterventionFilter, LineMoveDirection, LineMutationResult};

use crate::models::{ModelContext, ModelError};

impl Intervention {
    /// Load this intervention's explicitly ordered lines.
    pub async fn lines(
        &self,
        context: &ModelContext,
    ) -> Result<Vec<line::InterventionLine>, ModelError> {
        InterventionModel::from_context(context)?
            .list_lines(&self.id)
            .await
    }
}
