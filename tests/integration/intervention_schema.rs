use serde_json::{json, Value};
use surrealdb::types::SurrealValue;
use surrealdb::{engine::any::Any, opt::capabilities::Capabilities, opt::Config, Surreal};
use surrealkit::{EmbeddedSchemaFile, Rollout, RolloutAction, RolloutSpec, RolloutStatus, Sync};

const SUPPORT_SCHEMA: &str = r#"
DEFINE TABLE customer SCHEMAFULL;
DEFINE FIELD display_name ON customer TYPE string;
DEFINE FIELD display_name_normalized ON customer TYPE string;

DEFINE TABLE vehicle SCHEMAFULL;
DEFINE FIELD customer ON vehicle TYPE record<customer>;
DEFINE FIELD make ON vehicle TYPE string;
DEFINE FIELD make_normalized ON vehicle TYPE string;
DEFINE FIELD model ON vehicle TYPE string;
DEFINE FIELD model_normalized ON vehicle TYPE string;
"#;

const DATE_ONLY_INTERVENTION_SCHEMA: &str = r#"
DEFINE TABLE intervention SCHEMAFULL;
DEFINE FIELD vehicle ON intervention TYPE record<vehicle>;
DEFINE FIELD service_date ON intervention TYPE datetime;
DEFINE FIELD status ON intervention TYPE string DEFAULT 'draft';
DEFINE FIELD created_at ON intervention TYPE datetime DEFAULT time::now() READONLY;
DEFINE FIELD updated_at ON intervention TYPE datetime DEFAULT ALWAYS time::now();
DEFINE INDEX intervention_vehicle_service_history ON intervention
    FIELDS vehicle, service_date, created_at;
DEFINE INDEX intervention_recent_work ON intervention FIELDS service_date, created_at;
DEFINE EVENT intervention_reject_terminal_update ON TABLE intervention
    WHEN $event = 'UPDATE' AND $before.status IN ['completed', 'cancelled']
    THEN { THROW 'completed and cancelled interventions are immutable'; };
"#;

static DATE_ONLY_SCHEMA: &[EmbeddedSchemaFile] = &[
    EmbeddedSchemaFile {
        path: "database/schema/business/support.surql",
        sql: SUPPORT_SCHEMA,
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/intervention.surql",
        sql: DATE_ONLY_INTERVENTION_SCHEMA,
    },
];

static SCHEDULED_SCHEMA: &[EmbeddedSchemaFile] = &[
    EmbeddedSchemaFile {
        path: "database/schema/business/support.surql",
        sql: SUPPORT_SCHEMA,
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/intervention.surql",
        sql: include_str!("../../database/schema/business/intervention.surql"),
    },
];

async fn database(name: &str) -> Surreal<Any> {
    let config = Config::new().capabilities(Capabilities::all());
    let database = surrealdb::engine::any::connect(("mem://", config))
        .await
        .expect("isolated intervention schema database should connect");
    database
        .use_ns("pipauto_intervention_schema_tests")
        .use_db(name)
        .await
        .expect("isolated intervention schema database should be selected");
    database
}

fn intervention_rollout(id: &str) -> Rollout<'static> {
    let mut spec: RolloutSpec = toml::from_str(include_str!(
        "../../database/rollouts/20260721133849__mandatory_intervention_scheduling_snapshots.toml"
    ))
    .expect("mandatory intervention rollout manifest should deserialize");
    spec.id = id.to_owned();
    spec.source_schema_hash.clear();
    spec.target_schema_hash.clear();
    Rollout::new(spec, SCHEDULED_SCHEMA)
}

async fn catalog(database: &Surreal<Any>) -> Value {
    let mut response = database
        .query("INFO FOR DB; INFO FOR TABLE customer; INFO FOR TABLE intervention; INFO FOR TABLE vehicle;")
        .await
        .expect("intervention catalog inspection should execute");
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
    for (offset, table) in ["customer", "intervention", "vehicle"]
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

async fn seed_relationships(database: &Surreal<Any>) {
    database
        .query(
            "CREATE customer:owner CONTENT { \
                 display_name: 'Mario Rossi', display_name_normalized: 'mario rossi' \
             }; \
             CREATE customer:other CONTENT { \
                 display_name: 'Other Owner', display_name_normalized: 'other owner' \
             }; \
             CREATE vehicle:golf CONTENT { \
                 customer: customer:owner, make: 'Volkswagen', \
                 make_normalized: 'volkswagen', model: 'Golf', model_normalized: 'golf' \
             };",
        )
        .await
        .expect("relationship fixtures should execute")
        .check()
        .expect("relationship fixtures should satisfy the schema");
}

#[tokio::test]
async fn intervention_schema_requires_complete_schedule_and_immutable_snapshots() {
    let database = database("boundary").await;
    Sync::embedded(SCHEDULED_SCHEMA)
        .run(&database)
        .await
        .expect("scheduled intervention schema should synchronize");
    seed_relationships(&database).await;

    let missing = database
        .query(
            "CREATE intervention:missing CONTENT { \
                 vehicle: vehicle:golf, service_date: d'2026-07-22T07:30:00Z' \
             };",
        )
        .await
        .expect("missing-field query should execute");
    assert!(missing.check().is_err());

    for duration in [0, 45, 1470] {
        let invalid = database
            .query(format!(
                "CREATE intervention:duration_{duration} CONTENT {{ \
                     vehicle: vehicle:golf, service_date: d'2026-07-22T07:30:00Z', \
                     estimated_duration_minutes: {duration}, \
                     customer_snapshot_id: customer:owner, \
                     customer_snapshot_name: 'Mario Rossi', \
                     vehicle_snapshot_make: 'Volkswagen', vehicle_snapshot_model: 'Golf' \
                 }};"
            ))
            .await
            .expect("invalid-duration query should execute");
        assert!(invalid.check().is_err());
    }

    database
        .query(
            "CREATE intervention:valid CONTENT { \
                 vehicle: vehicle:golf, service_date: d'2026-07-22T07:30:00Z', \
                 estimated_duration_minutes: 120, customer_snapshot_id: customer:owner, \
                 customer_snapshot_name: 'Mario Rossi', vehicle_snapshot_registration: NONE, \
                 vehicle_snapshot_make: 'Volkswagen', vehicle_snapshot_model: 'Golf' \
             }; \
             UPDATE intervention:valid SET service_date = d'2026-07-22T08:00:00Z', \
                 estimated_duration_minutes = 180;",
        )
        .await
        .expect("valid scheduling query should execute")
        .check()
        .expect("draft scheduling should remain mutable");

    for rewrite in [
        "customer_snapshot_id = customer:other",
        "customer_snapshot_name = 'Other Owner'",
        "vehicle_snapshot_registration = '1-ABC-234'",
        "vehicle_snapshot_make = 'Peugeot'",
        "vehicle_snapshot_model = '208'",
    ] {
        let rewrite = database
            .query(format!("UPDATE intervention:valid SET {rewrite};"))
            .await
            .expect("snapshot rewrite query should execute");
        assert!(rewrite.check().is_err());
    }
}

#[tokio::test]
async fn intervention_rollout_preflight_rejects_rows_without_deleting_them() {
    let database = database("preflight").await;
    Sync::embedded(DATE_ONLY_SCHEMA)
        .run(&database)
        .await
        .expect("date-only intervention schema should synchronize");
    seed_relationships(&database).await;
    database
        .query(
            "CREATE intervention:legacy CONTENT { \
                 vehicle: vehicle:golf, service_date: d'2026-07-22T00:00:00Z' \
             };",
        )
        .await
        .expect("legacy intervention fixture should execute")
        .check()
        .expect("legacy intervention fixture should satisfy the date-only schema");

    let error = intervention_rollout("vin_73_non_empty")
        .start(&database)
        .await
        .expect_err("non-empty intervention table must block rollout start");
    assert!(error
        .to_string()
        .contains("preflight_empty_interventions_or_reset_disposable_database"));

    let mut response = database
        .query("RETURN count(SELECT * FROM intervention);")
        .await
        .expect("preservation count should execute");
    let count = Value::from_value(
        response
            .take::<surrealdb::types::Value>(0)
            .expect("preservation count should decode"),
    )
    .expect("preservation count should convert to JSON");
    assert_eq!(count, json!(1));
}

#[tokio::test]
async fn intervention_rollout_clean_database_converges_with_desired_schema() {
    let clean = database("clean_desired").await;
    Sync::embedded(SCHEDULED_SCHEMA)
        .run(&clean)
        .await
        .expect("clean desired schema should synchronize");

    let existing = database("empty_date_only").await;
    Sync::embedded(DATE_ONLY_SCHEMA)
        .run(&existing)
        .await
        .expect("empty date-only schema should synchronize");
    let rollout = intervention_rollout("vin_73_clean");
    rollout
        .start(&existing)
        .await
        .expect("empty intervention rollout should start");
    assert_eq!(
        rollout
            .status(&existing)
            .await
            .expect("rollout status should load")
            .expect("rollout record should exist")
            .status,
        Some(RolloutStatus::ReadyToComplete)
    );
    rollout
        .complete(&existing)
        .await
        .expect("empty intervention rollout should complete");
    assert_eq!(catalog(&existing).await, catalog(&clean).await);
}

#[test]
fn intervention_rollout_contains_no_data_deletion_step() {
    let spec: RolloutSpec = toml::from_str(include_str!(
        "../../database/rollouts/20260721133849__mandatory_intervention_scheduling_snapshots.toml"
    ))
    .expect("mandatory intervention rollout manifest should deserialize");
    assert!(spec
        .steps
        .iter()
        .all(|step| !matches!(step.action, RolloutAction::RunSql { .. })));
}

#[test]
fn startup_health_and_readiness_paths_do_not_apply_schema_or_reset_data() {
    let read_only_paths = [
        include_str!("../../src/initializers/surrealdb.rs"),
        include_str!("../../src/controllers/setup.rs"),
        include_str!("../../src/controllers/surrealdb_health.rs"),
        include_str!("../../src/services/health.rs"),
    ];
    for source in read_only_paths {
        for forbidden in [
            "surrealkit",
            "DEFINE TABLE",
            "REMOVE TABLE",
            "DELETE ",
            "reset",
        ] {
            assert!(
                !source.contains(forbidden),
                "runtime readiness path contains forbidden mutation marker {forbidden}"
            );
        }
    }

    let business = include_str!("../../src/initializers/business.rs");
    assert!(business.contains("if ctx.environment == Environment::Test"));
    let smoke = include_str!("../../scripts/browser-smoke-server");
    assert!(
        smoke.find("./scripts/surrealkit sync") < smoke.find("cargo loco start"),
        "disposable smoke setup must remain explicit and precede application startup"
    );
}
