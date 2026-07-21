use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use surrealdb::types::SurrealValue;
use surrealdb::{engine::any::Any, opt::capabilities::Capabilities, opt::Config, Surreal};
use surrealkit::schema_state::CatalogSnapshot;
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

static CORE_DOMAIN_SCHEMA: &[EmbeddedSchemaFile] = &[
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
        path: "database/schema/business/attachment.surql",
        sql: include_str!("../../database/schema/business/attachment.surql"),
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/customer.surql",
        sql: include_str!("../../database/schema/business/customer.surql"),
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/intervention.surql",
        sql: include_str!("../../database/schema/business/intervention.surql"),
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/intervention_line.surql",
        sql: include_str!("../../database/schema/business/intervention_line.surql"),
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/invoice.surql",
        sql: include_str!("../../database/schema/business/invoice.surql"),
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/invoice_line.surql",
        sql: include_str!("../../database/schema/business/invoice_line.surql"),
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/payment.surql",
        sql: include_str!("../../database/schema/business/payment.surql"),
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/technical_note.surql",
        sql: include_str!("../../database/schema/business/technical_note.surql"),
    },
    EmbeddedSchemaFile {
        path: "database/schema/business/vehicle.surql",
        sql: include_str!("../../database/schema/business/vehicle.surql"),
    },
];

const BUSINESS_SCHEMA_SQL: &str = concat!(
    include_str!("../../database/schema/business/attachment.surql"),
    "\n",
    include_str!("../../database/schema/business/customer.surql"),
    "\n",
    include_str!("../../database/schema/business/intervention.surql"),
    "\n",
    include_str!("../../database/schema/business/intervention_line.surql"),
    "\n",
    include_str!("../../database/schema/business/invoice.surql"),
    "\n",
    include_str!("../../database/schema/business/invoice_line.surql"),
    "\n",
    include_str!("../../database/schema/business/payment.surql"),
    "\n",
    include_str!("../../database/schema/business/technical_note.surql"),
    "\n",
    include_str!("../../database/schema/business/vehicle.surql"),
);

const BUSINESS_TABLES: [&str; 9] = [
    "attachment",
    "customer",
    "intervention",
    "intervention_line",
    "invoice",
    "invoice_line",
    "payment",
    "technical_note",
    "vehicle",
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

async fn authentication_counts(database: &Surreal<Any>) -> Value {
    let mut response = database
        .query(
            "RETURN [count(SELECT * FROM user), count(SELECT * FROM auth_session), \
             count(SELECT * FROM login_throttle)];",
        )
        .await
        .expect("authentication counts should execute");
    json_value(
        response
            .take(0)
            .expect("authentication counts should deserialize"),
    )
}

async fn application_catalog(database: &Surreal<Any>) -> Value {
    let mut sql = String::from("INFO FOR DB;");
    for table in AUTHENTICATION_SCHEMA
        .iter()
        .map(|file| file.path.rsplit('/').next().expect("schema file name"))
        .map(|file| file.trim_end_matches(".surql"))
        .chain(BUSINESS_TABLES)
    {
        sql.push_str(&format!(" INFO FOR TABLE {table};"));
    }
    let mut response = database
        .query(sql)
        .await
        .expect("application catalog should execute");
    let mut database_info = json_value(
        response
            .take(0)
            .expect("database catalog should deserialize"),
    );
    if let Some(tables) = database_info["tables"].as_object_mut() {
        tables.remove("__entity");
        tables.remove("__rollout");
    }

    let mut table_info = serde_json::Map::new();
    for (offset, table) in ["user", "auth_session", "login_throttle"]
        .into_iter()
        .chain(BUSINESS_TABLES)
        .enumerate()
    {
        table_info.insert(
            table.to_owned(),
            json_value(
                response
                    .take(offset + 1)
                    .expect("table catalog should deserialize"),
            ),
        );
    }
    json!({"database": database_info, "tables": table_info})
}

async fn business_tables(database: &Surreal<Any>) -> Vec<String> {
    let mut response = database
        .query("INFO FOR DB;")
        .await
        .expect("database table inspection should execute");
    let info = json_value(response.take(0).expect("database info should deserialize"));
    BUSINESS_TABLES
        .iter()
        .filter(|table| info["tables"].get(**table).is_some())
        .map(|table| (*table).to_owned())
        .collect()
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

fn core_domain_rollout(id: &str) -> Rollout<'static> {
    let catalog: CatalogSnapshot = serde_json::from_str(include_str!(
        "../../database/snapshots/catalog_snapshot.json"
    ))
    .expect("committed core-domain catalog snapshot should be valid JSON");
    let rollback_entities = catalog
        .entities
        .into_iter()
        .filter(|entity| entity.source_path.starts_with("database/schema/business/"))
        .map(|entity| EntityKey {
            kind: entity.kind,
            scope: entity.scope,
            name: entity.name,
        })
        .collect();
    let spec = RolloutSpec::builder(id)
        .name("Initial core domain")
        .step(RolloutStep::apply_schema(
            "apply_expand_schema",
            RolloutPhase::Start,
            BUSINESS_SCHEMA_SQL,
        ))
        .step(RolloutStep::remove_entities(
            "rollback_expand_schema",
            RolloutPhase::Rollback,
            rollback_entities,
        ))
        .build();
    Rollout::new(spec, CORE_DOMAIN_SCHEMA)
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
async fn core_domain_rollout_clean_and_existing_databases_reach_the_same_catalog() {
    let clean = migration_database("core_domain_clean").await;
    Sync::embedded(CORE_DOMAIN_SCHEMA)
        .run(&clean)
        .await
        .expect("clean desired schema should synchronize");
    let desired_catalog = application_catalog(&clean).await;

    let existing = migration_database("core_domain_existing").await;
    apply_authentication_schema(&existing).await;
    seed_authentication_fixtures(&existing).await;
    let before_fingerprint = fixture_fingerprint(&existing).await;
    let before_counts = authentication_counts(&existing).await;

    let rollout = core_domain_rollout("vin_44_existing_database");
    rollout
        .start(&existing)
        .await
        .expect("core-domain rollout should start");
    assert_eq!(fixture_fingerprint(&existing).await, before_fingerprint);
    assert_eq!(authentication_counts(&existing).await, before_counts);
    rollout
        .complete(&existing)
        .await
        .expect("additive rollout should complete without a contract step");
    assert_eq!(fixture_fingerprint(&existing).await, before_fingerprint);
    assert_eq!(authentication_counts(&existing).await, before_counts);
    assert_eq!(application_catalog(&existing).await, desired_catalog);
}

#[tokio::test]
async fn core_domain_rollout_reapplication_is_stable() {
    let database = migration_database("core_domain_reapplication").await;
    apply_authentication_schema(&database).await;
    let rollout = core_domain_rollout("vin_44_reapplication");

    rollout
        .start(&database)
        .await
        .expect("first rollout start should succeed");
    let first_status = rollout
        .status(&database)
        .await
        .expect("rollout status should load")
        .expect("rollout record should exist");
    assert_eq!(
        first_status.status,
        Some(surrealkit::RolloutStatus::ReadyToComplete)
    );
    assert_eq!(first_status.steps.len(), 1);
    assert_eq!(first_status.steps[0].status, "completed");

    let pending_error = rollout
        .start(&database)
        .await
        .expect_err("an active rollout must reject a second start");
    assert!(pending_error.to_string().contains("active"));
    let repeated_status = rollout
        .status(&database)
        .await
        .expect("repeated rollout status should load")
        .expect("rollout record should remain present");
    assert_eq!(repeated_status.steps, first_status.steps);

    rollout
        .complete(&database)
        .await
        .expect("rollout should complete");
    let error = rollout
        .start(&database)
        .await
        .expect_err("a completed rollout must not reapply");
    assert!(error.to_string().contains("already completed"));
    assert_eq!(
        rollout
            .status(&database)
            .await
            .expect("completed status should load")
            .expect("completed rollout should remain recorded")
            .status,
        Some(surrealkit::RolloutStatus::Completed)
    );
}

#[tokio::test]
async fn core_domain_rollout_rollback_removes_only_business_schema_and_metadata() {
    let database = migration_database("core_domain_rollback").await;
    apply_authentication_schema(&database).await;
    seed_authentication_fixtures(&database).await;
    let before_fingerprint = fixture_fingerprint(&database).await;
    let before_counts = authentication_counts(&database).await;
    let before_catalog = authentication_catalog(&database).await;
    let rollout = core_domain_rollout("vin_44_rollback");

    rollout
        .start(&database)
        .await
        .expect("disposable rollout should start");
    assert_eq!(fixture_fingerprint(&database).await, before_fingerprint);
    rollout
        .rollback(&database)
        .await
        .expect("disposable rollout should roll back");

    assert_eq!(fixture_fingerprint(&database).await, before_fingerprint);
    assert_eq!(authentication_counts(&database).await, before_counts);
    assert_eq!(authentication_catalog(&database).await, before_catalog);
    assert!(business_tables(&database).await.is_empty());
    assert_eq!(
        rollout
            .status(&database)
            .await
            .expect("rolled-back status should load")
            .expect("rolled-back rollout should remain recorded")
            .status,
        Some(surrealkit::RolloutStatus::RolledBack)
    );
}

#[tokio::test]
async fn core_domain_rollout_drift_blocks_before_business_changes() {
    let database = migration_database("core_domain_drift").await;
    apply_authentication_schema(&database).await;
    seed_authentication_fixtures(&database).await;
    database
        .query("REMOVE INDEX auth_session_expires_at ON auth_session;")
        .await
        .expect("drift setup should execute")
        .check()
        .expect("drift setup should remove the authentication index");
    let before_fingerprint = fixture_fingerprint(&database).await;
    let actual = authentication_catalog(&database).await;
    let expected: Value = serde_json::from_str(include_str!(
        "../../database/tests/fixtures/authentication_catalog.json"
    ))
    .expect("committed authentication catalog should be valid JSON");

    let error = validate_authentication_catalog(&actual, &expected)
        .expect_err("authentication drift must block rollout start");
    assert_eq!(
        error,
        CatalogValidationError::TableDrift {
            table: "auth_session".to_owned()
        }
    );
    assert_eq!(fixture_fingerprint(&database).await, before_fingerprint);
    assert!(business_tables(&database).await.is_empty());
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("auth_session"));
    assert!(!diagnostic.contains("active-session-digest"));
}

#[tokio::test]
async fn core_domain_rollout_representative_queries_pass() {
    let database = migration_database("core_domain_queries").await;
    apply_authentication_schema(&database).await;
    seed_authentication_fixtures(&database).await;
    core_domain_rollout("vin_44_queries")
        .start(&database)
        .await
        .expect("query verification rollout should start");

    database
        .query(
            r#"
            CREATE customer:filippo CONTENT {
                display_name: 'Filippo Customer',
                display_name_normalized: 'filippo customer'
            };
            CREATE vehicle:golf CONTENT {
                customer: customer:filippo,
                make: 'Volkswagen',
                make_normalized: 'volkswagen',
                model: 'Golf',
                model_normalized: 'golf',
                registration: '1-ABC-123',
                registration_normalized: '1ABC123'
            };
            CREATE intervention:older CONTENT {
                vehicle: vehicle:golf,
                service_date: d'2026-06-01T08:30:00Z',
                estimated_duration_minutes: 60,
                customer_snapshot_id: customer:filippo,
                customer_snapshot_name: 'Filippo Customer',
                vehicle_snapshot_registration: '1-ABC-123',
                vehicle_snapshot_make: 'Volkswagen',
                vehicle_snapshot_model: 'Golf',
                mileage: 120000
            };
            CREATE intervention:newer CONTENT {
                vehicle: vehicle:golf,
                service_date: d'2026-07-01T13:00:00Z',
                estimated_duration_minutes: 120,
                customer_snapshot_id: customer:filippo,
                customer_snapshot_name: 'Filippo Customer',
                vehicle_snapshot_registration: '1-ABC-123',
                vehicle_snapshot_make: 'Volkswagen',
                vehicle_snapshot_model: 'Golf',
                mileage: 121000,
                performed_work: 'Water pump replacement'
            };
            CREATE intervention_line:labour CONTENT {
                intervention: intervention:newer,
                category: 'labour',
                description: 'Water pump labour',
                quantity: 2dec,
                unit_label: 'hour',
                currency: 'EUR',
                unit_price_minor: 5000,
                total_price_minor: 10000,
                position: 0
            };
            CREATE technical_note:water_pump CONTENT {
                title: 'Water pump replacement',
                body: 'Use the locking tool before removing the pulley.',
                tags: ['cooling'],
                make: 'Volkswagen',
                make_normalized: 'volkswagen'
            };
            CREATE invoice:repair CONTENT {
                customer: customer:filippo,
                vehicle: vehicle:golf,
                intervention: intervention:newer
            };
            CREATE invoice_line:repair CONTENT {
                invoice: invoice:repair,
                description: 'Water pump replacement',
                quantity: 1dec,
                unit_label: 'job',
                currency: 'EUR',
                unit_price_minor: 10000,
                line_total_minor: 10000,
                position: 0
            };
            UPDATE invoice:repair SET
                status = 'issued',
                issue_number = '2026-00444',
                issue_date = d'2026-07-19',
                customer_display_snapshot = 'Filippo Customer',
                subtotal_minor = 10000,
                total_minor = 10000,
                issued_at = time::now();
            CREATE payment:deposit CONTENT {
                invoice: invoice:repair,
                amount_minor: 4000,
                currency: 'EUR',
                received_at: d'2026-07-19T13:00:00Z',
                method: 'cash',
                created_by: user:active
            };
            "#,
        )
        .await
        .expect("representative fixtures should execute")
        .check()
        .expect("representative fixtures should satisfy the rollout schema");

    let mut response = database
        .query(
            r#"
            RETURN count(SELECT * FROM vehicle WHERE customer = customer:filippo);
            RETURN (SELECT VALUE id FROM intervention WHERE vehicle = vehicle:golf
                ORDER BY service_date DESC, created_at DESC, id DESC);
            RETURN count(SELECT * FROM technical_note
                WHERE (title @@ 'water' OR body @@ 'water')
                    AND tags CONTAINS 'cooling'
                    AND make_normalized = 'volkswagen');
            RETURN invoice:repair.total_minor;
            RETURN invoice:repair.total_minor - math::sum((SELECT VALUE amount_minor
                FROM payment WHERE invoice = invoice:repair));
            RETURN sequence::nextval('invoice_issue_number');
            RETURN sequence::nextval('invoice_issue_number');
            "#,
        )
        .await
        .expect("representative queries should execute");
    assert_eq!(
        json_value(response.take(0).expect("customer navigation result")),
        json!(1)
    );
    let history = json_value(response.take(1).expect("service history result"));
    assert_eq!(history.as_array().expect("service history array").len(), 2);
    assert!(history.to_string().find("newer") < history.to_string().find("older"));
    assert_eq!(
        json_value(response.take(2).expect("technical search result")),
        json!(1)
    );
    assert_eq!(
        json_value(response.take(3).expect("invoice total result")),
        json!(10000)
    );
    assert_eq!(
        json_value(response.take(4).expect("outstanding total result")),
        json!(6000)
    );
    assert_eq!(
        json_value(response.take(5).expect("first sequence result")),
        json!(1)
    );
    assert_eq!(
        json_value(response.take(6).expect("second sequence result")),
        json!(2)
    );
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
