//! Vehicle application workflows and current-owner policy.

use std::sync::Arc;

use chrono::{Datelike, Utc};

use crate::{
    domain::{
        normalize_search_text, CursorCodec, CursorResource, CustomerId, NormalizedRegistration,
        NormalizedVin, Page, PageRequest, ValidationCode, ValidationError, ValidationErrors,
        VehicleId,
    },
    models::vehicle::{NewVehicle, Vehicle, VehicleModelError},
    repositories::{
        customer::CustomerRepository,
        vehicle::{VehicleFilter, VehicleRepository},
    },
};

use super::WorkflowError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateVehicle {
    pub customer_id: CustomerId,
    pub make: String,
    pub model: String,
    pub year: Option<i32>,
    pub registration: Option<String>,
    pub vin: Option<String>,
    pub current_mileage: Option<u64>,
    pub engine_type: Option<String>,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UpdateVehicle {
    pub customer_id: Option<CustomerId>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub year: Option<Option<i32>>,
    pub registration: Option<Option<String>>,
    pub vin: Option<Option<String>>,
    pub current_mileage: Option<Option<u64>>,
    pub engine_type: Option<Option<String>>,
    pub notes: Option<Option<String>>,
}

#[derive(Clone)]
pub struct VehicleService {
    vehicles: Arc<dyn VehicleRepository>,
    customers: Arc<dyn CustomerRepository>,
    cursors: CursorCodec,
    resource: CursorResource,
}

impl VehicleService {
    pub fn new(
        vehicles: Arc<dyn VehicleRepository>,
        customers: Arc<dyn CustomerRepository>,
        cursors: CursorCodec,
    ) -> Self {
        Self {
            vehicles,
            customers,
            cursors,
            resource: CursorResource::parse("vehicles").expect("static resource is valid"),
        }
    }

    pub async fn create(&self, command: CreateVehicle) -> Result<Vehicle, WorkflowError> {
        self.require_active_customer(&command.customer_id).await?;
        let vehicle = validate_vehicle(command)?;
        self.vehicles.create(&vehicle).await.map_err(Into::into)
    }

    pub async fn get(&self, id: &VehicleId) -> Result<Vehicle, WorkflowError> {
        self.vehicles
            .find_by_id(id)
            .await?
            .ok_or(WorkflowError::NotFound)
    }

    pub async fn find_by_vin(&self, value: &str) -> Result<Vehicle, WorkflowError> {
        let vin =
            NormalizedVin::parse(value).map_err(|_| validation("vin", "Enter a valid VIN."))?;
        self.vehicles
            .find_by_vin(&vin)
            .await?
            .ok_or(WorkflowError::NotFound)
    }

    pub async fn find_by_registration(&self, value: &str) -> Result<Vehicle, WorkflowError> {
        let registration = NormalizedRegistration::parse(value)
            .map_err(|_| validation("registration", "Enter a valid registration."))?;
        self.vehicles
            .find_by_registration(&registration)
            .await?
            .ok_or(WorkflowError::NotFound)
    }

    pub async fn update(
        &self,
        id: &VehicleId,
        command: UpdateVehicle,
    ) -> Result<Vehicle, WorkflowError> {
        let current = self.get(id).await?;
        let owner = command
            .customer_id
            .unwrap_or_else(|| current.customer_id.clone());
        if owner != current.customer_id {
            self.require_active_customer(&owner).await?;
        }
        let vehicle = validate_vehicle(CreateVehicle {
            customer_id: owner,
            make: command.make.unwrap_or(current.make),
            model: command.model.unwrap_or(current.model),
            year: command.year.unwrap_or(current.year),
            registration: command.registration.unwrap_or(current.registration),
            vin: command.vin.unwrap_or(current.vin),
            current_mileage: command.current_mileage.unwrap_or(current.current_mileage),
            engine_type: command.engine_type.unwrap_or(current.engine_type),
            notes: command.notes.unwrap_or(current.notes),
        })?;
        self.vehicles.update(id, &vehicle).await.map_err(Into::into)
    }

    pub async fn reassign(
        &self,
        id: &VehicleId,
        customer_id: &CustomerId,
    ) -> Result<Vehicle, WorkflowError> {
        self.require_active_customer(customer_id).await?;
        self.vehicles
            .reassign(id, customer_id)
            .await
            .map_err(Into::into)
    }

    pub async fn list(
        &self,
        request: PageRequest<VehicleFilter>,
    ) -> Result<Page<Vehicle>, WorkflowError> {
        let mut filter = request.filter;
        filter.query = normalize_optional(filter.query);
        filter.make = normalize_optional(filter.make);
        filter.model = normalize_optional(filter.model);
        let after = request
            .after
            .as_ref()
            .map(|cursor| self.cursors.decode(cursor, &self.resource, &filter))
            .transpose()
            .map_err(|_| validation("cursor", "Use the cursor returned by this search."))?;
        let page = self
            .vehicles
            .list(&filter, request.limit, after.as_ref())
            .await?;
        let next_cursor = page
            .next
            .as_ref()
            .map(|tuple| self.cursors.encode(&self.resource, tuple, &filter))
            .transpose()
            .map_err(|_| WorkflowError::Internal)?;
        Ok(Page {
            items: page.items,
            next_cursor,
        })
    }

    pub async fn list_by_customer(
        &self,
        customer_id: &CustomerId,
        mut request: PageRequest<VehicleFilter>,
    ) -> Result<Page<Vehicle>, WorkflowError> {
        self.customers
            .find_by_id(customer_id)
            .await?
            .ok_or(WorkflowError::NotFound)?;
        request.filter.customer_id = Some(customer_id.clone());
        self.list(request).await
    }

    pub async fn archive(&self, id: &VehicleId) -> Result<Vehicle, WorkflowError> {
        self.vehicles.archive(id).await.map_err(Into::into)
    }

    pub async fn restore(&self, id: &VehicleId) -> Result<Vehicle, WorkflowError> {
        self.vehicles.restore(id).await.map_err(Into::into)
    }

    async fn require_active_customer(&self, id: &CustomerId) -> Result<(), WorkflowError> {
        let customer = self
            .customers
            .find_by_id(id)
            .await?
            .ok_or(WorkflowError::Conflict)?;
        if customer.is_archived() {
            Err(WorkflowError::Conflict)
        } else {
            Ok(())
        }
    }
}

fn validate_vehicle(command: CreateVehicle) -> Result<NewVehicle, WorkflowError> {
    if command
        .current_mileage
        .is_some_and(|value| value > i64::MAX as u64)
    {
        return Err(validation("current_mileage", "Enter a supported mileage."));
    }
    NewVehicle::new(
        command.customer_id,
        command.make,
        command.model,
        command.year,
        command.registration,
        command.vin,
        command.current_mileage,
        command.engine_type,
        command.notes,
        Utc::now().year(),
    )
    .map_err(vehicle_validation)
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|value| normalize_search_text(&value))
        .filter(|value| !value.is_empty())
}

fn vehicle_validation(error: VehicleModelError) -> WorkflowError {
    let (field, message) = match error {
        VehicleModelError::Required => ("vehicle", "Enter the required vehicle details."),
        VehicleModelError::TooLong => ("vehicle", "Shorten the submitted value."),
        VehicleModelError::InvalidYear => ("year", "Enter a plausible vehicle year."),
        VehicleModelError::InvalidRegistration => ("registration", "Enter a valid registration."),
        VehicleModelError::InvalidVin => ("vin", "Enter a valid VIN."),
    };
    validation(field, message)
}

fn validation(field: &str, message: &str) -> WorkflowError {
    WorkflowError::Validation(ValidationErrors::one(
        ValidationError::new(field, ValidationCode::InvalidFormat, message)
            .expect("static validation metadata is valid"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vehicle_service_normalizes_structured_text_filters() {
        assert_eq!(
            normalize_optional(Some("  Golf  GTE ".into())).as_deref(),
            Some("golf gte")
        );
    }
}
