//! Customer and vehicle service composition.

use std::sync::Arc;

use loco_rs::{app::AppContext, environment::Environment, Error, Result};

use crate::{
    database::client::AppDatabase,
    domain::CursorCodec,
    repositories::{
        attachment::AttachmentRepository,
        customer::CustomerRepository,
        health::HealthRepository,
        intervention::InterventionRepository,
        invoice::InvoiceRepository,
        surreal::{
            attachment::SurrealAttachmentRepository, customer::SurrealCustomerRepository,
            health::SurrealHealthRepository, intervention::SurrealInterventionRepository,
            invoice::SurrealInvoiceRepository, technical_note::SurrealTechnicalNoteRepository,
            vehicle::SurrealVehicleRepository,
        },
        technical_note::TechnicalNoteRepository,
        vehicle::VehicleRepository,
    },
    services::{
        attachment::AttachmentService, customer::CustomerService, health::HealthService,
        intervention::InterventionService, invoice::InvoiceService,
        technical_note::TechnicalNoteService, vehicle::VehicleService,
    },
};

pub async fn install(ctx: &AppContext) -> Result<()> {
    let database = ctx
        .shared_store
        .get::<AppDatabase>()
        .ok_or_else(|| Error::string("application database is not installed"))?;
    let client = database.client().map_err(Error::msg)?;
    let health: Arc<dyn HealthRepository> =
        Arc::new(SurrealHealthRepository::new(database.clone()));
    if ctx.environment == Environment::Test {
        let schema = [
            include_str!("../../database/schema/business/customer.surql"),
            include_str!("../../database/schema/business/vehicle.surql"),
            include_str!("../../database/schema/business/intervention.surql"),
            include_str!("../../database/schema/business/intervention_line.surql"),
            include_str!("../../database/schema/business/technical_note.surql"),
            include_str!("../../database/schema/business/attachment.surql"),
            include_str!("../../database/schema/business/invoice.surql"),
            include_str!("../../database/schema/business/invoice_line.surql"),
            include_str!("../../database/schema/business/payment.surql"),
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
    let vehicles: Arc<dyn VehicleRepository> =
        Arc::new(SurrealVehicleRepository::new(client.clone()));
    let interventions: Arc<dyn InterventionRepository> =
        Arc::new(SurrealInterventionRepository::new(client.clone()));
    let notes: Arc<dyn TechnicalNoteRepository> =
        Arc::new(SurrealTechnicalNoteRepository::new(client.clone()));
    let invoices: Arc<dyn InvoiceRepository> =
        Arc::new(SurrealInvoiceRepository::new(client.clone()));
    let attachments: Arc<dyn AttachmentRepository> =
        Arc::new(SurrealAttachmentRepository::new(client));
    ctx.shared_store
        .insert(CustomerService::new(customers.clone(), cursors.clone()));
    ctx.shared_store.insert(HealthService::new(health));
    ctx.shared_store.insert(VehicleService::new(
        vehicles.clone(),
        customers.clone(),
        cursors.clone(),
    ));
    ctx.shared_store.insert(InterventionService::new(
        interventions.clone(),
        vehicles.clone(),
        cursors.clone(),
    ));
    ctx.shared_store.insert(TechnicalNoteService::new(
        notes,
        vehicles.clone(),
        interventions.clone(),
        cursors.clone(),
    ));
    ctx.shared_store.insert(InvoiceService::new(
        invoices,
        customers.clone(),
        vehicles.clone(),
        interventions.clone(),
        cursors.clone(),
    ));
    ctx.shared_store
        .insert(AttachmentService::new(attachments, vehicles, interventions));
    Ok(())
}
