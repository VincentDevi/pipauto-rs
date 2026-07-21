use loco_rs::testing::request::boot_test;
use pipauto::{
    app::App,
    database::client::{AppDatabase, AttachmentBucketStatus, ATTACHMENT_BUCKET_NAME},
};
use surrealdb::types::{Bytes, File, Value};

use crate::support::define_attachment_memory_bucket;

#[tokio::test]
async fn surrealdb_bucket_capability_is_non_mutating_and_isolated_by_test_database() {
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
        AttachmentBucketStatus::Missing,
        "startup must not define the bucket"
    );

    define_attachment_memory_bucket(&client).await;
    assert_eq!(
        database
            .attachment_bucket_status()
            .await
            .expect("defined bucket should be inspectable"),
        AttachmentBucketStatus::Ready
    );

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
        AttachmentBucketStatus::Missing,
        "bucket definitions must remain isolated per disposable database"
    );
}
