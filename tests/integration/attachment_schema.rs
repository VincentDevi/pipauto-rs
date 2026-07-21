use serde_json::{json, Value};
use surrealdb::types::SurrealValue;
use surrealdb::{
    engine::any::Any,
    opt::{capabilities::Capabilities, capabilities::ExperimentalFeature, Config},
    Surreal,
};
use surrealkit::{
    EmbeddedSchemaFile, Rollout, RolloutAction, RolloutPhase, RolloutSpec, RolloutStatus,
    RolloutStep, Sync,
};

const LEGACY_ATTACHMENT_SCHEMA: &str = r#"
DEFINE TABLE attachment SCHEMAFULL;
DEFINE FIELD vehicle ON attachment TYPE option<record<vehicle>>
    REFERENCE ON DELETE REJECT ASSERT $value IS NONE OR record::exists($value);
DEFINE FIELD intervention ON attachment TYPE option<record<intervention>>
    REFERENCE ON DELETE REJECT ASSERT $value IS NONE OR record::exists($value);
DEFINE FIELD display_name ON attachment TYPE string;
DEFINE FIELD media_type ON attachment TYPE string;
DEFINE FIELD byte_size ON attachment TYPE option<int> ASSERT $value IS NONE OR $value >= 0;
DEFINE FIELD caption ON attachment TYPE option<string>;
DEFINE FIELD storage_state ON attachment TYPE string DEFAULT 'metadata_only'
    ASSERT $value = 'metadata_only';
DEFINE FIELD created_at ON attachment TYPE datetime DEFAULT time::now() READONLY;
DEFINE FIELD updated_at ON attachment TYPE datetime DEFAULT ALWAYS time::now();
DEFINE INDEX attachment_vehicle ON attachment FIELDS vehicle;
DEFINE INDEX attachment_intervention ON attachment FIELDS intervention;
DEFINE INDEX attachment_storage_state ON attachment FIELDS storage_state;
DEFINE EVENT attachment_validate_owner ON TABLE attachment
    WHEN $event IN ['CREATE', 'UPDATE'] THEN {
        IF ($after.vehicle IS NONE) = ($after.intervention IS NONE) {
            THROW 'attachment metadata must have exactly one owner';
        };
    };
"#;

const VEHICLE_SCHEMA: &str = "DEFINE TABLE vehicle SCHEMAFULL;";
const INTERVENTION_SCHEMA: &str = "DEFINE TABLE intervention SCHEMAFULL;";
const TECHNICAL_NOTE_SCHEMA: &str = "DEFINE TABLE technical_note SCHEMAFULL;";
const MEMORY_BUCKET_SCHEMA: &str =
    "DEFINE BUCKET pipauto_attachments BACKEND 'memory' PERMISSIONS NONE;";

static PRE_STORAGE_SCHEMA: &[EmbeddedSchemaFile] = &[
    EmbeddedSchemaFile {
        path: "database/schema/business/attachment.surql",
        sql: LEGACY_ATTACHMENT_SCHEMA,
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/intervention.surql",
        sql: INTERVENTION_SCHEMA,
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/technical_note.surql",
        sql: TECHNICAL_NOTE_SCHEMA,
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/vehicle.surql",
        sql: VEHICLE_SCHEMA,
    },
];

static STORED_ATTACHMENT_SCHEMA: &[EmbeddedSchemaFile] = &[
    EmbeddedSchemaFile {
        path: "database/schema/business/attachment.surql",
        sql: include_str!("../../database/schema/business/attachment.surql"),
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/intervention.surql",
        sql: INTERVENTION_SCHEMA,
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/technical_note.surql",
        sql: TECHNICAL_NOTE_SCHEMA,
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/vehicle.surql",
        sql: VEHICLE_SCHEMA,
    },
    EmbeddedSchemaFile {
        path: "database/schema/storage/pipauto_attachments.surql",
        sql: MEMORY_BUCKET_SCHEMA,
    },
];

async fn database(name: &str) -> Surreal<Any> {
    let capabilities =
        Capabilities::all().with_experimental_feature_allowed(ExperimentalFeature::Files);
    let config = Config::new().capabilities(capabilities);
    let database = surrealdb::engine::any::connect(("mem://", config))
        .await
        .expect("isolated attachment migration database should connect");
    database
        .use_ns("pipauto_attachment_schema_tests")
        .use_db(name)
        .await
        .expect("isolated attachment migration database should be selected");
    database
}

fn attachment_rollout(id: &str) -> Rollout<'static> {
    let mut spec: RolloutSpec = toml::from_str(include_str!(
        "../../database/rollouts/20260721104500__stored_attachment_schema.toml"
    ))
    .expect("stored-attachment rollout manifest should deserialize");
    spec.id = id.to_owned();
    spec.source_schema_hash.clear();
    spec.target_schema_hash.clear();
    for step in &mut spec.steps {
        if let RolloutAction::ApplySchema { sql } = &mut step.action {
            *sql = sql.replace("file:/home/nonroot/pipauto_attachments", "memory");
        }
    }
    Rollout::new(spec, STORED_ATTACHMENT_SCHEMA)
}

async fn seed_legacy_attachment(database: &Surreal<Any>) {
    database
        .query(
            "CREATE vehicle:legacy_owner; \
             CREATE attachment:legacy CONTENT { \
                 vehicle: vehicle:legacy_owner, \
                 display_name: 'Legacy photo.jpg', \
                 media_type: 'image/jpeg', \
                 byte_size: 0 \
             };",
        )
        .await
        .expect("legacy attachment fixture should execute")
        .check()
        .expect("legacy attachment fixture should satisfy the old schema");
}

async fn count_state(database: &Surreal<Any>, state: &str) -> i64 {
    let mut response = database
        .query("RETURN count(SELECT * FROM attachment WHERE storage_state = $state);")
        .bind(("state", state.to_owned()))
        .await
        .expect("attachment state count should execute");
    let value: surrealdb::types::Value = response
        .take(0)
        .expect("attachment state count should decode");
    Value::from_value(value)
        .expect("attachment state count should convert to JSON")
        .as_i64()
        .expect("attachment state count should be an integer")
}

async fn catalog(database: &Surreal<Any>) -> Value {
    let mut response = database
        .query(
            "INFO FOR DB; INFO FOR TABLE attachment; INFO FOR TABLE intervention; \
             INFO FOR TABLE technical_note; INFO FOR TABLE vehicle;",
        )
        .await
        .expect("attachment catalog inspection should execute");
    let mut database_info = Value::from_value(
        response
            .take::<surrealdb::types::Value>(0)
            .expect("database catalog should decode"),
    )
    .expect("database catalog should convert to JSON");
    if let Some(tables) = database_info["tables"].as_object_mut() {
        tables.remove("__entity");
        tables.remove("__rollout");
    }
    let mut tables = serde_json::Map::new();
    for (offset, table) in ["attachment", "intervention", "technical_note", "vehicle"]
        .into_iter()
        .enumerate()
    {
        tables.insert(
            table.to_owned(),
            Value::from_value(
                response
                    .take::<surrealdb::types::Value>(offset + 1)
                    .expect("table catalog should decode"),
            )
            .expect("table catalog should convert to JSON"),
        );
    }
    json!({"database": database_info, "tables": tables})
}

#[tokio::test]
async fn attachment_schema_rollout_preserves_counts_then_converges_clean_and_existing_databases() {
    let clean = database("clean").await;
    Sync::embedded(STORED_ATTACHMENT_SCHEMA)
        .run(&clean)
        .await
        .expect("clean stored-attachment schema should synchronize");
    let clean_catalog = catalog(&clean).await;

    let existing = database("existing").await;
    Sync::embedded(PRE_STORAGE_SCHEMA)
        .run(&existing)
        .await
        .expect("pre-storage schema should synchronize");
    seed_legacy_attachment(&existing).await;

    let rollout = attachment_rollout("vin_65_existing_database");
    rollout
        .start(&existing)
        .await
        .expect("attachment additive phase should start");
    assert_eq!(count_state(&existing, "metadata_only").await, 1);
    assert_eq!(
        rollout
            .status(&existing)
            .await
            .expect("rollout status should load")
            .expect("rollout record should exist")
            .status,
        Some(RolloutStatus::ReadyToComplete)
    );

    existing
        .query(
            "CREATE technical_note:stored_owner; \
             CREATE attachment:pending CONTENT { \
                 technical_note: technical_note:stored_owner, \
                 display_name: 'Pending photo.png', \
                 media_type: 'image/png', \
                 file: f'pipauto_attachments:/pending-photo', \
                 storage_state: 'pending' \
             };",
        )
        .await
        .expect("compatible pending attachment should execute")
        .check()
        .expect("additive schema should accept compatible code");

    rollout
        .complete(&existing)
        .await
        .expect("attachment contract phase should complete");
    assert_eq!(count_state(&existing, "metadata_only").await, 0);
    assert_eq!(count_state(&existing, "pending").await, 1);
    assert_eq!(catalog(&existing).await, clean_catalog);
}

#[tokio::test]
async fn attachment_schema_rollout_contract_failure_is_retryable_without_early_deletion() {
    let database = database("retry_contract").await;
    Sync::embedded(PRE_STORAGE_SCHEMA)
        .run(&database)
        .await
        .expect("pre-storage schema should synchronize");
    seed_legacy_attachment(&database).await;

    let mut rollout = attachment_rollout("vin_65_retry_contract");
    let gate = RolloutStep::assert_sql(
        "verify_contract_gate",
        RolloutPhase::Complete,
        "RETURN record::exists(technical_note:contract_gate);",
        "true",
    );
    let delete_index = rollout
        .spec()
        .steps
        .iter()
        .position(|step| step.id == "delete_metadata_only_attachments")
        .expect("delete step should exist");
    let mut spec = rollout.spec().clone();
    spec.steps.insert(delete_index, gate);
    rollout = Rollout::new(spec, STORED_ATTACHMENT_SCHEMA);

    rollout
        .start(&database)
        .await
        .expect("attachment additive phase should start");
    rollout
        .complete(&database)
        .await
        .expect_err("closed contract gate should fail safely");
    assert_eq!(count_state(&database, "metadata_only").await, 1);
    assert_eq!(
        rollout
            .status(&database)
            .await
            .expect("failed rollout status should load")
            .expect("failed rollout should remain recorded")
            .status,
        Some(RolloutStatus::Failed)
    );

    database
        .query("CREATE technical_note:contract_gate;")
        .await
        .expect("contract recovery gate should execute")
        .check()
        .expect("contract recovery gate should be valid");
    rollout
        .complete(&database)
        .await
        .expect("failed contract phase should retry idempotently");
    assert_eq!(count_state(&database, "metadata_only").await, 0);
}

#[tokio::test]
async fn attachment_schema_rollout_rollback_preserves_legacy_rows() {
    let database = database("rollback").await;
    Sync::embedded(PRE_STORAGE_SCHEMA)
        .run(&database)
        .await
        .expect("pre-storage schema should synchronize");
    seed_legacy_attachment(&database).await;
    let before = catalog(&database).await;
    let rollout = attachment_rollout("vin_65_rollback");

    rollout
        .start(&database)
        .await
        .expect("attachment additive phase should start");
    rollout
        .rollback(&database)
        .await
        .expect("attachment additive phase should roll back");

    assert_eq!(count_state(&database, "metadata_only").await, 1);
    assert_eq!(catalog(&database).await, before);
}
