//! Owner-specific, metadata-only attachment workflows.

use std::sync::Arc;

use crate::{
    domain::{AttachmentId, ValidationCode, ValidationError, ValidationErrors},
    models::{
        attachment::{
            AttachmentMediaType, AttachmentMetadata, AttachmentMetadataError, AttachmentOwner,
            NewAttachmentMetadata,
        },
        intervention::Intervention,
        vehicle::Vehicle,
    },
    repositories::{
        attachment::AttachmentRepository, intervention::InterventionRepository,
        vehicle::VehicleRepository,
    },
};

use super::WorkflowError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteAttachmentMetadata {
    pub display_name: String,
    pub media_type: String,
    pub byte_size: Option<u64>,
    pub caption: Option<String>,
}

#[derive(Clone)]
pub struct AttachmentService {
    attachments: Arc<dyn AttachmentRepository>,
    vehicles: Arc<dyn VehicleRepository>,
    interventions: Arc<dyn InterventionRepository>,
}

impl AttachmentService {
    pub fn new(
        attachments: Arc<dyn AttachmentRepository>,
        vehicles: Arc<dyn VehicleRepository>,
        interventions: Arc<dyn InterventionRepository>,
    ) -> Self {
        Self {
            attachments,
            vehicles,
            interventions,
        }
    }

    pub async fn create(
        &self,
        owner: AttachmentOwner,
        command: WriteAttachmentMetadata,
    ) -> Result<AttachmentMetadata, WorkflowError> {
        self.require_active_owner(&owner).await?;
        let value = validate(owner, command)?;
        self.attachments.create(&value).await.map_err(Into::into)
    }

    pub async fn list(
        &self,
        owner: &AttachmentOwner,
    ) -> Result<Vec<AttachmentMetadata>, WorkflowError> {
        self.require_owner(owner).await?;
        self.attachments.list_owner(owner).await.map_err(Into::into)
    }

    pub async fn get(&self, id: &AttachmentId) -> Result<AttachmentMetadata, WorkflowError> {
        self.attachments
            .find_by_id(id)
            .await?
            .ok_or(WorkflowError::NotFound)
    }

    pub async fn update(
        &self,
        id: &AttachmentId,
        command: WriteAttachmentMetadata,
    ) -> Result<AttachmentMetadata, WorkflowError> {
        let current = self.get(id).await?;
        self.require_active_owner(&current.owner).await?;
        let value = validate(current.owner, command)?;
        self.attachments
            .update(id, &value)
            .await
            .map_err(Into::into)
    }

    pub async fn delete(&self, id: &AttachmentId) -> Result<(), WorkflowError> {
        let current = self.get(id).await?;
        self.require_active_owner(&current.owner).await?;
        self.attachments.delete(id).await.map_err(Into::into)
    }

    async fn require_owner(&self, owner: &AttachmentOwner) -> Result<(), WorkflowError> {
        match owner {
            AttachmentOwner::Vehicle(id) => {
                self.vehicle(id).await?;
            }
            AttachmentOwner::Intervention(id) => {
                self.intervention(id).await?;
            }
        }
        Ok(())
    }

    async fn require_active_owner(&self, owner: &AttachmentOwner) -> Result<(), WorkflowError> {
        let vehicle = match owner {
            AttachmentOwner::Vehicle(id) => self.vehicle(id).await?,
            AttachmentOwner::Intervention(id) => {
                let intervention = self.intervention(id).await?;
                if intervention.status != crate::models::intervention::InterventionStatus::Draft {
                    return Err(WorkflowError::Conflict);
                }
                self.vehicle(&intervention.vehicle_id).await?
            }
        };
        if vehicle.is_archived() {
            Err(WorkflowError::Conflict)
        } else {
            Ok(())
        }
    }

    async fn vehicle(&self, id: &crate::domain::VehicleId) -> Result<Vehicle, WorkflowError> {
        self.vehicles
            .find_by_id(id)
            .await?
            .ok_or(WorkflowError::NotFound)
    }

    async fn intervention(
        &self,
        id: &crate::domain::InterventionId,
    ) -> Result<Intervention, WorkflowError> {
        self.interventions
            .find_by_id(id)
            .await?
            .ok_or(WorkflowError::NotFound)
    }
}

fn validate(
    owner: AttachmentOwner,
    command: WriteAttachmentMetadata,
) -> Result<NewAttachmentMetadata, WorkflowError> {
    if command
        .byte_size
        .is_some_and(|value| value > i64::MAX as u64)
    {
        return Err(validation(
            "byte_size",
            "Enter a supported non-negative byte size.",
        ));
    }
    let media_type = AttachmentMediaType::parse(&command.media_type)
        .map_err(|_| validation("media_type", "Use a supported PDF or image media type."))?;
    NewAttachmentMetadata::new(
        owner,
        command.display_name,
        media_type,
        command.byte_size,
        command.caption,
    )
    .map_err(metadata_validation)
}

fn metadata_validation(error: AttachmentMetadataError) -> WorkflowError {
    match error {
        AttachmentMetadataError::Required => {
            validation("display_name", "Enter an attachment display name.")
        }
        AttachmentMetadataError::TooLong => {
            validation("display_name", "Attachment metadata text is too long.")
        }
        AttachmentMetadataError::UnsupportedMediaType => {
            validation("media_type", "Use a supported PDF or image media type.")
        }
    }
}

fn validation(field: &str, message: &str) -> WorkflowError {
    WorkflowError::Validation(ValidationErrors::one(
        ValidationError::new(field, ValidationCode::InvalidFormat, message)
            .expect("static validation metadata is valid"),
    ))
}
