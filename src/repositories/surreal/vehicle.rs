//! SurrealDB vehicle repository adapter.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use surrealdb::{
    engine::any::Any,
    types::{RecordId, SurrealValue},
    Surreal,
};

use crate::{
    domain::{
        CursorTuple, CustomerId, NormalizedRegistration, NormalizedVin, PageLimit, VehicleId,
    },
    models::vehicle::{NewVehicle, Vehicle},
    repositories::{
        customer::{ArchiveFilter, RepositoryPage},
        vehicle::{VehicleFilter, VehicleRepository},
        RepositoryError,
    },
};

use super::support;

const PROJECTION: &str = "id, customer, make, model, year, registration, vin, current_mileage, engine_type, notes, created_at, updated_at, archived_at";

#[derive(Clone)]
pub struct SurrealVehicleRepository {
    client: Surreal<Any>,
}

impl SurrealVehicleRepository {
    #[must_use]
    pub fn new(client: Surreal<Any>) -> Self {
        Self { client }
    }

    async fn customer_is_active(&self, customer_id: &CustomerId) -> Result<bool, RepositoryError> {
        let mut response = support::checked_response(
            self.client
                .query("SELECT VALUE id FROM ONLY $customer WHERE archived_at IS NONE;")
                .bind((
                    "customer",
                    support::record_id("customer", customer_id.as_str())?,
                ))
                .await,
        )?;
        let found: Option<RecordId> = support::take(&mut response, 0)?;
        Ok(found.is_some())
    }

    async fn lookup(
        &self,
        field: &'static str,
        value: String,
    ) -> Result<Option<Vehicle>, RepositoryError> {
        let query = format!("SELECT {PROJECTION} FROM vehicle WHERE {field} = $value LIMIT 1;");
        let mut response =
            support::checked_response(self.client.query(query).bind(("value", value)).await)?;
        let row: Option<DbVehicle> = support::take(&mut response, 0)?;
        row.map(TryInto::try_into).transpose()
    }

    async fn set_archive(
        &self,
        id: &VehicleId,
        archived: bool,
    ) -> Result<Vehicle, RepositoryError> {
        let predicate = if archived {
            "archived_at IS NONE"
        } else {
            "archived_at IS NOT NONE"
        };
        let value = if archived { "time::now()" } else { "NONE" };
        let query = format!(
            "UPDATE ONLY $record SET archived_at = {value} WHERE {predicate} RETURN AFTER;"
        );
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind(("record", support::record_id("vehicle", id.as_str())?))
                .await,
        )?;
        let row: Option<DbVehicle> = support::take(&mut response, 0)?;
        match row {
            Some(row) => row.try_into(),
            None => self.find_by_id(id).await?.ok_or(RepositoryError::NotFound),
        }
    }
}

#[derive(Deserialize, SurrealValue)]
struct DbVehicle {
    id: RecordId,
    customer: RecordId,
    make: String,
    model: String,
    year: Option<i32>,
    registration: Option<String>,
    vin: Option<String>,
    current_mileage: Option<i64>,
    engine_type: Option<String>,
    notes: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    archived_at: Option<DateTime<Utc>>,
}

impl TryFrom<DbVehicle> for Vehicle {
    type Error = RepositoryError;

    fn try_from(value: DbVehicle) -> Result<Self, Self::Error> {
        Ok(Self {
            id: VehicleId::parse(support::record_key(&value.id, "vehicle")?)
                .map_err(|_| RepositoryError::CorruptData)?,
            customer_id: CustomerId::parse(support::record_key(&value.customer, "customer")?)
                .map_err(|_| RepositoryError::CorruptData)?,
            make: value.make,
            model: value.model,
            year: value.year,
            registration: value.registration,
            vin: value.vin,
            current_mileage: value
                .current_mileage
                .map(u64::try_from)
                .transpose()
                .map_err(|_| RepositoryError::CorruptData)?,
            engine_type: value.engine_type,
            notes: value.notes,
            created_at: value.created_at,
            updated_at: value.updated_at,
            archived_at: value.archived_at,
        })
    }
}

fn archive_value(filter: ArchiveFilter) -> &'static str {
    match filter {
        ArchiveFilter::Active => "active",
        ArchiveFilter::Archived => "archived",
        ArchiveFilter::All => "all",
    }
}

#[allow(clippy::too_many_arguments)]
async fn bind_write(
    client: &Surreal<Any>,
    query: &str,
    record: Option<RecordId>,
    vehicle: &NewVehicle,
) -> Result<surrealdb::IndexedResults, RepositoryError> {
    let mileage = vehicle
        .current_mileage
        .map(i64::try_from)
        .transpose()
        .map_err(|_| RepositoryError::CorruptData)?;
    let mut builder = client
        .query(query)
        .bind((
            "customer",
            support::record_id("customer", vehicle.customer_id.as_str())?,
        ))
        .bind(("make", vehicle.make.clone()))
        .bind(("make_normalized", vehicle.make_normalized.clone()))
        .bind(("model", vehicle.model.clone()))
        .bind(("model_normalized", vehicle.model_normalized.clone()))
        .bind(("year", vehicle.year))
        .bind(("registration", vehicle.registration.clone()))
        .bind((
            "registration_normalized",
            vehicle
                .registration_normalized
                .as_ref()
                .map(|v| v.as_str().to_owned()),
        ))
        .bind(("vin", vehicle.vin.clone()))
        .bind((
            "vin_normalized",
            vehicle
                .vin_normalized
                .as_ref()
                .map(|v| v.as_str().to_owned()),
        ))
        .bind(("current_mileage", mileage))
        .bind(("engine_type", vehicle.engine_type.clone()))
        .bind(("notes", vehicle.notes.clone()));
    if let Some(record) = record {
        builder = builder.bind(("record", record));
    }
    support::checked_response(builder.await)
}

#[async_trait]
impl VehicleRepository for SurrealVehicleRepository {
    async fn create(&self, vehicle: &NewVehicle) -> Result<Vehicle, RepositoryError> {
        if !self.customer_is_active(&vehicle.customer_id).await? {
            return Err(RepositoryError::Conflict);
        }
        let mut response = bind_write(
            &self.client,
            "CREATE vehicle SET customer = $customer, make = $make, make_normalized = $make_normalized, model = $model, model_normalized = $model_normalized, year = $year, registration = $registration, registration_normalized = $registration_normalized, vin = $vin, vin_normalized = $vin_normalized, current_mileage = $current_mileage, engine_type = $engine_type, notes = $notes, archived_at = NONE RETURN AFTER;",
            None,
            vehicle,
        ).await?;
        let row: Option<DbVehicle> = support::take(&mut response, 0)?;
        row.ok_or(RepositoryError::CorruptData)?.try_into()
    }

    async fn find_by_id(&self, id: &VehicleId) -> Result<Option<Vehicle>, RepositoryError> {
        let query = format!("SELECT {PROJECTION} FROM ONLY $record;");
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind(("record", support::record_id("vehicle", id.as_str())?))
                .await,
        )?;
        let row: Option<DbVehicle> = support::take(&mut response, 0)?;
        row.map(TryInto::try_into).transpose()
    }

    async fn find_by_vin(&self, vin: &NormalizedVin) -> Result<Option<Vehicle>, RepositoryError> {
        self.lookup("vin_normalized", vin.as_str().to_owned()).await
    }

    async fn find_by_registration(
        &self,
        registration: &NormalizedRegistration,
    ) -> Result<Option<Vehicle>, RepositoryError> {
        self.lookup("registration_normalized", registration.as_str().to_owned())
            .await
    }

    async fn update(
        &self,
        id: &VehicleId,
        vehicle: &NewVehicle,
    ) -> Result<Vehicle, RepositoryError> {
        if !self.customer_is_active(&vehicle.customer_id).await? {
            return Err(RepositoryError::Conflict);
        }
        let mut response = bind_write(
            &self.client,
            "UPDATE ONLY $record SET customer = $customer, make = $make, make_normalized = $make_normalized, model = $model, model_normalized = $model_normalized, year = $year, registration = $registration, registration_normalized = $registration_normalized, vin = $vin, vin_normalized = $vin_normalized, current_mileage = $current_mileage, engine_type = $engine_type, notes = $notes RETURN AFTER;",
            Some(support::record_id("vehicle", id.as_str())?),
            vehicle,
        )
        .await?;
        let row: Option<DbVehicle> = support::take(&mut response, 0)?;
        row.ok_or(RepositoryError::NotFound)?.try_into()
    }

    async fn list(
        &self,
        filter: &VehicleFilter,
        limit: PageLimit,
        after: Option<&CursorTuple>,
    ) -> Result<RepositoryPage<Vehicle>, RepositoryError> {
        let (after_time, after_id) = after
            .map(|cursor| support::surreal_cursor_tuple(cursor, "vehicle"))
            .transpose()?
            .map_or((None, None), |(time, id)| (Some(time), Some(id)));
        let query = format!(
            "SELECT {PROJECTION} FROM vehicle WHERE ($archive = 'all' OR ($archive = 'active' AND archived_at IS NONE) OR ($archive = 'archived' AND archived_at IS NOT NONE)) AND ($customer IS NONE OR customer = $customer) AND ($registration IS NONE OR registration_normalized = $registration) AND ($vin IS NONE OR vin_normalized = $vin) AND ($make IS NONE OR make_normalized = $make) AND ($model IS NONE OR model_normalized = $model) AND ($query IS NONE OR string::contains(make_normalized, $query) OR string::contains(model_normalized, $query) OR string::contains(registration_normalized ?? '', $query) OR string::contains(vin_normalized ?? '', $query)) AND ($after_time IS NONE OR created_at < $after_time OR (created_at = $after_time AND id < $after_id)) ORDER BY created_at DESC, id DESC LIMIT $fetch_limit;"
        );
        let customer = filter
            .customer_id
            .as_ref()
            .map(|id| support::record_id("customer", id.as_str()))
            .transpose()?;
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind(("archive", archive_value(filter.archive).to_owned()))
                .bind(("customer", customer))
                .bind((
                    "registration",
                    filter.registration.as_ref().map(|v| v.as_str().to_owned()),
                ))
                .bind(("vin", filter.vin.as_ref().map(|v| v.as_str().to_owned())))
                .bind(("make", filter.make.clone()))
                .bind(("model", filter.model.clone()))
                .bind(("query", filter.query.clone()))
                .bind(("after_time", after_time))
                .bind(("after_id", after_id))
                .bind(("fetch_limit", i64::from(limit.value()) + 1))
                .await,
        )?;
        let mut rows: Vec<DbVehicle> = support::take(&mut response, 0)?;
        let has_more = rows.len() > usize::from(limit.value());
        if has_more {
            rows.pop();
        }
        let next = if has_more {
            rows.last()
                .map(|row| support::cursor_tuple(row.created_at, &row.id, "vehicle"))
                .transpose()?
        } else {
            None
        };
        let items = rows
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RepositoryPage { items, next })
    }

    async fn list_by_customer(
        &self,
        customer_id: &CustomerId,
        filter: &VehicleFilter,
        limit: PageLimit,
        after: Option<&CursorTuple>,
    ) -> Result<RepositoryPage<Vehicle>, RepositoryError> {
        let mut filter = filter.clone();
        filter.customer_id = Some(customer_id.clone());
        self.list(&filter, limit, after).await
    }

    async fn reassign(
        &self,
        id: &VehicleId,
        customer_id: &CustomerId,
    ) -> Result<Vehicle, RepositoryError> {
        let mut response = support::checked_response(
            self.client.query(
                "UPDATE ONLY $vehicle SET customer = $customer WHERE record::exists($customer) AND $customer.archived_at IS NONE RETURN AFTER;"
            ).bind(("vehicle", support::record_id("vehicle", id.as_str())?))
             .bind(("customer", support::record_id("customer", customer_id.as_str())?)).await,
        )?;
        let row: Option<DbVehicle> = support::take(&mut response, 0)?;
        if let Some(row) = row {
            return row.try_into();
        }
        if self.find_by_id(id).await?.is_none() {
            Err(RepositoryError::NotFound)
        } else {
            Err(RepositoryError::Conflict)
        }
    }

    async fn archive(&self, id: &VehicleId) -> Result<Vehicle, RepositoryError> {
        self.set_archive(id, true).await
    }
    async fn restore(&self, id: &VehicleId) -> Result<Vehicle, RepositoryError> {
        self.set_archive(id, false).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vehicle_repository_uses_explicit_projection_and_typed_archive_filter() {
        assert!(!PROJECTION.contains('*'));
        assert_eq!(archive_value(ArchiveFilter::Archived), "archived");
    }
}
