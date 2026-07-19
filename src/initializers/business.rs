//! Customer and vehicle service composition.

use std::sync::Arc;

use loco_rs::{app::AppContext, environment::Environment, Error, Result};

use crate::{
    database::client::AppDatabase,
    domain::CursorCodec,
    repositories::{
        customer::CustomerRepository,
        surreal::{customer::SurrealCustomerRepository, vehicle::SurrealVehicleRepository},
        vehicle::VehicleRepository,
    },
    services::{customer::CustomerService, vehicle::VehicleService},
};

pub async fn install(ctx: &AppContext) -> Result<()> {
    let database = ctx
        .shared_store
        .get::<AppDatabase>()
        .ok_or_else(|| Error::string("application database is not installed"))?;
    let client = database.client().map_err(Error::msg)?;
    if ctx.environment == Environment::Test {
        let schema = [
            include_str!("../../database/schema/business/customer.surql"),
            include_str!("../../database/schema/business/vehicle.surql"),
        ]
        .join("\n");
        client
            .query(schema)
            .await
            .map_err(|_| Error::string("test business schema application failed"))?
            .check()
            .map_err(|_| Error::string("test business schema application failed"))?;
    }
    let cursors = ctx
        .shared_store
        .get::<CursorCodec>()
        .ok_or_else(|| Error::string("cursor service is not installed"))?;
    let customers: Arc<dyn CustomerRepository> =
        Arc::new(SurrealCustomerRepository::new(client.clone()));
    let vehicles: Arc<dyn VehicleRepository> = Arc::new(SurrealVehicleRepository::new(client));
    ctx.shared_store
        .insert(CustomerService::new(customers.clone(), cursors.clone()));
    ctx.shared_store
        .insert(VehicleService::new(vehicles, customers, cursors));
    Ok(())
}
