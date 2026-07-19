//! Persistence-neutral vehicle repository contract.

use async_trait::async_trait;

use crate::{
    domain::{
        CollectionFilter, CursorTuple, CustomerId, NormalizedRegistration, NormalizedVin,
        PageLimit, VehicleId,
    },
    models::vehicle::{NewVehicle, Vehicle},
};

use super::{customer::ArchiveFilter, customer::RepositoryPage, RepositoryError};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VehicleFilter {
    pub query: Option<String>,
    pub archive: ArchiveFilter,
    pub customer_id: Option<CustomerId>,
    pub registration: Option<NormalizedRegistration>,
    pub vin: Option<NormalizedVin>,
    pub make: Option<String>,
    pub model: Option<String>,
}

impl CollectionFilter for VehicleFilter {
    fn fingerprint_bytes(&self) -> Vec<u8> {
        format!(
            "vehicles:v1:{:?}:{}:{}:{}:{}:{}:{}",
            self.archive,
            self.query.as_deref().unwrap_or(""),
            self.customer_id.as_ref().map_or("", CustomerId::as_str),
            self.registration
                .as_ref()
                .map_or("", NormalizedRegistration::as_str),
            self.vin.as_ref().map_or("", NormalizedVin::as_str),
            self.make.as_deref().unwrap_or(""),
            self.model.as_deref().unwrap_or("")
        )
        .into_bytes()
    }
}

#[async_trait]
pub trait VehicleRepository: Send + Sync {
    async fn create(&self, vehicle: &NewVehicle) -> Result<Vehicle, RepositoryError>;
    async fn find_by_id(&self, id: &VehicleId) -> Result<Option<Vehicle>, RepositoryError>;
    async fn find_by_vin(&self, vin: &NormalizedVin) -> Result<Option<Vehicle>, RepositoryError>;
    async fn find_by_registration(
        &self,
        registration: &NormalizedRegistration,
    ) -> Result<Option<Vehicle>, RepositoryError>;
    async fn update(
        &self,
        id: &VehicleId,
        vehicle: &NewVehicle,
    ) -> Result<Vehicle, RepositoryError>;
    async fn list(
        &self,
        filter: &VehicleFilter,
        limit: PageLimit,
        after: Option<&CursorTuple>,
    ) -> Result<RepositoryPage<Vehicle>, RepositoryError>;
    async fn list_by_customer(
        &self,
        customer_id: &CustomerId,
        filter: &VehicleFilter,
        limit: PageLimit,
        after: Option<&CursorTuple>,
    ) -> Result<RepositoryPage<Vehicle>, RepositoryError>;
    async fn reassign(
        &self,
        id: &VehicleId,
        customer_id: &CustomerId,
    ) -> Result<Vehicle, RepositoryError>;
    async fn archive(&self, id: &VehicleId) -> Result<Vehicle, RepositoryError>;
    async fn restore(&self, id: &VehicleId) -> Result<Vehicle, RepositoryError>;
}
