use std::sync::Arc;

use chrono::{TimeZone as _, Utc};
use loco_rs::testing::request::boot_test;
use pipauto::{
    app::App,
    database::client::AppDatabase,
    domain::{CurrencyCode, Money, PageLimit, Quantity, WorkshopTime},
    models::{
        calendar::CalendarModel as CalendarService,
        customer::NewCustomer,
        intervention::{
            EstimatedDuration, InterventionIdentitySnapshot, InterventionStatus, NewIntervention,
        },
        intervention_line::{InterventionLineCategory, NewInterventionLine},
        vehicle::NewVehicle,
    },
    testing::persistence::{
        calendar::CalendarRepository,
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
async fn calendar_repository_returns_exact_bounded_overlap_without_totals_or_live_joins() {
    let (customers, vehicles, interventions) = repositories().await;
    let vehicle = vehicle_fixture(&customers, &vehicles).await;
    let workshop_time = WorkshopTime::system(chrono_tz::Europe::Brussels);

    let ending_at_start = interventions
        .create(&scheduled_intervention(
            &vehicle,
            workshop_time
                .local_to_utc("2026-06-30T23:00")
                .expect("boundary fixture"),
            60,
            "ending-at-start",
        ))
        .await
        .expect("ending-at-start job");
    let maximum_lookback = interventions
        .create(&scheduled_intervention(
            &vehicle,
            workshop_time
                .local_to_utc("2026-06-30T00:30")
                .expect("maximum-lookback fixture"),
            1_440,
            "maximum-lookback",
        ))
        .await
        .expect("maximum-lookback job");
    let overlapping_lookback = interventions
        .create(&scheduled_intervention(
            &vehicle,
            workshop_time
                .local_to_utc("2026-06-30T23:30")
                .expect("lookback fixture"),
            60,
            "lookback",
        ))
        .await
        .expect("lookback job");
    let completed = interventions
        .create(&scheduled_intervention(
            &vehicle,
            workshop_time
                .local_to_utc("2026-07-15T09:00")
                .expect("completed fixture"),
            90,
            "completed",
        ))
        .await
        .expect("completed job");
    interventions
        .transition_draft(&completed.id, InterventionStatus::Completed)
        .await
        .expect("complete job");
    let cancelled = interventions
        .create(&scheduled_intervention(
            &vehicle,
            workshop_time
                .local_to_utc("2026-07-16T09:00")
                .expect("cancelled fixture"),
            60,
            "cancelled",
        ))
        .await
        .expect("cancelled job");
    interventions
        .transition_draft(&cancelled.id, InterventionStatus::Cancelled)
        .await
        .expect("cancel job");
    let starting_at_end = interventions
        .create(&scheduled_intervention(
            &vehicle,
            workshop_time
                .local_to_utc("2026-08-01T00:00")
                .expect("range-end fixture"),
            60,
            "starting-at-end",
        ))
        .await
        .expect("starting-at-end job");

    let calendar_repository: Arc<dyn CalendarRepository> = interventions;
    let schedule = CalendarService::new(calendar_repository, workshop_time)
        .month(Some("2026-07-15".parse().expect("anchor")))
        .await
        .expect("calendar query");
    let ids = schedule
        .entries
        .iter()
        .map(|entry| entry.id.clone())
        .collect::<Vec<_>>();

    assert_eq!(
        ids,
        vec![maximum_lookback.id, overlapping_lookback.id, completed.id]
    );
    assert!(!ids.contains(&ending_at_start.id));
    assert!(!ids.contains(&cancelled.id));
    assert!(!ids.contains(&starting_at_end.id));
    assert_eq!(schedule.entries[0].identity_snapshot.customer_name, "Owner");
}

#[tokio::test]
async fn intervention_repository_preserves_mileage_history_and_archived_readability() {
    let (customers, vehicles, interventions) = repositories().await;
    let vehicle = vehicle_fixture(&customers, &vehicles).await;

    let first = interventions
        .create(&intervention(&vehicle, 1, 100_000, Some("Initial service")))
        .await
        .expect("first intervention");
    interventions
        .create(&intervention(&vehicle, 20, 120_000, Some("Later service")))
        .await
        .expect("later intervention");
    interventions
        .create(&intervention(
            &vehicle,
            10,
            110_000,
            Some("Backdated service"),
        ))
        .await
        .expect("valid backdated intervention");

    assert_eq!(
        interventions
            .create(&intervention(&vehicle, 15, 120_001, Some("Regression")))
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
            .create(&intervention(&vehicle, 21, 121_000, Some("Archived work")))
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
        .create(&intervention(&vehicle, 19, 100_000, Some("Older")))
        .await
        .expect("older same-date intervention");
    let newer = interventions
        .create(&intervention(&vehicle, 19, 100_000, Some("Newer")))
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
            &vehicle,
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

#[tokio::test]
async fn intervention_history_filters_and_mileage_use_complete_timestamps() {
    let (customers, vehicles, interventions) = repositories().await;
    let vehicle = vehicle_fixture(&customers, &vehicles).await;
    let mut morning = intervention(&vehicle, 19, 100_000, Some("Morning"));
    morning.service_date = Utc
        .with_ymd_and_hms(2026, 7, 19, 9, 0, 0)
        .single()
        .expect("morning");
    let mut afternoon = intervention(&vehicle, 19, 120_000, Some("Afternoon"));
    afternoon.service_date = Utc
        .with_ymd_and_hms(2026, 7, 19, 15, 0, 0)
        .single()
        .expect("afternoon");
    let mut midday = intervention(&vehicle, 19, 110_000, Some("Midday"));
    midday.service_date = Utc
        .with_ymd_and_hms(2026, 7, 19, 12, 0, 0)
        .single()
        .expect("midday");

    interventions.create(&morning).await.expect("morning job");
    let afternoon = interventions
        .create(&afternoon)
        .await
        .expect("afternoon job");
    let midday = interventions.create(&midday).await.expect("midday job");

    let page = interventions
        .vehicle_history(
            &vehicle.id,
            &InterventionFilter {
                service_date_from: Some(
                    Utc.with_ymd_and_hms(2026, 7, 19, 12, 0, 0)
                        .single()
                        .expect("range start"),
                ),
                service_date_until: Some(
                    Utc.with_ymd_and_hms(2026, 7, 19, 16, 0, 0)
                        .single()
                        .expect("range end"),
                ),
                ..InterventionFilter::default()
            },
            PageLimit::new(10).expect("limit"),
            None,
        )
        .await
        .expect("timestamp-filtered history");
    assert_eq!(
        page.items
            .iter()
            .map(|entry| &entry.intervention.id)
            .collect::<Vec<_>>(),
        vec![&afternoon.id, &midday.id]
    );
}

fn intervention(
    vehicle: &pipauto::models::vehicle::Vehicle,
    day: u32,
    mileage: u64,
    performed_work: Option<&str>,
) -> NewIntervention {
    NewIntervention::new(
        vehicle.id.clone(),
        Utc.with_ymd_and_hms(2026, 7, day, 9, 0, 0)
            .single()
            .expect("timestamp"),
        EstimatedDuration::new(60).expect("duration"),
        InterventionIdentitySnapshot::new(
            vehicle.customer_id.clone(),
            "Owner".into(),
            vehicle.registration.clone(),
            vehicle.make.clone(),
            vehicle.model.clone(),
        )
        .expect("snapshot"),
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

fn scheduled_intervention(
    vehicle: &pipauto::models::vehicle::Vehicle,
    service_date: chrono::DateTime<Utc>,
    duration_minutes: u16,
    performed_work: &str,
) -> NewIntervention {
    let mut value = intervention(vehicle, 1, 100_000, Some(performed_work));
    value.service_date = service_date;
    value.estimated_duration = EstimatedDuration::new(duration_minutes).expect("duration");
    value
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
