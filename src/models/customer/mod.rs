//! Customer data, validation, workflows, queries, associations, and persistence.

mod domain;
mod operations;
#[doc(hidden)]
pub mod persistence;
#[doc(hidden)]
pub mod repository;

pub use domain::*;
pub use operations::{CreateCustomer, CustomerAddressInput, UpdateCustomer};
pub use repository::{ArchiveFilter, CustomerFilter};

use crate::{
    domain::{CustomerId, Page, PageRequest},
    models::{
        vehicle::{Vehicle, VehicleFilter, VehicleModel},
        ModelContext, ModelError,
    },
};

use self::operations::CustomerOperations;

impl Customer {
    /// Create a customer from delivery-independent input.
    pub async fn create(context: &ModelContext, input: CreateCustomer) -> Result<Self, ModelError> {
        CustomerOperations::new(context)?.create(input).await
    }

    /// Find a customer, returning absence without turning it into an error.
    pub async fn find(context: &ModelContext, id: &CustomerId) -> Result<Option<Self>, ModelError> {
        CustomerOperations::new(context)?.find(id).await
    }

    /// Find a required customer.
    pub async fn require(context: &ModelContext, id: &CustomerId) -> Result<Self, ModelError> {
        CustomerOperations::new(context)?.get(id).await
    }

    /// Search customers with stable cursor pagination.
    pub async fn search(
        context: &ModelContext,
        query: PageRequest<CustomerFilter>,
    ) -> Result<Page<Self>, ModelError> {
        CustomerOperations::new(context)?.list(query).await
    }

    /// Apply an explicit partial update.
    pub async fn update(
        &self,
        context: &ModelContext,
        changes: UpdateCustomer,
    ) -> Result<Self, ModelError> {
        CustomerOperations::new(context)?
            .update(&self.id, changes)
            .await
    }

    /// Archive this customer.
    pub async fn archive(&self, context: &ModelContext) -> Result<Self, ModelError> {
        CustomerOperations::new(context)?.archive(&self.id).await
    }

    /// Restore this customer.
    pub async fn restore(&self, context: &ModelContext) -> Result<Self, ModelError> {
        CustomerOperations::new(context)?.restore(&self.id).await
    }

    /// Load this customer's vehicles with the existing deterministic cursor contract.
    pub async fn vehicles(
        &self,
        context: &ModelContext,
        query: PageRequest<VehicleFilter>,
    ) -> Result<Page<Vehicle>, ModelError> {
        VehicleModel::from_context(context)?
            .list_by_customer(&self.id, query)
            .await
    }
}

/// Model-owned customer API used by delivery adapters during the structural migration.
#[derive(Clone)]
pub struct CustomerModel {
    context: ModelContext,
}

impl CustomerModel {
    #[must_use]
    pub const fn new(context: ModelContext) -> Self {
        Self { context }
    }

    pub async fn create(&self, input: CreateCustomer) -> Result<Customer, ModelError> {
        Customer::create(&self.context, input).await
    }

    pub async fn get(&self, id: &CustomerId) -> Result<Customer, ModelError> {
        Customer::require(&self.context, id).await
    }

    pub async fn update(
        &self,
        id: &CustomerId,
        changes: UpdateCustomer,
    ) -> Result<Customer, ModelError> {
        self.get(id).await?.update(&self.context, changes).await
    }

    pub async fn list(
        &self,
        query: PageRequest<CustomerFilter>,
    ) -> Result<Page<Customer>, ModelError> {
        Customer::search(&self.context, query).await
    }

    pub async fn archive(&self, id: &CustomerId) -> Result<Customer, ModelError> {
        self.get(id).await?.archive(&self.context).await
    }

    pub async fn restore(&self, id: &CustomerId) -> Result<Customer, ModelError> {
        self.get(id).await?.restore(&self.context).await
    }
}
