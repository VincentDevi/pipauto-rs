use loco_rs::{boot::create_context, environment::Environment};
use pipauto::{
    app::App,
    database::{client::AppDatabase, settings::DatabaseSettings},
};

#[tokio::test]
async fn database_initializes_in_memory_and_is_healthy() {
    let config = Environment::Test
        .load()
        .expect("test configuration should load");
    let settings =
        DatabaseSettings::from_config(&config).expect("database settings should be valid");

    let database = AppDatabase::connect(&settings)
        .await
        .expect("in-memory database should initialize");

    database.health().await.expect("database should be healthy");
}

#[tokio::test]
async fn database_is_installed_once_in_the_application_shared_store() {
    let config = Environment::Test
        .load()
        .expect("test configuration should load");

    let context = create_context::<App>(&Environment::Test, config)
        .await
        .expect("application context should initialize");

    let database = context
        .shared_store
        .get::<AppDatabase>()
        .expect("application database should be installed");
    database
        .health()
        .await
        .expect("installed database should be healthy");
}

#[tokio::test]
async fn incomplete_database_configuration_prevents_startup() {
    let mut config = Environment::Test
        .load()
        .expect("test configuration should load");
    config
        .settings
        .as_mut()
        .and_then(|settings| settings.get_mut("surrealdb"))
        .and_then(serde_json::Value::as_object_mut)
        .expect("surrealdb settings should be an object")
        .remove("namespace");

    let error = match create_context::<App>(&Environment::Test, config).await {
        Ok(_) => panic!("startup should reject incomplete database settings"),
        Err(error) => error,
    };

    assert!(error.to_string().contains("namespace"));
    assert!(!error.to_string().contains("password"));
}

#[tokio::test]
async fn attachment_settings_reject_invalid_startup_values_without_echoing_them() {
    for invalid in [0, 99_999_999] {
        let mut config = Environment::Test
            .load()
            .expect("test configuration should load");
        config
            .settings
            .as_mut()
            .and_then(|settings| settings.get_mut("attachments"))
            .and_then(serde_json::Value::as_object_mut)
            .expect("attachment settings should be an object")
            .insert("maximum_file_bytes".to_owned(), invalid.into());

        let error = match create_context::<App>(&Environment::Test, config).await {
            Ok(_) => panic!("startup should reject an invalid attachment limit"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("settings.attachments"));
        assert!(!error.to_string().contains(&invalid.to_string()));
    }
}
