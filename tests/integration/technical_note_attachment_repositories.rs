use loco_rs::testing::request::boot_test;
use pipauto::{
    app::App,
    database::client::AppDatabase,
    domain::{PageLimit, VehicleId},
    models::{
        attachment::{AttachmentMediaType, AttachmentOwner, NewAttachmentMetadata},
        technical_note::NewTechnicalNote,
    },
    repositories::{
        attachment::AttachmentRepository,
        surreal::{
            attachment::SurrealAttachmentRepository, technical_note::SurrealTechnicalNoteRepository,
        },
        technical_note::{TechnicalNoteFilter, TechnicalNoteRepository},
    },
};

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
async fn attachment_repository_keeps_owner_and_metadata_only_state() {
    let (_, attachments, client) = repositories().await;
    let owner = AttachmentOwner::Vehicle(VehicleId::parse("repository_vehicle").expect("id"));
    client.query("CREATE customer:repository_owner CONTENT { display_name: 'Owner', display_name_normalized: 'owner' }; CREATE vehicle:repository_vehicle CONTENT { customer: customer:repository_owner, make: 'VW', make_normalized: 'vw', model: 'Golf', model_normalized: 'golf' };").await.expect("fixtures").check().expect("fixtures valid");
    let input = NewAttachmentMetadata::new(
        owner.clone(),
        "Photo.jpg".into(),
        AttachmentMediaType::Jpeg,
        Some(42),
        None,
    )
    .expect("metadata");
    let created = attachments.create(&input).await.expect("create attachment");
    assert_eq!(created.storage_state(), "metadata_only");
    assert_eq!(attachments.list_owner(&owner).await.expect("list").len(), 1);
    attachments.delete(&created.id).await.expect("delete");
    assert!(attachments
        .find_by_id(&created.id)
        .await
        .expect("find")
        .is_none());
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
