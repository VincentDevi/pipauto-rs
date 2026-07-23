//! Model context and business-model composition.

use loco_rs::{app::AppContext, environment::Environment, Error, Result};

use crate::{
    database::client::AppDatabase,
    domain::{CursorCodec, WorkshopTime},
    models::{
        attachment::{AttachmentModel, AttachmentReconciliation},
        calendar::CalendarModel,
        customer::CustomerModel,
        intervention::InterventionModel,
        invoice::InvoiceModel,
        technical_note::TechnicalNoteModel,
        vehicle::VehicleModel,
        ModelContext,
    },
};

pub async fn install(ctx: &AppContext) -> Result<()> {
    let database = ctx
        .shared_store
        .get::<AppDatabase>()
        .ok_or_else(|| Error::string("application database is not installed"))?;
    let client = database.client().map_err(Error::msg)?;
    if ctx.environment == Environment::Test {
        client
            .query("DEFINE BUCKET pipauto_attachments BACKEND 'memory' PERMISSIONS NONE;")
            .await
            .map_err(|_| Error::string("test attachment bucket definition failed"))?
            .check()
            .map_err(|_| Error::string("test attachment bucket definition failed"))?;
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
    let workshop_time = ctx
        .shared_store
        .get::<WorkshopTime>()
        .ok_or_else(|| Error::string("workshop time is not installed"))?;
    let model_context = ModelContext::new(database.clone(), cursors.clone(), workshop_time.clone())
        .map_err(Error::msg)?;
    ctx.shared_store.insert(model_context.clone());
    ctx.shared_store
        .insert(CustomerModel::new(model_context.clone()));
    ctx.shared_store
        .insert(CalendarModel::from_context(&model_context).map_err(Error::msg)?);
    ctx.shared_store
        .insert(VehicleModel::from_context(&model_context).map_err(Error::msg)?);
    ctx.shared_store
        .insert(InterventionModel::from_context(&model_context).map_err(Error::msg)?);
    ctx.shared_store
        .insert(TechnicalNoteModel::from_context(&model_context).map_err(Error::msg)?);
    ctx.shared_store
        .insert(InvoiceModel::from_context(&model_context).map_err(Error::msg)?);
    ctx.shared_store
        .insert(AttachmentReconciliation::from_context(&model_context).map_err(Error::msg)?);
    ctx.shared_store
        .insert(AttachmentModel::from_context(&model_context).map_err(Error::msg)?);
    Ok(())
}
