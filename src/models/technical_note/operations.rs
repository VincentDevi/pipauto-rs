//! Technical-note model operations, source consistency, search, and archive behavior.

use std::sync::Arc;

use crate::{
    domain::{
        CursorCodec, CursorResource, Page, PageRequest, TechnicalNoteId, ValidationCode,
        ValidationError, ValidationErrors,
    },
    models::{
        intervention::repository::InterventionRepository,
        technical_note::{NewTechnicalNote, TechnicalNote, TechnicalNoteModelError},
        vehicle::repository::VehicleRepository,
    },
};

use super::{
    repository::{TechnicalNoteFilter, TechnicalNoteRepository},
    WorkflowError,
};

pub type WriteTechnicalNote = NewTechnicalNote;

#[derive(Clone)]
pub struct TechnicalNoteModel {
    notes: Arc<dyn TechnicalNoteRepository>,
    vehicles: Arc<dyn VehicleRepository>,
    interventions: Arc<dyn InterventionRepository>,
    cursors: CursorCodec,
    resource: CursorResource,
}

impl TechnicalNoteModel {
    pub fn from_context(
        context: &crate::models::ModelContext,
    ) -> Result<Self, crate::models::ModelError> {
        Ok(Self::new(
            context.technical_notes(),
            context.vehicles(),
            context.interventions(),
            context.cursors().clone(),
        ))
    }

    pub fn new(
        notes: Arc<dyn TechnicalNoteRepository>,
        vehicles: Arc<dyn VehicleRepository>,
        interventions: Arc<dyn InterventionRepository>,
        cursors: CursorCodec,
    ) -> Self {
        Self {
            notes,
            vehicles,
            interventions,
            cursors,
            resource: CursorResource::parse("technical_notes").expect("static resource is valid"),
        }
    }

    pub async fn create(&self, value: WriteTechnicalNote) -> Result<TechnicalNote, WorkflowError> {
        self.validate_sources(&value).await?;
        self.notes.create(&value).await.map_err(Into::into)
    }

    pub async fn get(&self, id: &TechnicalNoteId) -> Result<TechnicalNote, WorkflowError> {
        self.notes
            .find_by_id(id)
            .await?
            .ok_or(WorkflowError::NotFound)
    }

    pub async fn update(
        &self,
        id: &TechnicalNoteId,
        value: WriteTechnicalNote,
    ) -> Result<TechnicalNote, WorkflowError> {
        self.get(id).await?;
        self.validate_sources(&value).await?;
        self.notes.update(id, &value).await.map_err(Into::into)
    }

    pub async fn list(
        &self,
        request: PageRequest<TechnicalNoteFilter>,
    ) -> Result<Page<TechnicalNote>, WorkflowError> {
        let filter = request.filter;
        let after = request
            .after
            .as_ref()
            .map(|cursor| self.cursors.decode(cursor, &self.resource, &filter))
            .transpose()
            .map_err(|_| invalid_cursor())?;
        let page = self
            .notes
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

    pub async fn archive(&self, id: &TechnicalNoteId) -> Result<TechnicalNote, WorkflowError> {
        self.notes.archive(id).await.map_err(Into::into)
    }

    pub async fn restore(&self, id: &TechnicalNoteId) -> Result<TechnicalNote, WorkflowError> {
        self.notes.restore(id).await.map_err(Into::into)
    }

    async fn validate_sources(&self, value: &NewTechnicalNote) -> Result<(), WorkflowError> {
        let vehicle = if let Some(id) = &value.vehicle_id {
            Some(
                self.vehicles
                    .find_by_id(id)
                    .await?
                    .ok_or(WorkflowError::NotFound)?,
            )
        } else {
            None
        };
        if let Some(id) = &value.source_intervention_id {
            let intervention = self
                .interventions
                .find_by_id(id)
                .await?
                .ok_or(WorkflowError::NotFound)?;
            if vehicle
                .as_ref()
                .is_some_and(|vehicle| vehicle.id != intervention.vehicle_id)
            {
                return Err(WorkflowError::Conflict);
            }
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub fn validate_write(
    title: String,
    body: String,
    tags: Vec<String>,
    vehicle_id: Option<crate::domain::VehicleId>,
    source_intervention_id: Option<crate::domain::InterventionId>,
    make: Option<String>,
    model: Option<String>,
    engine: Option<String>,
) -> Result<WriteTechnicalNote, WorkflowError> {
    NewTechnicalNote::new(
        title,
        body,
        tags,
        vehicle_id,
        source_intervention_id,
        make,
        model,
        engine,
    )
    .map_err(model_validation)
}

fn model_validation(error: TechnicalNoteModelError) -> WorkflowError {
    let (field, code, message) = match error {
        TechnicalNoteModelError::Required => (
            "title",
            ValidationCode::Required,
            "Enter required technical-note text.",
        ),
        TechnicalNoteModelError::TooLong => (
            "title",
            ValidationCode::TooLong,
            "Technical-note text is too long.",
        ),
        TechnicalNoteModelError::TooManyTags => {
            ("tags", ValidationCode::TooLong, "Use no more than 20 tags.")
        }
    };
    WorkflowError::Validation(ValidationErrors::one(
        ValidationError::new(field, code, message).expect("static validation metadata is valid"),
    ))
}

fn invalid_cursor() -> WorkflowError {
    WorkflowError::Validation(ValidationErrors::one(
        ValidationError::new(
            "cursor",
            ValidationCode::InvalidFormat,
            "Use a cursor returned by this exact search.",
        )
        .expect("static validation metadata is valid"),
    ))
}
