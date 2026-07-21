use loco_rs::testing::request::boot_test;
use pipauto::{
    app::App,
    database::client::AppDatabase,
    domain::{AttachmentId, PageLimit, VehicleId},
    models::{
        attachment::{
            AttachmentDigest, AttachmentFilePointer, AttachmentMediaType, AttachmentOwner,
            NewAttachmentReservation,
        },
        technical_note::NewTechnicalNote,
    },
    repositories::{
        attachment::{AttachmentFileStore, AttachmentRepository},
        surreal::{
            attachment::{SurrealAttachmentFileStore, SurrealAttachmentRepository},
            technical_note::SurrealTechnicalNoteRepository,
        },
        technical_note::{TechnicalNoteFilter, TechnicalNoteRepository},
    },
};

use crate::support::define_attachment_memory_bucket;

#[tokio::test]
async fn technical_note_repository_crud_archive_and_restore() {
    let (notes, _, _) = repositories().await;
    let value = note("Water pump", "Use the locking tool", vec!["cooling"]);
    let created = notes.create(&value).await.expect("create note");
    assert_eq!(
        notes.find_by_id(&created.id).await.expect("find"),
        Some(created.clone())
    );

    let updated = note(
        "Water pump procedure",
        "Lock before removal",
        vec!["cooling", "vw"],
    );
    assert_eq!(
        notes
            .update(&created.id, &updated)
            .await
            .expect("update")
            .tags
            .len(),
        2
    );
    assert!(notes
        .archive(&created.id)
        .await
        .expect("archive")
        .is_archived());
    assert!(!notes
        .restore(&created.id)
        .await
        .expect("restore")
        .is_archived());
}

#[tokio::test]
async fn technical_note_search_combines_full_text_exact_filters_and_stable_cursor() {
    let (notes, _, _) = repositories().await;
    for (title, body, tags, make) in [
        (
            "Water pump",
            "Use locking tool",
            vec!["cooling"],
            "Volkswagen",
        ),
        (
            "Water diagnosis",
            "Inspect pump",
            vec!["cooling"],
            "Volkswagen",
        ),
        ("Water pump", "Bleed circuit", vec!["cooling"], "Peugeot"),
    ] {
        let value = NewTechnicalNote::new(
            title.into(),
            body.into(),
            tags.into_iter().map(str::to_owned).collect(),
            None,
            None,
            Some(make.into()),
            None,
            None,
        )
        .expect("valid note");
        notes.create(&value).await.expect("create note");
    }
    let filter = TechnicalNoteFilter {
        query: Some("water".into()),
        tags: vec!["cooling".into()],
        make: Some("volkswagen".into()),
        ..TechnicalNoteFilter::default()
    };
    let first = notes
        .list(&filter, PageLimit::new(1).expect("limit"), None)
        .await
        .expect("first page");
    assert_eq!(first.items.len(), 1);
    let second = notes
        .list(
            &filter,
            PageLimit::new(1).expect("limit"),
            first.next.as_ref(),
        )
        .await
        .expect("second page");
    assert_eq!(second.items.len(), 1);
    assert_ne!(first.items[0].id, second.items[0].id);
}

#[tokio::test]
async fn attachment_repository_hides_transitions_and_persists_stored_state() {
    let (_, attachments, client) = repositories().await;
    let owner = AttachmentOwner::Vehicle(VehicleId::parse("repository_vehicle").expect("id"));
    client.query("CREATE customer:repository_owner CONTENT { display_name: 'Owner', display_name_normalized: 'owner' }; CREATE vehicle:repository_vehicle CONTENT { customer: customer:repository_owner, make: 'VW', make_normalized: 'vw', model: 'Golf', model_normalized: 'golf' };").await.expect("fixtures").check().expect("fixtures valid");
    let input = NewAttachmentReservation::new(
        AttachmentId::parse("repository_attachment").expect("attachment id"),
        owner.clone(),
        "Photo.jpg".into(),
        AttachmentMediaType::Jpeg,
        None,
        AttachmentFilePointer::new(
            "pipauto_attachments",
            "0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .expect("file pointer"),
    )
    .expect("reservation");
    let pending = attachments
        .reserve(&input)
        .await
        .expect("reserve attachment");
    assert_eq!(pending.storage_state.as_str(), "pending");
    assert!(attachments
        .list_stored_owner(&owner)
        .await
        .expect("list")
        .is_empty());
    let digest = AttachmentDigest::calculate(b"fixture");
    let stored = attachments
        .finalize(&pending.id, 7, &digest)
        .await
        .expect("finalize attachment");
    assert_eq!(stored.storage_state.as_str(), "stored");
    assert_eq!(
        attachments
            .list_stored_owner(&owner)
            .await
            .expect("list")
            .len(),
        1
    );
    attachments
        .mark_deleting(&stored.id)
        .await
        .expect("mark deleting");
    attachments
        .delete_deleting(&stored.id)
        .await
        .expect("delete");
    assert!(attachments
        .find_internal(&stored.id)
        .await
        .expect("find")
        .is_none());
}

#[tokio::test]
async fn attachment_file_store_uses_typed_pointer_bytes_and_idempotent_delete() {
    let client = test_client().await;
    define_attachment_memory_bucket(&client).await;
    let store = SurrealAttachmentFileStore::new(client);
    let pointer = AttachmentFilePointer::new(
        "pipauto_attachments",
        "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdef",
    )
    .expect("file pointer");
    let bytes = b"private attachment bytes";
    store
        .put_if_absent(&pointer, bytes)
        .await
        .expect("put object");
    assert_eq!(
        store.head(&pointer).await.expect("head object").byte_size,
        u64::try_from(bytes.len()).expect("fixture size")
    );
    assert_eq!(store.get(&pointer).await.expect("get object"), bytes);
    assert!(store.put_if_absent(&pointer, bytes).await.is_err());
    store.delete(&pointer).await.expect("delete object");
    store.delete(&pointer).await.expect("idempotent delete");
}

fn note(title: &str, body: &str, tags: Vec<&str>) -> NewTechnicalNote {
    NewTechnicalNote::new(
        title.into(),
        body.into(),
        tags.into_iter().map(str::to_owned).collect(),
        None,
        None,
        None,
        None,
        None,
    )
    .expect("valid note")
}

async fn test_client() -> surrealdb::Surreal<surrealdb::engine::any::Any> {
    let boot = boot_test::<App>().await.expect("application should boot");
    boot.app_context
        .shared_store
        .get::<AppDatabase>()
        .expect("database")
        .client()
        .expect("client")
}

async fn repositories() -> (
    SurrealTechnicalNoteRepository,
    SurrealAttachmentRepository,
    surrealdb::Surreal<surrealdb::engine::any::Any>,
) {
    let client = test_client().await;
    (
        SurrealTechnicalNoteRepository::new(client.clone()),
        SurrealAttachmentRepository::new(client.clone()),
        client,
    )
}
