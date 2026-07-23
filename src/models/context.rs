//! Runtime dependencies shared by model operations.

use std::sync::Arc;

use crate::{
    database::client::{AppDatabase, DatabaseAccessError},
    domain::{CursorCodec, WorkshopTime},
    models::{
        attachment::persistence::{SurrealAttachmentFileStore, SurrealAttachmentRepository},
        customer::persistence::SurrealCustomerRepository,
        intervention::persistence::SurrealInterventionRepository,
        invoice::persistence::SurrealInvoiceRepository,
        technical_note::persistence::SurrealTechnicalNoteRepository,
        vehicle::persistence::SurrealVehicleRepository,
    },
};

/// Cheap-to-clone runtime context passed explicitly to public model operations.
#[derive(Clone)]
pub struct ModelContext {
    database: AppDatabase,
    cursors: CursorCodec,
    workshop_time: WorkshopTime,
    customers: Arc<SurrealCustomerRepository>,
    vehicles: Arc<SurrealVehicleRepository>,
    interventions: Arc<SurrealInterventionRepository>,
    technical_notes: Arc<SurrealTechnicalNoteRepository>,
    invoices: Arc<SurrealInvoiceRepository>,
    attachments: Arc<SurrealAttachmentRepository>,
    attachment_files: Arc<SurrealAttachmentFileStore>,
}

impl ModelContext {
    /// Construct the application model context at the composition root.
    pub fn new(
        database: AppDatabase,
        cursors: CursorCodec,
        workshop_time: WorkshopTime,
    ) -> Result<Self, DatabaseAccessError> {
        let client = database.client()?;
        Ok(Self {
            database,
            cursors,
            workshop_time,
            customers: Arc::new(SurrealCustomerRepository::new(client.clone())),
            vehicles: Arc::new(SurrealVehicleRepository::new(client.clone())),
            interventions: Arc::new(SurrealInterventionRepository::new(client.clone())),
            technical_notes: Arc::new(SurrealTechnicalNoteRepository::new(client.clone())),
            invoices: Arc::new(SurrealInvoiceRepository::new(client.clone())),
            attachments: Arc::new(SurrealAttachmentRepository::new(client.clone())),
            attachment_files: Arc::new(SurrealAttachmentFileStore::new(client)),
        })
    }

    /// Selected application database used by infrastructure-aware model operations.
    #[must_use]
    pub const fn database(&self) -> &AppDatabase {
        &self.database
    }

    pub(crate) const fn cursors(&self) -> &CursorCodec {
        &self.cursors
    }

    pub(crate) fn customers(&self) -> Arc<SurrealCustomerRepository> {
        self.customers.clone()
    }

    pub(crate) fn vehicles(&self) -> Arc<SurrealVehicleRepository> {
        self.vehicles.clone()
    }

    pub(crate) fn interventions(&self) -> Arc<SurrealInterventionRepository> {
        self.interventions.clone()
    }

    pub(crate) fn technical_notes(&self) -> Arc<SurrealTechnicalNoteRepository> {
        self.technical_notes.clone()
    }

    pub(crate) fn invoices(&self) -> Arc<SurrealInvoiceRepository> {
        self.invoices.clone()
    }

    pub(crate) fn attachments(&self) -> Arc<SurrealAttachmentRepository> {
        self.attachments.clone()
    }

    pub(crate) fn attachment_files(&self) -> Arc<SurrealAttachmentFileStore> {
        self.attachment_files.clone()
    }

    /// Workshop-local time conversion used by scheduling models and presentation.
    #[must_use]
    pub const fn workshop_time(&self) -> &WorkshopTime {
        &self.workshop_time
    }
}
