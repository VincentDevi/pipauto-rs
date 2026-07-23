//! Vehicle data, validation, associations, operations, and private persistence.

mod domain;
mod operations;
pub(crate) mod persistence;
pub(crate) mod repository;

pub(crate) use crate::models::ModelError as WorkflowError;
pub use domain::*;
pub use operations::{CreateVehicle, UpdateVehicle, VehicleModel};
pub use repository::VehicleFilter;

use crate::{
    domain::{Page, PageRequest},
    models::{
        customer::Customer,
        intervention::{InterventionFilter, InterventionModel, ServiceHistorySummary},
        ModelContext, ModelError,
    },
};

impl Vehicle {
    /// Load the current customer for this vehicle.
    pub async fn customer(&self, context: &ModelContext) -> Result<Customer, ModelError> {
        Customer::require(context, &self.customer_id).await
    }

    /// Load this vehicle's deterministic service history.
    pub async fn interventions(
        &self,
        context: &ModelContext,
        query: PageRequest<InterventionFilter>,
    ) -> Result<Page<ServiceHistorySummary>, ModelError> {
        InterventionModel::from_context(context)?
            .service_history(&self.id, query)
            .await
    }
}
