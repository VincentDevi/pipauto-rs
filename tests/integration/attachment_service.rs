use std::sync::Arc;

use loco_rs::testing::request::boot_test;
use pipauto::{
    app::App,
    database::client::AppDatabase,
    domain::{AttachmentId, VehicleId},
    models::{
        attachment::{
            AttachmentFilePointer, AttachmentIdentitySource, AttachmentModel as AttachmentService,
            AttachmentOwner, AttachmentStorageState, UploadAttachment, WriteAttachmentMetadata,
        },
        ModelError as WorkflowError,
    },
    testing::persistence::{
        attachment::{
            memory::{
                FileOperation, InMemoryAttachmentFileStore, InMemoryAttachmentRepository,
                RepositoryOperation,
            },
            AttachmentFileStoreError, AttachmentRepository,
        },
        surreal::{
            intervention::SurrealInterventionRepository,
            technical_note::SurrealTechnicalNoteRepository, vehicle::SurrealVehicleRepository,
        },
        RepositoryError,
    },
};

struct FixedIdentity {
    id: &'static str,
    key: &'static str,
}

impl AttachmentIdentitySource for FixedIdentity {
    fn generate(&self) -> Result<(AttachmentId, AttachmentFilePointer), WorkflowError> {
        Ok((
            AttachmentId::parse(self.id).map_err(|_| WorkflowError::Internal)?,
            AttachmentFilePointer::new("pipauto_attachments", self.key)
                .map_err(|_| WorkflowError::Internal)?,
        ))
    }
}

#[tokio::test]
async fn attachment_service_stores_derived_content_and_only_edits_display_metadata() {
    let (service, records, files) = service(
        "attachment_service_success",
        "111111111111111111111111111111111111111111111111",
    )
    .await;
    let bytes = b"%PDF-1.7\nservice fixture".to_vec();
    let stored = service
        .upload(
            owner(),
            UploadAttachment {
                bytes: bytes.clone(),
                display_name: None,
                original_filename: Some("../scan.pdf".into()),
                caption: Some(" Initial ".into()),
            },
        )
        .await
        .expect("upload should finalize");
    assert_eq!(stored.display_name, "scan.pdf");
    assert_eq!(
        stored.byte_size,
        u64::try_from(bytes.len()).expect("fixture size")
    );
    assert_eq!(stored.storage_state(), "stored");
    assert_eq!(service.list(&owner()).await.expect("list").len(), 1);
    assert_eq!(
        service.content(&stored.id).await.expect("content").bytes,
        bytes
    );

    let updated = service
        .update(
            &stored.id,
            WriteAttachmentMetadata {
                display_name: " Workshop scan ".into(),
                media_type: "image/jpeg".into(),
                byte_size: Some(1),
                caption: Some(" Final ".into()),
            },
        )
        .await
        .expect("metadata update");
    assert_eq!(updated.display_name, "Workshop scan");
    assert_eq!(updated.media_type, stored.media_type);
    assert_eq!(updated.byte_size, stored.byte_size);

    let pointer = records.snapshot().expect("snapshot")[0].file.clone();
    service.delete(&stored.id).await.expect("delete");
    assert!(records.snapshot().expect("snapshot").is_empty());
    assert!(!files.contains(&pointer));
}

#[tokio::test]
async fn attachment_failure_injection_leaves_recoverable_truthful_states() {
    let (service, records, files) = service(
        "attachment_service_failure",
        "222222222222222222222222222222222222222222222222",
    )
    .await;
    records.fail_next(RepositoryOperation::Finalize, RepositoryError::Unavailable);
    assert_eq!(
        service
            .upload(
                owner(),
                UploadAttachment {
                    bytes: b"%PDF-1.7\npending".to_vec(),
                    display_name: Some("pending.pdf".into()),
                    original_filename: None,
                    caption: None,
                },
            )
            .await,
        Err(WorkflowError::Unavailable)
    );
    let pending = records.snapshot().expect("snapshot");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].storage_state, AttachmentStorageState::Pending);
    assert!(files.contains(&pending[0].file));
    assert!(service
        .list(&owner())
        .await
        .expect("normal list")
        .is_empty());

    records
        .mark_deleting(&pending[0].id)
        .await
        .expect("operator marks recoverable pending");
    files.fail_next(FileOperation::Delete, AttachmentFileStoreError::Unavailable);
    assert_eq!(
        service.delete(&pending[0].id).await,
        Err(WorkflowError::Unavailable)
    );
    assert_eq!(
        records.snapshot().expect("snapshot")[0].storage_state,
        AttachmentStorageState::Deleting
    );
    service
        .delete(&pending[0].id)
        .await
        .expect("deleting retry should finish");
    assert!(records.snapshot().expect("snapshot").is_empty());
}

#[tokio::test]
async fn attachment_failure_injection_compensates_failed_bucket_write() {
    let (service, records, files) = service(
        "attachment_service_put_failure",
        "333333333333333333333333333333333333333333333333",
    )
    .await;
    files.fail_next(FileOperation::Put, AttachmentFileStoreError::Unavailable);
    assert_eq!(
        service
            .upload(
                owner(),
                UploadAttachment {
                    bytes: b"%PDF-1.7\nfailed".to_vec(),
                    display_name: Some("failed.pdf".into()),
                    original_filename: None,
                    caption: None,
                },
            )
            .await,
        Err(WorkflowError::Unavailable)
    );
    assert!(records.snapshot().expect("compensated records").is_empty());
}

#[tokio::test]
async fn attachment_failure_injection_collision_never_overwrites_existing_bytes() {
    let key = "777777777777777777777777777777777777777777777777";
    let (service, records, files) = service("attachment_collision", key).await;
    let pointer = AttachmentFilePointer::new("pipauto_attachments", key).expect("pointer");
    let existing = b"%PDF-1.7\nexisting private bytes".to_vec();
    pipauto::testing::persistence::attachment::AttachmentFileStore::put_if_absent(
        files.as_ref(),
        &pointer,
        &existing,
    )
    .await
    .expect("collision fixture");

    assert_eq!(
        service
            .upload(
                owner(),
                UploadAttachment {
                    bytes: b"%PDF-1.7\nreplacement bytes".to_vec(),
                    display_name: Some("replacement.pdf".into()),
                    original_filename: None,
                    caption: None,
                },
            )
            .await,
        Err(WorkflowError::Conflict)
    );
    assert!(records.snapshot().expect("compensated record").is_empty());
    assert_eq!(
        pipauto::testing::persistence::attachment::AttachmentFileStore::get(
            files.as_ref(),
            &pointer
        )
        .await
        .expect("existing bytes retained"),
        existing
    );
}

async fn service(
    id: &'static str,
    key: &'static str,
) -> (
    AttachmentService,
    Arc<InMemoryAttachmentRepository>,
    Arc<InMemoryAttachmentFileStore>,
) {
    let boot = boot_test::<App>().await.expect("application should boot");
    let client = boot
        .app_context
        .shared_store
        .get::<AppDatabase>()
        .expect("database")
        .client()
        .expect("client");
    client
        .query("CREATE customer:attachment_owner CONTENT { display_name: 'Owner', display_name_normalized: 'owner' }; CREATE vehicle:attachment_vehicle CONTENT { customer: customer:attachment_owner, make: 'VW', make_normalized: 'vw', model: 'Golf', model_normalized: 'golf' };")
        .await
        .expect("owner fixtures")
        .check()
        .expect("owner fixtures valid");
    let records = Arc::new(InMemoryAttachmentRepository::default());
    let files = Arc::new(InMemoryAttachmentFileStore::default());
    let service = AttachmentService::with_identity_source(
        records.clone(),
        files.clone(),
        Arc::new(SurrealVehicleRepository::new(client.clone())),
        Arc::new(SurrealInterventionRepository::new(client.clone())),
        Arc::new(SurrealTechnicalNoteRepository::new(client)),
        Arc::new(FixedIdentity { id, key }),
    );
    (service, records, files)
}

fn owner() -> AttachmentOwner {
    AttachmentOwner::Vehicle(VehicleId::parse("attachment_vehicle").expect("vehicle id"))
}
