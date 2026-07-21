use loco_rs::testing::request::boot_test;
use pipauto::{
    app::App,
    database::client::{AppDatabase, AttachmentBucketStatus, ATTACHMENT_BUCKET_NAME},
    models::attachment::AttachmentFilePointer,
    repositories::{
        attachment::AttachmentFileStore, surreal::attachment::SurrealAttachmentFileStore,
    },
};
use surrealdb::types::{Bytes, File, Value};

use crate::support::define_attachment_memory_bucket;

#[tokio::test]
async fn surrealdb_bucket_capability_is_ready_and_isolated_by_test_database() {
    let boot = boot_test::<App>().await.expect("application should boot");
    let database = boot
        .app_context
        .shared_store
        .get::<AppDatabase>()
        .expect("application database should be installed");
    let client = database
        .client()
        .expect("test database should have a client");

    assert_eq!(
        database
            .attachment_bucket_status()
            .await
            .expect("catalog inspection should succeed"),
        AttachmentBucketStatus::Ready,
        "test startup must define the disposable memory bucket"
    );

    define_attachment_memory_bucket(&client).await;
    assert_eq!(
        database
            .attachment_bucket_status()
            .await
            .expect("defined bucket should be inspectable"),
        AttachmentBucketStatus::Ready
    );

    let files = SurrealAttachmentFileStore::new(client.clone());
    let empty_page = files
        .list(None, 1)
        .await
        .expect("an empty bucket should list without a start cursor");
    assert!(empty_page.pointers.is_empty());
    assert!(empty_page.next_cursor.is_none());

    for key in ["00".repeat(24), "11".repeat(24)] {
        let pointer = AttachmentFilePointer::new(ATTACHMENT_BUCKET_NAME, key)
            .expect("test pointer should satisfy the opaque-key contract");
        files
            .put_if_absent(&pointer, &[0, 1, 2, 255])
            .await
            .expect("test object should be writable");
    }
    let first_page = files
        .list(None, 1)
        .await
        .expect("the first bucket page should omit a start cursor");
    assert_eq!(first_page.pointers.len(), 1);
    let cursor = first_page
        .next_cursor
        .expect("a full first page should expose a cursor");
    let second_page = files
        .list(Some(&cursor), 1)
        .await
        .expect("the next bucket page should bind its start cursor");
    assert_eq!(second_page.pointers.len(), 1);
    assert_ne!(first_page.pointers, second_page.pointers);

    let file = File::new(ATTACHMENT_BUCKET_NAME, "contract-check");
    let bytes = Bytes::from(vec![0, 1, 2, 255]);
    client
        .query("RETURN file::put_if_not_exists($file, $bytes);")
        .bind(("file", file.clone()))
        .bind(("bytes", bytes.clone()))
        .await
        .expect("typed file and byte bindings should execute")
        .check()
        .expect("memory bucket write should succeed");
    let mut response = client
        .query("RETURN file::get($file);")
        .bind(("file", file))
        .await
        .expect("bucket read should execute")
        .check()
        .expect("bucket read should succeed");
    let stored = response
        .take::<Value>(0)
        .expect("file bytes should decode as a database value");
    let Value::Bytes(stored) = stored else {
        panic!("file::get should return the SDK Bytes value");
    };
    assert_eq!(&*stored, &*bytes);

    let second_boot = boot_test::<App>()
        .await
        .expect("a second application should boot with a disposable database");
    let second_database = second_boot
        .app_context
        .shared_store
        .get::<AppDatabase>()
        .expect("second application database should be installed");
    assert_eq!(
        second_database
            .attachment_bucket_status()
            .await
            .expect("second catalog inspection should succeed"),
        AttachmentBucketStatus::Ready,
        "each disposable test database must receive its own ready bucket"
    );
}
