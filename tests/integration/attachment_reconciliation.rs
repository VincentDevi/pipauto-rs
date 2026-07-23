use std::sync::Arc;

use pipauto::{
    domain::{AttachmentId, VehicleId},
    models::attachment::{
        AttachmentDigest, AttachmentFilePointer, AttachmentMediaType, AttachmentOwner,
        AttachmentReconciliation as AttachmentReconciler, AttachmentReconciliationError,
        AttachmentStorageState, NewAttachmentReservation, ReconciliationMode,
        ATTACHMENT_BUCKET_NAME,
    },
    testing::persistence::attachment::{
        memory::{
            FileOperation, InMemoryAttachmentFileStore, InMemoryAttachmentRepository,
            RepositoryOperation,
        },
        AttachmentFileStore, AttachmentFileStoreError, AttachmentRepository,
    },
};

const READY_KEY: &str = "111111111111111111111111111111111111111111111111";
const INCOMPLETE_KEY: &str = "222222222222222222222222222222222222222222222222";
const DELETING_KEY: &str = "333333333333333333333333333333333333333333333333";
const STORED_MISSING_KEY: &str = "444444444444444444444444444444444444444444444444";
const STORED_WRONG_KEY: &str = "555555555555555555555555555555555555555555555555";
const ORPHAN_KEY: &str = "666666666666666666666666666666666666666666666666";
const INVALID_PENDING_KEY: &str = "888888888888888888888888888888888888888888888888";
const STORED_CHECKSUM_KEY: &str = "999999999999999999999999999999999999999999999999";

#[tokio::test]
async fn attachment_reconciliation_dry_run_reports_without_mutation() {
    let fixture = Fixture::new();
    fixture.seed_all_states().await;
    let records_before = fixture.records.snapshot().expect("record snapshot");
    let objects_before = fixture.objects().await;

    let report = fixture
        .reconciler
        .reconcile(ReconciliationMode::DryRun)
        .await
        .expect("dry-run scan");

    assert_eq!(report.pending.len(), 3);
    assert_eq!(report.pending_ready, vec![id("pending_ready")]);
    assert_eq!(report.pending_missing, vec![id("pending_incomplete")]);
    assert_eq!(report.pending_invalid, vec![id("pending_invalid")]);
    assert_eq!(report.deleting, vec![id("deleting")]);
    assert_eq!(report.deleting_object_present, vec![id("deleting")]);
    assert_eq!(report.stored_missing, vec![id("stored_missing")]);
    assert_eq!(report.stored_wrong_size, vec![id("stored_wrong_size")]);
    assert_eq!(
        report.stored_checksum_mismatch,
        vec![id("stored_checksum_mismatch")]
    );
    assert_eq!(report.orphan_objects, 1);
    assert_eq!(
        fixture.records.snapshot().expect("record snapshot"),
        records_before
    );
    assert_eq!(fixture.objects().await, objects_before);
}

#[tokio::test]
async fn attachment_reconciliation_apply_repairs_only_documented_states() {
    let fixture = Fixture::new();
    fixture.seed_all_states().await;

    let report = fixture
        .reconciler
        .reconcile(ReconciliationMode::Apply)
        .await
        .expect("apply reconciliation");

    assert_eq!(report.finalized_pending, 1);
    assert_eq!(report.removed_incomplete_pending, 2);
    assert_eq!(report.resumed_deleting, 1);
    assert_eq!(report.removed_orphans, 1);
    assert_eq!(
        fixture
            .records
            .find_internal(&id("pending_ready"))
            .await
            .expect("ready record")
            .expect("ready retained")
            .storage_state,
        AttachmentStorageState::Stored
    );
    assert!(fixture
        .records
        .find_internal(&id("pending_incomplete"))
        .await
        .expect("incomplete record")
        .is_none());
    assert!(fixture
        .records
        .find_internal(&id("pending_invalid"))
        .await
        .expect("invalid record")
        .is_none());
    assert!(fixture
        .records
        .find_internal(&id("deleting"))
        .await
        .expect("deleting record")
        .is_none());
    for stored_id in [
        "stored_missing",
        "stored_wrong_size",
        "stored_checksum_mismatch",
    ] {
        assert_eq!(
            fixture
                .records
                .find_internal(&id(stored_id))
                .await
                .expect("stored record lookup")
                .expect("stored corruption must remain known")
                .storage_state,
            AttachmentStorageState::Stored
        );
    }
    assert!(!fixture.files.contains(&pointer(ORPHAN_KEY)));
}

#[tokio::test]
async fn attachment_failure_injection_stops_before_mutation_when_bucket_is_unavailable() {
    let fixture = Fixture::new();
    fixture
        .seed_pending("pending_ready", READY_KEY, Some(pdf(b"ready")))
        .await;
    fixture
        .files
        .fail_next(FileOperation::List, AttachmentFileStoreError::Unavailable);
    let before = fixture.records.snapshot().expect("record snapshot");

    assert_eq!(
        fixture
            .reconciler
            .reconcile(ReconciliationMode::Apply)
            .await,
        Err(AttachmentReconciliationError::BucketUnavailable)
    );
    assert_eq!(fixture.records.snapshot().expect("record snapshot"), before);
}

#[tokio::test]
async fn attachment_failure_injection_keeps_interrupted_apply_retryable() {
    let fixture = Fixture::new();
    fixture
        .seed_pending("pending_ready", READY_KEY, Some(pdf(b"ready")))
        .await;
    fixture.records.fail_next(
        RepositoryOperation::Finalize,
        pipauto::testing::persistence::RepositoryError::Unavailable,
    );

    assert_eq!(
        fixture
            .reconciler
            .reconcile(ReconciliationMode::Apply)
            .await,
        Err(AttachmentReconciliationError::RecordStorageUnavailable)
    );
    assert_eq!(
        fixture.records.snapshot().expect("snapshot")[0].storage_state,
        AttachmentStorageState::Pending
    );
    fixture
        .reconciler
        .reconcile(ReconciliationMode::Apply)
        .await
        .expect("retry completes");
    assert_eq!(
        fixture.records.snapshot().expect("snapshot")[0].storage_state,
        AttachmentStorageState::Stored
    );
}

#[tokio::test]
async fn attachment_security_reconciliation_output_excludes_private_storage_and_customer_data() {
    let fixture = Fixture::new();
    fixture.seed_all_states().await;
    let report = fixture
        .reconciler
        .reconcile(ReconciliationMode::DryRun)
        .await
        .expect("dry-run scan");
    let output = report.safe_output(ReconciliationMode::DryRun);

    for sensitive in [
        ATTACHMENT_BUCKET_NAME,
        READY_KEY,
        ORPHAN_KEY,
        "customer",
        "workshop-secret.pdf",
        "%PDF-1.7",
        "cookie",
        "csrf",
    ] {
        assert!(!output.contains(sensitive), "leaked {sensitive}");
    }
    assert!(output.contains("pending_ready"));
}

struct Fixture {
    records: Arc<InMemoryAttachmentRepository>,
    files: Arc<InMemoryAttachmentFileStore>,
    reconciler: AttachmentReconciler,
}

impl Fixture {
    fn new() -> Self {
        let records = Arc::new(InMemoryAttachmentRepository::default());
        let files = Arc::new(InMemoryAttachmentFileStore::default());
        let reconciler = AttachmentReconciler::new(records.clone(), files.clone());
        Self {
            records,
            files,
            reconciler,
        }
    }

    async fn seed_all_states(&self) {
        self.seed_pending("pending_ready", READY_KEY, Some(pdf(b"ready")))
            .await;
        self.seed_pending("pending_incomplete", INCOMPLETE_KEY, None)
            .await;
        self.seed_pending(
            "pending_invalid",
            INVALID_PENDING_KEY,
            Some(b"not an approved media type".to_vec()),
        )
        .await;
        self.seed_pending("deleting", DELETING_KEY, Some(pdf(b"delete")))
            .await;
        self.records
            .mark_deleting(&id("deleting"))
            .await
            .expect("mark deleting");
        self.seed_stored("stored_missing", STORED_MISSING_KEY, pdf(b"missing"))
            .await;
        self.files
            .delete(&pointer(STORED_MISSING_KEY))
            .await
            .expect("remove stored object");
        let original = pdf(b"right-size");
        self.seed_stored("stored_wrong_size", STORED_WRONG_KEY, original)
            .await;
        self.files
            .delete(&pointer(STORED_WRONG_KEY))
            .await
            .expect("remove original object");
        self.files
            .put_if_absent(&pointer(STORED_WRONG_KEY), &pdf(b"wrong-size-longer"))
            .await
            .expect("replace with wrong-sized object");
        let checksum_a = pdf(b"checksum-a");
        self.seed_stored("stored_checksum_mismatch", STORED_CHECKSUM_KEY, checksum_a)
            .await;
        self.files
            .delete(&pointer(STORED_CHECKSUM_KEY))
            .await
            .expect("remove checksum object");
        self.files
            .put_if_absent(&pointer(STORED_CHECKSUM_KEY), &pdf(b"checksum-b"))
            .await
            .expect("replace with checksum mismatch");
        self.files
            .put_if_absent(&pointer(ORPHAN_KEY), &pdf(b"orphan"))
            .await
            .expect("orphan object");
    }

    async fn seed_pending(&self, attachment_id: &str, key: &str, bytes: Option<Vec<u8>>) {
        self.records
            .reserve(&reservation(attachment_id, key))
            .await
            .expect("pending reservation");
        if let Some(bytes) = bytes {
            self.files
                .put_if_absent(&pointer(key), &bytes)
                .await
                .expect("pending object");
        }
    }

    async fn seed_stored(&self, attachment_id: &str, key: &str, bytes: Vec<u8>) {
        self.seed_pending(attachment_id, key, Some(bytes.clone()))
            .await;
        self.records
            .finalize(
                &id(attachment_id),
                u64::try_from(bytes.len()).expect("fixture size"),
                &AttachmentDigest::calculate(&bytes),
            )
            .await
            .expect("stored record");
    }

    async fn objects(&self) -> Vec<String> {
        self.files
            .list(None, 200)
            .await
            .expect("bucket list")
            .pointers
            .into_iter()
            .map(|pointer| pointer.key().to_owned())
            .collect()
    }
}

fn reservation(attachment_id: &str, key: &str) -> NewAttachmentReservation {
    NewAttachmentReservation::new(
        id(attachment_id),
        AttachmentOwner::Vehicle(VehicleId::parse("reconciliation_vehicle").expect("vehicle id")),
        "workshop-secret.pdf".to_owned(),
        AttachmentMediaType::Pdf,
        Some("customer private caption".to_owned()),
        pointer(key),
    )
    .expect("reservation")
}

fn id(value: &str) -> AttachmentId {
    AttachmentId::parse(value).expect("attachment id")
}

fn pointer(key: &str) -> AttachmentFilePointer {
    AttachmentFilePointer::new(ATTACHMENT_BUCKET_NAME, key).expect("attachment pointer")
}

fn pdf(suffix: &[u8]) -> Vec<u8> {
    let mut bytes = b"%PDF-1.7\n".to_vec();
    bytes.extend_from_slice(suffix);
    bytes
}
