use std::sync::Arc;

use loco_rs::testing::request::boot_test;
use pipauto::{
    app::App,
    database::client::AppDatabase,
    domain::{NormalizedRegistration, PageLimit},
    models::{customer::NewCustomer, vehicle::NewVehicle},
    repositories::{
        customer::{CustomerFilter, CustomerRepository},
        surreal::{customer::SurrealCustomerRepository, vehicle::SurrealVehicleRepository},
        vehicle::{VehicleFilter, VehicleRepository},
        RepositoryError,
    },
};

#[tokio::test]
async fn customer_repository_paginates_and_archives_idempotently() {
    let customers = repositories().await.0;
    for name in ["Ada Lovelace", "Grace Hopper", "Margaret Hamilton"] {
        customers
            .create(&NewCustomer::new(name.into(), None, None, None, None).expect("valid"))
            .await
            .expect("create customer");
    }

    let first = customers
        .list(
            &CustomerFilter::default(),
            PageLimit::new(2).expect("limit"),
            None,
        )
        .await
        .expect("first page");
    assert_eq!(first.items.len(), 2);
    let second = customers
        .list(
            &CustomerFilter::default(),
            PageLimit::new(2).expect("limit"),
            first.next.as_ref(),
        )
        .await
        .expect("second page");
    assert_eq!(second.items.len(), 1);

    let id = first.items[0].id.clone();
    let archived = customers.archive(&id).await.expect("archive");
    let repeated = customers.archive(&id).await.expect("idempotent archive");
    assert_eq!(archived.archived_at, repeated.archived_at);
    assert_eq!(archived.updated_at, repeated.updated_at);
}

#[tokio::test]
async fn vehicle_repository_classifies_concurrent_identifier_conflicts_and_owner_state() {
    let (customers, vehicles) = repositories().await;
    let owner = customers
        .create(&NewCustomer::new("Owner".into(), None, None, None, None).expect("valid"))
        .await
        .expect("owner");
    let new_vehicle = || {
        NewVehicle::new(
            owner.id.clone(),
            "Volkswagen".into(),
            "Golf".into(),
            Some(2020),
            Some("1-abc-234".into()),
            None,
            None,
            None,
            None,
            2026,
        )
        .expect("valid vehicle")
    };
    let left_input = new_vehicle();
    let right_input = new_vehicle();
    let (left, right) = tokio::join!(vehicles.create(&left_input), vehicles.create(&right_input));
    assert!(matches!(
        (&left, &right),
        (Ok(_), Err(RepositoryError::Conflict)) | (Err(RepositoryError::Conflict), Ok(_))
    ));

    let found = vehicles
        .find_by_registration(&NormalizedRegistration::parse("1 ABC 234").expect("normalized"))
        .await
        .expect("lookup")
        .expect("vehicle");
    let listed = vehicles
        .list_by_customer(
            &owner.id,
            &VehicleFilter::default(),
            PageLimit::new(25).expect("limit"),
            None,
        )
        .await
        .expect("customer list");
    assert_eq!(listed.items[0].id, found.id);

    customers.archive(&owner.id).await.expect("archive owner");
    let rejected = vehicles
        .create(
            &NewVehicle::new(
                owner.id,
                "Volvo".into(),
                "V70".into(),
                None,
                Some("2-XYZ-999".into()),
                None,
                None,
                None,
                None,
                2026,
            )
            .expect("valid vehicle"),
        )
        .await;
    assert_eq!(rejected, Err(RepositoryError::Conflict));
}

async fn repositories() -> (
    Arc<SurrealCustomerRepository>,
    Arc<SurrealVehicleRepository>,
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
        Arc::new(SurrealVehicleRepository::new(client)),
    )
}
