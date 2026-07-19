use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use surrealdb::types::SurrealValue;
use surrealdb::{engine::any::Any, opt::capabilities::Capabilities, opt::Config, Surreal};
use surrealkit::{
    EmbeddedSchemaFile, EntityKey, EntityKind, Rollout, RolloutPhase, RolloutSpec, RolloutStep,
    Sync,
};

use pipauto::database::migrations::{validate_authentication_catalog, CatalogValidationError};

static AUTHENTICATION_SCHEMA: &[EmbeddedSchemaFile] = &[
    EmbeddedSchemaFile {
        path: "database/schema/authentication/user.surql",
        sql: include_str!("../../database/schema/authentication/user.surql"),
    },
    EmbeddedSchemaFile {
        path: "database/schema/authentication/auth_session.surql",
        sql: include_str!("../../database/schema/authentication/auth_session.surql"),
    },
    EmbeddedSchemaFile {
        path: "database/schema/authentication/login_throttle.surql",
        sql: include_str!("../../database/schema/authentication/login_throttle.surql"),
    },
];

static AUTHENTICATION_WITH_VEHICLE: &[EmbeddedSchemaFile] = &[
    EmbeddedSchemaFile {
        path: "database/schema/authentication/user.surql",
        sql: include_str!("../../database/schema/authentication/user.surql"),
    },
    EmbeddedSchemaFile {
        path: "database/schema/authentication/auth_session.surql",
        sql: include_str!("../../database/schema/authentication/auth_session.surql"),
    },
    EmbeddedSchemaFile {
        path: "database/schema/authentication/login_throttle.surql",
        sql: include_str!("../../database/schema/authentication/login_throttle.surql"),
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/vehicle.surql",
        sql: "DEFINE TABLE vehicle SCHEMAFULL;",
    },
];

async fn migration_database(name: &str) -> Surreal<Any> {
    let config = Config::new().capabilities(Capabilities::all());
    let database = surrealdb::engine::any::connect(("mem://", config))
        .await
        .expect("isolated migration database should connect");
    database
        .use_ns("pipauto_migration_tests")
        .use_db(name)
        .await
        .expect("isolated migration database should be selected");
    database
}

async fn apply_authentication_schema(database: &Surreal<Any>) {
    Sync::embedded(AUTHENTICATION_SCHEMA)
        .run(database)
        .await
        .expect("authentication schema should synchronize");
}

async fn seed_authentication_fixtures(database: &Surreal<Any>) {
    database
        .query(
            r#"
            CREATE user:active CONTENT {
                email: 'Active@Example.com',
                email_normalized: 'active@example.com',
                display_name: 'Active Mechanic',
                password_hash: '$argon2id$fixture',
                active: true
            };
            CREATE user:inactive CONTENT {
                email: 'inactive@example.com',
                email_normalized: 'inactive@example.com',
                display_name: 'Inactive Mechanic',
                password_hash: '$argon2id$fixture',
                active: false
            };
            CREATE auth_session:active CONTENT {
                user: user:active,
                jti_digest: 'active-session-digest',
                issued_at: time::now() - 1h,
                expires_at: time::now() + 1h
            };
            CREATE auth_session:revoked CONTENT {
                user: user:active,
                jti_digest: 'revoked-session-digest',
                issued_at: time::now() - 2h,
                expires_at: time::now() + 1h,
                revoked_at: time::now() - 30m
            };
            CREATE auth_session:expired CONTENT {
                user: user:inactive,
                jti_digest: 'expired-session-digest',
                issued_at: time::now() - 3h,
                expires_at: time::now() - 1h
            };
            CREATE login_throttle:active CONTENT {
                identifier_digest: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                network_digest: 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                failed_attempts: 2,
                window_started_at: time::now() - 5m,
                blocked_until: time::now() + 5m
            };
            "#,
        )
        .await
        .expect("authentication fixtures should execute")
        .check()
        .expect("authentication fixtures should satisfy the schema");
}

async fn fixture_fingerprint(database: &Surreal<Any>) -> String {
    let mut response = database
        .query(
            "SELECT * FROM user ORDER BY id;\
             SELECT * FROM auth_session ORDER BY id;\
             SELECT * FROM login_throttle ORDER BY id;",
        )
        .await
        .expect("fixture fingerprint queries should execute");
    let mut hasher = Sha256::new();
    for statement in 0..3 {
        let value: surrealdb::types::Value = response
            .take(statement)
            .expect("fixture rows should deserialize");
        let encoded = serde_json::to_vec(&json_value(value))
            .expect("fixture rows should have a stable JSON representation");
        hasher.update(encoded);
    }
    format!("{:x}", hasher.finalize())
}

async fn authentication_catalog(database: &Surreal<Any>) -> Value {
    let mut response = database
        .query(
            "INFO FOR DB; INFO FOR TABLE auth_session; \
             INFO FOR TABLE login_throttle; INFO FOR TABLE user;",
        )
        .await
        .expect("catalog inspection should execute");
    let database_info = json_value(
        response
            .take(0)
            .expect("database metadata should deserialize"),
    );
    let mut catalog = serde_json::Map::new();
    for (offset, table) in ["auth_session", "login_throttle", "user"]
        .into_iter()
        .enumerate()
    {
        let table_info = json_value(
            response
                .take(offset + 1)
                .expect("table metadata should deserialize"),
        );
        catalog.insert(
            table.to_owned(),
            json!({
                "table": database_info["tables"][table],
                "fields": table_info["fields"],
                "indexes": table_info["indexes"],
            }),
        );
    }
    Value::Object(catalog)
}

fn json_value(value: surrealdb::types::Value) -> Value {
    Value::from_value(value).expect("SurrealDB metadata should convert to JSON")
}

fn vehicle_rollout(id: &str) -> Rollout<'static> {
    let spec = RolloutSpec::builder(id)
        .name("Add vehicle table for migration verification")
        .step(RolloutStep::apply_schema(
            "add_vehicle",
            RolloutPhase::Start,
            "DEFINE TABLE vehicle SCHEMAFULL;",
        ))
        .step(RolloutStep::remove_entities(
            "rollback_vehicle",
            RolloutPhase::Rollback,
            vec![EntityKey {
                kind: EntityKind::Table,
                scope: None,
                name: "vehicle".to_owned(),
            }],
        ))
        .build();
    Rollout::new(spec, AUTHENTICATION_WITH_VEHICLE)
}

#[tokio::test]
async fn migration_clean_sync_is_repeatable_and_matches_the_catalog() {
    let database = migration_database("clean_sync").await;

    apply_authentication_schema(&database).await;
    apply_authentication_schema(&database).await;

    let actual = authentication_catalog(&database).await;
    let expected: Value = serde_json::from_str(include_str!(
        "../../database/tests/fixtures/authentication_catalog.json"
    ))
    .expect("committed catalog fixture should be valid JSON");
    validate_authentication_catalog(&actual, &expected)
        .expect("repeated synchronization should match the authentication catalog");
}

#[tokio::test]
async fn migration_existing_database_inspection_and_sync_preserve_authentication_records() {
    let database = migration_database("existing_database").await;
    apply_authentication_schema(&database).await;
    seed_authentication_fixtures(&database).await;
    let before = fixture_fingerprint(&database).await;

    Sync::embedded(AUTHENTICATION_SCHEMA)
        .dry_run(true)
        .run(&database)
        .await
        .expect("baseline inspection should succeed without mutation");
    assert_eq!(fixture_fingerprint(&database).await, before);

    apply_authentication_schema(&database).await;
    assert_eq!(fixture_fingerprint(&database).await, before);
}

#[tokio::test]
async fn migration_catalog_drift_is_detected_before_records_are_mutated() {
    let database = migration_database("drifted_database").await;
    apply_authentication_schema(&database).await;
    seed_authentication_fixtures(&database).await;
    database
        .query("REMOVE INDEX auth_session_expires_at ON auth_session;")
        .await
        .expect("drift setup should execute")
        .check()
        .expect("drift setup should remove the index");
    let before = fixture_fingerprint(&database).await;

    let actual = authentication_catalog(&database).await;
    let expected: Value = serde_json::from_str(include_str!(
        "../../database/tests/fixtures/authentication_catalog.json"
    ))
    .expect("committed catalog fixture should be valid JSON");
    assert_eq!(
        validate_authentication_catalog(&actual, &expected),
        Err(CatalogValidationError::TableDrift {
            table: "auth_session".to_owned()
        })
    );
    assert_eq!(fixture_fingerprint(&database).await, before);
}

#[tokio::test]
async fn migration_pending_rollout_and_rollback_preserve_authentication_fixtures() {
    let database = migration_database("pending_rollout").await;
    apply_authentication_schema(&database).await;
    seed_authentication_fixtures(&database).await;
    let before = fixture_fingerprint(&database).await;

    let rollout = vehicle_rollout("vin_38_vehicle_rollout");
    rollout
        .start(&database)
        .await
        .expect("pending rollout should start");
    assert_eq!(
        rollout
            .status(&database)
            .await
            .expect("rollout status should load")
            .expect("rollout record should exist")
            .status,
        Some(surrealkit::RolloutStatus::ReadyToComplete)
    );
    let actual = authentication_catalog(&database).await;
    let expected: Value = serde_json::from_str(include_str!(
        "../../database/tests/fixtures/authentication_catalog.json"
    ))
    .expect("committed catalog fixture should be valid JSON");
    validate_authentication_catalog(&actual, &expected)
        .expect("business rollout should preserve the authentication catalog");
    assert_eq!(fixture_fingerprint(&database).await, before);

    rollout
        .rollback(&database)
        .await
        .expect("pending rollout should roll back");
    assert_eq!(fixture_fingerprint(&database).await, before);
}

#[tokio::test]
async fn migration_concurrent_rollout_starts_cannot_both_proceed() {
    let database = migration_database("concurrent_rollouts").await;
    apply_authentication_schema(&database).await;

    let first = vehicle_rollout("vin_38_first_rollout");
    let second = Rollout::new(
        RolloutSpec::builder("vin_38_second_rollout")
            .step(RolloutStep::apply_schema(
                "add_customer",
                RolloutPhase::Start,
                "DEFINE TABLE customer SCHEMAFULL;",
            ))
            .build(),
        AUTHENTICATION_SCHEMA,
    );

    let (first_result, second_result) =
        tokio::join!(first.start(&database), second.start(&database));
    assert!(
        !(first_result.is_ok() && second_result.is_ok()),
        "concurrent rollout starts must not both proceed"
    );
    let rejection = first_result
        .err()
        .or_else(|| second_result.err())
        .expect("one start fails");
    let rejection = rejection.to_string();
    assert!(
        rejection.contains("active") || rejection.contains("conflict"),
        "rejection should identify an active rollout without exposing data: {rejection}"
    );
}

#[tokio::test]
async fn migration_failure_names_the_rollout_and_phase_without_fixture_data() {
    let database = migration_database("failed_rollout").await;
    apply_authentication_schema(&database).await;
    seed_authentication_fixtures(&database).await;
    let rollout_id = "vin_38_safe_failure";
    let rollout = Rollout::new(
        RolloutSpec::builder(rollout_id)
            .step(RolloutStep::assert_sql(
                "verify_start_boundary",
                RolloutPhase::Start,
                "RETURN false;",
                "true",
            ))
            .build(),
        AUTHENTICATION_SCHEMA,
    );

    let error = rollout
        .start(&database)
        .await
        .expect_err("rollout assertion should fail");
    let report = rollout
        .status(&database)
        .await
        .expect("failed status should load")
        .expect("failed rollout should have a record");
    let diagnostic = format!(
        "rollout={rollout_id} phase={} status={}",
        report.steps[0].phase,
        report.status.expect("status should be recognized").as_str()
    );

    assert!(error.to_string().contains("verify_start_boundary"));
    assert_eq!(
        diagnostic,
        "rollout=vin_38_safe_failure phase=start status=failed"
    );
    assert!(!diagnostic.contains("Active@Example.com"));
    assert!(!diagnostic.contains("active-session-digest"));
}
