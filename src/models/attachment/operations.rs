//! Failure-safe attachment model operations.

use std::sync::Arc;

use crate::{
    domain::{AttachmentId, ValidationCode, ValidationError, ValidationErrors},
    models::{
        attachment::{
            AttachmentDigest, AttachmentFilePointer, AttachmentMediaType, AttachmentModelError,
            AttachmentOwner, AttachmentRecord, AttachmentStorageState, NewAttachmentReservation,
            StoredAttachment, UpdateAttachmentMetadata, ATTACHMENT_BUCKET_NAME,
        },
        intervention::{repository::InterventionRepository, Intervention},
        persistence_error::PersistenceError as RepositoryError,
        technical_note::{repository::TechnicalNoteRepository, TechnicalNote},
        vehicle::{repository::VehicleRepository, Vehicle},
    },
};

use super::{
    repository::{AttachmentFileStore, AttachmentFileStoreError, AttachmentRepository},
    WorkflowError,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UploadAttachment {
    pub bytes: Vec<u8>,
    pub display_name: Option<String>,
    pub original_filename: Option<String>,
    pub caption: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteAttachmentMetadata {
    pub display_name: String,
    pub media_type: String,
    pub byte_size: Option<u64>,
    pub caption: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachmentContent {
    pub attachment: StoredAttachment,
    pub bytes: Vec<u8>,
}

pub trait AttachmentIdentitySource: Send + Sync {
    fn generate(&self) -> Result<(AttachmentId, AttachmentFilePointer), WorkflowError>;
}

#[derive(Clone, Default)]
pub struct OsAttachmentIdentitySource;

impl AttachmentIdentitySource for OsAttachmentIdentitySource {
    fn generate(&self) -> Result<(AttachmentId, AttachmentFilePointer), WorkflowError> {
        let mut id_bytes = [0_u8; 24];
        let mut key_bytes = [0_u8; 24];
        getrandom::fill(&mut id_bytes).map_err(|_| WorkflowError::Unavailable)?;
        getrandom::fill(&mut key_bytes).map_err(|_| WorkflowError::Unavailable)?;
        let id = AttachmentId::parse(hex(&id_bytes)).map_err(|_| WorkflowError::Internal)?;
        let pointer = AttachmentFilePointer::new(ATTACHMENT_BUCKET_NAME, hex(&key_bytes))
            .map_err(|_| WorkflowError::Internal)?;
        Ok((id, pointer))
    }
}

#[derive(Clone)]
pub struct AttachmentModel {
    attachments: Arc<dyn AttachmentRepository>,
    files: Arc<dyn AttachmentFileStore>,
    vehicles: Arc<dyn VehicleRepository>,
    interventions: Arc<dyn InterventionRepository>,
    notes: Arc<dyn TechnicalNoteRepository>,
    identities: Arc<dyn AttachmentIdentitySource>,
}

impl AttachmentModel {
    pub fn from_context(
        context: &crate::models::ModelContext,
    ) -> Result<Self, crate::models::ModelError> {
        Ok(Self::new(
            context.attachments(),
            context.attachment_files(),
            context.vehicles(),
            context.interventions(),
            context.technical_notes(),
        ))
    }

    pub fn new(
        attachments: Arc<dyn AttachmentRepository>,
        files: Arc<dyn AttachmentFileStore>,
        vehicles: Arc<dyn VehicleRepository>,
        interventions: Arc<dyn InterventionRepository>,
        notes: Arc<dyn TechnicalNoteRepository>,
    ) -> Self {
        Self::with_identity_source(
            attachments,
            files,
            vehicles,
            interventions,
            notes,
            Arc::new(OsAttachmentIdentitySource),
        )
    }

    pub fn with_identity_source(
        attachments: Arc<dyn AttachmentRepository>,
        files: Arc<dyn AttachmentFileStore>,
        vehicles: Arc<dyn VehicleRepository>,
        interventions: Arc<dyn InterventionRepository>,
        notes: Arc<dyn TechnicalNoteRepository>,
        identities: Arc<dyn AttachmentIdentitySource>,
    ) -> Self {
        Self {
            attachments,
            files,
            vehicles,
            interventions,
            notes,
            identities,
        }
    }

    /// Store validated bytes and return only after the record reaches `stored`.
    ///
    /// # Errors
    ///
    /// Returns typed validation, lifecycle, persistence, and temporary storage failures.
    pub async fn upload(
        &self,
        owner: AttachmentOwner,
        command: UploadAttachment,
    ) -> Result<StoredAttachment, WorkflowError> {
        let media_type = AttachmentMediaType::detect(&command.bytes).map_err(model_validation)?;
        let byte_size = u64::try_from(command.bytes.len()).map_err(|_| WorkflowError::Internal)?;
        let digest = AttachmentDigest::calculate(&command.bytes);
        let display_name = display_name(command.display_name, command.original_filename)?;
        self.require_active_owner(&owner).await?;
        let (id, file) = self.identities.generate()?;
        let reservation = NewAttachmentReservation::new(
            id,
            owner,
            display_name,
            media_type,
            command.caption,
            file,
        )
        .map_err(model_validation)?;
        let pending = self.attachments.reserve(&reservation).await?;

        if let Err(error) = self
            .files
            .put_if_absent(&pending.file, &command.bytes)
            .await
        {
            let delete_object = error != AttachmentFileStoreError::Collision;
            self.compensate(&pending, delete_object).await;
            return Err(file_error(error));
        }
        match self.files.head(&pending.file).await {
            Ok(head) if head.byte_size == byte_size => {}
            Ok(_) => {
                self.compensate(&pending, true).await;
                return Err(WorkflowError::Internal);
            }
            Err(error) => {
                self.compensate(&pending, true).await;
                return Err(file_error(error));
            }
        }
        if let Err(error) = self.require_active_owner(&pending.owner).await {
            self.compensate(&pending, true).await;
            return Err(error);
        }

        let stored = self
            .attachments
            .finalize(&pending.id, byte_size, &digest)
            .await
            .map_err(finalize_error)?;
        stored.try_into().map_err(|_| WorkflowError::Internal)
    }

    /// Temporary compatibility endpoint for pre-VIN-67 transports.
    pub async fn create(
        &self,
        _owner: AttachmentOwner,
        _command: WriteAttachmentMetadata,
    ) -> Result<StoredAttachment, WorkflowError> {
        Err(validation(
            "file",
            "Select a supported PDF or image file to upload.",
        ))
    }

    pub async fn list(
        &self,
        owner: &AttachmentOwner,
    ) -> Result<Vec<StoredAttachment>, WorkflowError> {
        self.require_owner(owner).await?;
        self.attachments
            .list_stored_owner(owner)
            .await?
            .into_iter()
            .map(|record| record.try_into().map_err(|_| WorkflowError::Internal))
            .collect()
    }

    pub async fn get(&self, id: &AttachmentId) -> Result<StoredAttachment, WorkflowError> {
        let record = self
            .attachments
            .find_stored(id)
            .await?
            .ok_or(WorkflowError::NotFound)?;
        record.try_into().map_err(|_| WorkflowError::Internal)
    }

    pub async fn content(&self, id: &AttachmentId) -> Result<AttachmentContent, WorkflowError> {
        let attachment = self.get(id).await?;
        let bytes = self
            .files
            .get(attachment.file())
            .await
            .map_err(content_file_error)?;
        let byte_size = u64::try_from(bytes.len()).map_err(|_| WorkflowError::Internal)?;
        if byte_size != attachment.byte_size
            || AttachmentDigest::calculate(&bytes) != attachment.digest
        {
            return Err(WorkflowError::Unavailable);
        }
        Ok(AttachmentContent { attachment, bytes })
    }

    pub async fn update(
        &self,
        id: &AttachmentId,
        command: WriteAttachmentMetadata,
    ) -> Result<StoredAttachment, WorkflowError> {
        let current = self.get(id).await?;
        self.require_active_owner(&current.owner).await?;
        let value = UpdateAttachmentMetadata::new(command.display_name, command.caption)
            .map_err(model_validation)?;
        self.attachments
            .update_metadata(id, &value)
            .await?
            .try_into()
            .map_err(|_| WorkflowError::Internal)
    }

    pub async fn delete(&self, id: &AttachmentId) -> Result<(), WorkflowError> {
        let current = self
            .attachments
            .find_internal(id)
            .await?
            .ok_or(WorkflowError::NotFound)?;
        let deleting = if current.storage_state == AttachmentStorageState::Deleting {
            current
        } else {
            self.require_active_owner(&current.owner).await?;
            self.attachments.mark_deleting(id).await?
        };
        match self.files.delete(&deleting.file).await {
            Ok(()) | Err(AttachmentFileStoreError::MissingObject) => {}
            Err(error) => return Err(file_error(error)),
        }
        self.attachments.delete_deleting(id).await?;
        Ok(())
    }

    async fn compensate(&self, pending: &AttachmentRecord, delete_object: bool) {
        let Ok(deleting) = self.attachments.mark_deleting(&pending.id).await else {
            return;
        };
        if delete_object && self.files.delete(&deleting.file).await.is_err() {
            return;
        }
        let _ = self.attachments.delete_deleting(&deleting.id).await;
    }

    async fn require_owner(&self, owner: &AttachmentOwner) -> Result<(), WorkflowError> {
        match owner {
            AttachmentOwner::Vehicle(id) => {
                self.vehicle(id).await?;
            }
            AttachmentOwner::Intervention(id) => {
                self.intervention(id).await?;
            }
            AttachmentOwner::TechnicalNote(id) => {
                self.note(id).await?;
            }
        }
        Ok(())
    }

    async fn require_active_owner(&self, owner: &AttachmentOwner) -> Result<(), WorkflowError> {
        match owner {
            AttachmentOwner::Vehicle(id) => {
                if self.vehicle(id).await?.is_archived() {
                    return Err(WorkflowError::Conflict);
                }
            }
            AttachmentOwner::Intervention(id) => {
                let intervention = self.intervention(id).await?;
                if intervention.status != crate::models::intervention::InterventionStatus::Draft {
                    return Err(WorkflowError::Conflict);
                }
                if self.vehicle(&intervention.vehicle_id).await?.is_archived() {
                    return Err(WorkflowError::Conflict);
                }
            }
            AttachmentOwner::TechnicalNote(id) => {
                if self.note(id).await?.is_archived() {
                    return Err(WorkflowError::Conflict);
                }
            }
        }
        Ok(())
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

    async fn note(
        &self,
        id: &crate::domain::TechnicalNoteId,
    ) -> Result<TechnicalNote, WorkflowError> {
        self.notes
            .find_by_id(id)
            .await?
            .ok_or(WorkflowError::NotFound)
    }
}

fn display_name(
    supplied: Option<String>,
    original_filename: Option<String>,
) -> Result<String, WorkflowError> {
    let supplied = supplied.filter(|value| !value.trim().is_empty());
    let candidate = supplied.or_else(|| {
        original_filename.map(|filename| {
            let derived: String = filename
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or_default()
                .chars()
                .filter(|character| !character.is_control())
                .collect();
            if matches!(derived.trim(), "." | "..") {
                String::new()
            } else {
                derived
            }
        })
    });
    UpdateAttachmentMetadata::new(candidate.unwrap_or_default(), None)
        .map(|value| value.display_name)
        .map_err(model_validation)
}

fn model_validation(error: AttachmentModelError) -> WorkflowError {
    match error {
        AttachmentModelError::Required => {
            validation("display_name", "Enter an attachment display name.")
        }
        AttachmentModelError::TooLong => {
            validation("display_name", "Attachment metadata text is too long.")
        }
        AttachmentModelError::EmptyContent => validation("file", "Select a non-empty file."),
        AttachmentModelError::ContentTooLarge => {
            validation("file", "Select a file no larger than 25 MiB.")
        }
        AttachmentModelError::UnsupportedMediaType => validation(
            "file",
            "Select a supported PDF, JPEG, PNG, WebP, HEIC, or HEIF file.",
        ),
        AttachmentModelError::InvalidStorageState
        | AttachmentModelError::InvalidStorageFields
        | AttachmentModelError::InvalidFilePointer
        | AttachmentModelError::InvalidDigest => WorkflowError::Internal,
    }
}

fn file_error(error: AttachmentFileStoreError) -> WorkflowError {
    match error {
        AttachmentFileStoreError::Collision => WorkflowError::Conflict,
        AttachmentFileStoreError::Unavailable | AttachmentFileStoreError::MissingObject => {
            WorkflowError::Unavailable
        }
        AttachmentFileStoreError::CorruptData => WorkflowError::Internal,
    }
}

fn content_file_error(error: AttachmentFileStoreError) -> WorkflowError {
    match error {
        AttachmentFileStoreError::MissingObject | AttachmentFileStoreError::Unavailable => {
            WorkflowError::Unavailable
        }
        AttachmentFileStoreError::Collision | AttachmentFileStoreError::CorruptData => {
            WorkflowError::Internal
        }
    }
}

fn finalize_error(error: RepositoryError) -> WorkflowError {
    match error {
        RepositoryError::Unavailable | RepositoryError::Conflict => WorkflowError::Unavailable,
        RepositoryError::NotFound | RepositoryError::CorruptData => WorkflowError::Internal,
    }
}

fn validation(field: &str, message: &str) -> WorkflowError {
    WorkflowError::Validation(ValidationErrors::one(
        ValidationError::new(field, ValidationCode::InvalidFormat, message)
            .expect("static validation metadata is valid"),
    ))
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(char::from(HEX[usize::from(byte >> 4)]));
        value.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    value
}
