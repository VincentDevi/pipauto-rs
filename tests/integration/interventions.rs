use std::sync::Arc;

use chrono::{NaiveDate, Utc};
use loco_rs::testing::request::boot_test;
use pipauto::{
    app::App,
    database::client::AppDatabase,
    domain::{CurrencyCode, Money, PageLimit, Quantity},
    models::{
        customer::NewCustomer,
        intervention::{InterventionStatus, NewIntervention},
        intervention_line::{InterventionLineCategory, NewInterventionLine},
        vehicle::NewVehicle,
    },
    repositories::{
        customer::CustomerRepository,
        intervention::{InterventionFilter, InterventionRepository, LineMutation},
        surreal::{
            customer::SurrealCustomerRepository, intervention::SurrealInterventionRepository,
            vehicle::SurrealVehicleRepository,
        },
        vehicle::VehicleRepository,
        RepositoryError,
    },
};

#[tokio::test]
async fn intervention_repository_preserves_mileage_history_and_archived_readability() {
    let (customers, vehicles, interventions) = repositories().await;
    let vehicle = vehicle_fixture(&customers, &vehicles).await;

    let first = interventions
        .create(&intervention(
            &vehicle.id,
            1,
            100_000,
            Some("Initial service"),
        ))
        .await
        .expect("first intervention");
    interventions
        .create(&intervention(
            &vehicle.id,
            20,
            120_000,
            Some("Later service"),
        ))
        .await
        .expect("later intervention");
    interventions
        .create(&intervention(
            &vehicle.id,
            10,
            110_000,
            Some("Backdated service"),
        ))
        .await
        .expect("valid backdated intervention");

    assert_eq!(
        interventions
            .create(&intervention(&vehicle.id, 15, 120_001, Some("Regression")))
            .await,
        Err(RepositoryError::Conflict)
    );
    assert_eq!(
        vehicles
            .find_by_id(&vehicle.id)
            .await
            .expect("vehicle lookup")
            .expect("vehicle")
            .current_mileage,
        Some(120_000)
    );

    vehicles
        .archive(&vehicle.id)
        .await
        .expect("archive vehicle");
    assert_eq!(
        interventions
            .create(&intervention(
                &vehicle.id,
                21,
                121_000,
                Some("Archived work")
            ))
            .await,
        Err(RepositoryError::Conflict)
    );
    assert!(interventions
        .find_by_id(&first.id)
        .await
        .expect("history remains readable")
        .is_some());
}

#[tokio::test]
async fn service_history_cursor_and_atomic_line_totals_are_deterministic() {
    let (customers, vehicles, interventions) = repositories().await;
    let vehicle = vehicle_fixture(&customers, &vehicles).await;
    let older = interventions
        .create(&intervention(&vehicle.id, 19, 100_000, Some("Older")))
        .await
        .expect("older same-date intervention");
    let newer = interventions
        .create(&intervention(&vehicle.id, 19, 100_000, Some("Newer")))
        .await
        .expect("newer same-date intervention");

    let filter = InterventionFilter {
        vehicle_id: Some(vehicle.id.clone()),
        ..InterventionFilter::default()
    };
    let first_page = interventions
        .vehicle_history(
            &vehicle.id,
            &filter,
            PageLimit::new(1).expect("limit"),
            None,
        )
        .await
        .expect("first page");
    let second_page = interventions
        .vehicle_history(
            &vehicle.id,
            &filter,
            PageLimit::new(1).expect("limit"),
            first_page.next.as_ref(),
        )
        .await
        .expect("second page");
    let ids = [
        first_page.items[0].intervention.id.clone(),
        second_page.items[0].intervention.id.clone(),
    ];
    assert_ne!(ids[0], ids[1]);
    assert!(ids.contains(&older.id));
    assert!(ids.contains(&newer.id));

    let currency = CurrencyCode::parse("EUR").expect("currency");
    let result = interventions
        .mutate_line(
            &newer.id,
            LineMutation::Create(
                NewInterventionLine::new(
                    newer.id.clone(),
                    InterventionLineCategory::Part,
                    "Brake pads".into(),
                    Quantity::parse("1.5").expect("quantity"),
                    "set".into(),
                    Money::new(101, currency).expect("price"),
                    Some(Money::new(51, currency).expect("cost")),
                    0,
                    currency,
                )
                .expect("line"),
            ),
        )
        .await
        .expect("atomic line mutation");
    assert_eq!(result.totals.price.minor_units(), 152);
    assert_eq!(result.totals.cost.minor_units(), 77);
}

#[tokio::test]
async fn intervention_concurrency_has_one_transition_winner_and_freezes_lines() {
    let (customers, vehicles, interventions) = repositories().await;
    let vehicle = vehicle_fixture(&customers, &vehicles).await;
    let intervention = interventions
        .create(&intervention(
            &vehicle.id,
            19,
            100_000,
            Some("Inspection completed"),
        ))
        .await
        .expect("draft");

    let left = Arc::clone(&interventions);
    let right = Arc::clone(&interventions);
    let left_id = intervention.id.clone();
    let right_id = intervention.id.clone();
    let (left, right) = tokio::join!(
        left.transition_draft(&left_id, InterventionStatus::Completed),
        right.transition_draft(&right_id, InterventionStatus::Cancelled)
    );
    assert!(matches!(
        (&left, &right),
        (Ok(_), Err(RepositoryError::Conflict)) | (Err(RepositoryError::Conflict), Ok(_))
    ));

    let currency = CurrencyCode::parse("EUR").expect("currency");
    let line = NewInterventionLine::new(
        intervention.id.clone(),
        InterventionLineCategory::Labour,
        "Late edit".into(),
        Quantity::parse("1").expect("quantity"),
        "hour".into(),
        Money::new(5_000, currency).expect("price"),
        None,
        0,
        currency,
    )
    .expect("line");
    assert_eq!(
        interventions
            .mutate_line(&intervention.id, LineMutation::Create(line))
            .await,
        Err(RepositoryError::Conflict)
    );
}

fn intervention(
    vehicle_id: &pipauto::domain::VehicleId,
    day: u32,
    mileage: u64,
    performed_work: Option<&str>,
) -> NewIntervention {
    NewIntervention::new(
        vehicle_id.clone(),
        NaiveDate::from_ymd_opt(2026, 7, day).expect("date"),
        Some(mileage),
        None,
        None,
        performed_work.map(str::to_owned),
        None,
        None,
        CurrencyCode::parse("EUR").expect("currency"),
        Utc::now(),
    )
    .expect("intervention")
}

async fn vehicle_fixture(
    customers: &SurrealCustomerRepository,
    vehicles: &SurrealVehicleRepository,
) -> pipauto::models::vehicle::Vehicle {
    let owner = customers
        .create(&NewCustomer::new("Owner".into(), None, None, None, None).expect("customer"))
        .await
        .expect("owner");
    vehicles
        .create(
            &NewVehicle::new(
                owner.id,
                "Volkswagen".into(),
                "Golf".into(),
                Some(2020),
                None,
                None,
                None,
                None,
                None,
                2026,
            )
            .expect("vehicle"),
        )
        .await
        .expect("vehicle fixture")
}

async fn repositories() -> (
    Arc<SurrealCustomerRepository>,
    Arc<SurrealVehicleRepository>,
    Arc<SurrealInterventionRepository>,
) {
    let boot = boot_test::<App>().await.expect("application should boot");
    let client = boot
        .app_context
        .shared_store
        .get::<AppDatabase>()
        .expect("database")
        .client()
        .expect("client");
    (
        Arc::new(SurrealCustomerRepository::new(client.clone())),
        Arc::new(SurrealVehicleRepository::new(client.clone())),
        Arc::new(SurrealInterventionRepository::new(client)),
    )
}
