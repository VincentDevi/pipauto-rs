use chrono::{NaiveDate, Utc};
use loco_rs::testing::request::boot_test;
use pipauto::{
    app::App,
    domain::{CurrencyCode, InvoiceId, Quantity},
    models::{auth::UserId, invoice::PaymentStatus, payment::PaymentMethod},
    services::{
        auth::AuthService,
        customer::{CreateCustomer, CustomerService},
        invoice::{
            CreateInvoice, InvoiceService, IssueInvoiceCommand, RecordPayment, WriteInvoiceLine,
        },
        WorkflowError,
    },
};

#[tokio::test]
async fn invoice_repository_recalculates_draft_totals_atomically() {
    let fixture = invoice_fixture().await;
    let first = fixture
        .service
        .create_line(&fixture.invoice_id, line(0, "1.5", 101))
        .await
        .expect("first line");
    assert_eq!(first.invoice.invoice.subtotal.minor_units(), 152);
    let second = fixture
        .service
        .create_line(&fixture.invoice_id, line(1, "2", 49))
        .await
        .expect("second line");
    assert_eq!(second.invoice.invoice.subtotal.minor_units(), 250);
    assert_eq!(second.invoice.invoice.total.minor_units(), 250);
    fixture
        .service
        .delete_line(&fixture.invoice_id, first.line.expect("created line").id)
        .await
        .expect("delete line");
    let view = fixture
        .service
        .get(&fixture.invoice_id)
        .await
        .expect("invoice view");
    assert_eq!(view.invoice.invoice.total.minor_units(), 98);
}

#[tokio::test]
async fn invoice_service_requires_non_empty_draft_and_freezes_snapshot() {
    let fixture = invoice_fixture().await;
    assert_eq!(
        fixture
            .service
            .issue(&fixture.invoice_id, issue_command())
            .await,
        Err(WorkflowError::Conflict)
    );
    fixture
        .service
        .create_line(&fixture.invoice_id, line(0, "1", 100))
        .await
        .expect("line");
    let issued = fixture
        .service
        .issue(&fixture.invoice_id, issue_command())
        .await
        .expect("issue");
    assert_eq!(
        issued.invoice.invoice.status,
        pipauto::models::invoice::InvoiceStatus::Issued
    );
    assert_eq!(
        issued.invoice.invoice.customer_display_snapshot.as_deref(),
        Some("Invoice Customer")
    );
    assert!(issued.invoice.invoice.number.is_some());
}

#[tokio::test]
async fn invoice_concurrency_allows_one_issue_transition() {
    let fixture = invoice_fixture().await;
    fixture
        .service
        .create_line(&fixture.invoice_id, line(0, "1", 100))
        .await
        .expect("line");
    let left_service = fixture.service.clone();
    let right_service = fixture.service.clone();
    let left_id = fixture.invoice_id.clone();
    let right_id = fixture.invoice_id.clone();
    let (left, right) = tokio::join!(
        left_service.issue(&left_id, issue_command()),
        right_service.issue(&right_id, issue_command())
    );
    assert!(
        matches!(
            (&left, &right),
            (Ok(_), Err(WorkflowError::Conflict)) | (Err(WorkflowError::Conflict), Ok(_))
        ),
        "left={left:?}, right={right:?}"
    );
}

#[tokio::test]
async fn payment_service_derives_balance_and_rejects_overpayment() {
    let fixture = issued_fixture(100).await;
    let partial = fixture
        .service
        .record_payment(&fixture.invoice_id, payment(40), fixture.user_id.clone())
        .await
        .expect("partial payment");
    assert_eq!(partial.invoice.payment_status, PaymentStatus::PartiallyPaid);
    assert_eq!(partial.invoice.paid.minor_units(), 40);
    assert_eq!(partial.invoice.outstanding.minor_units(), 60);
    assert_eq!(
        fixture
            .service
            .record_payment(&fixture.invoice_id, payment(61), fixture.user_id)
            .await,
        Err(WorkflowError::Conflict)
    );
}

#[tokio::test]
async fn payment_concurrency_spends_outstanding_balance_once() {
    let fixture = issued_fixture(100).await;
    let left_service = fixture.service.clone();
    let right_service = fixture.service.clone();
    let left_id = fixture.invoice_id.clone();
    let right_id = fixture.invoice_id.clone();
    let left_user = fixture.user_id.clone();
    let right_user = fixture.user_id;
    let (left, right) = tokio::join!(
        left_service.record_payment(&left_id, payment(100), left_user),
        right_service.record_payment(&right_id, payment(100), right_user)
    );
    assert!(
        matches!(
            (&left, &right),
            (Ok(_), Err(WorkflowError::Conflict)) | (Err(WorkflowError::Conflict), Ok(_))
        ),
        "left={left:?}, right={right:?}"
    );
    let view = fixture
        .service
        .get(&fixture.invoice_id)
        .await
        .expect("authoritative view");
    assert_eq!(view.payment_status, PaymentStatus::Paid);
    assert_eq!(view.payments.len(), 1);
}

struct InvoiceFixture {
    service: InvoiceService,
    invoice_id: InvoiceId,
    user_id: UserId,
}

async fn invoice_fixture() -> InvoiceFixture {
    let boot = boot_test::<App>().await.expect("application should boot");
    let user = boot
        .app_context
        .shared_store
        .get::<AuthService>()
        .expect("auth service")
        .create_user(
            "invoice-tests@example.com",
            "Invoice Tester",
            "Workshop-password-123",
        )
        .await
        .expect("user");
    let customer = boot
        .app_context
        .shared_store
        .get::<CustomerService>()
        .expect("customer service")
        .create(CreateCustomer {
            display_name: "Invoice Customer".into(),
            email: None,
            phone: None,
            address: None,
            notes: None,
        })
        .await
        .expect("customer");
    let service = boot
        .app_context
        .shared_store
        .get::<InvoiceService>()
        .expect("invoice service");
    let invoice = service
        .create(CreateInvoice {
            customer_id: customer.id,
            vehicle_id: None,
            intervention_id: None,
            currency: eur(),
            notes: None,
        })
        .await
        .expect("draft invoice");
    InvoiceFixture {
        service,
        invoice_id: invoice.invoice.id,
        user_id: user.id,
    }
}

async fn issued_fixture(total: i64) -> InvoiceFixture {
    let fixture = invoice_fixture().await;
    fixture
        .service
        .create_line(&fixture.invoice_id, line(0, "1", total))
        .await
        .expect("line");
    fixture
        .service
        .issue(&fixture.invoice_id, issue_command())
        .await
        .expect("issued invoice");
    fixture
}

fn line(position: u32, quantity: &str, amount: i64) -> WriteInvoiceLine {
    WriteInvoiceLine {
        source_intervention_line_id: None,
        description: format!("Invoice line {position}"),
        quantity: Quantity::parse(quantity).expect("quantity"),
        unit_label: "item".into(),
        unit_price_minor: amount,
        position,
    }
}

fn issue_command() -> IssueInvoiceCommand {
    IssueInvoiceCommand {
        issue_date: NaiveDate::from_ymd_opt(2026, 7, 19).expect("date"),
        due_date: Some(NaiveDate::from_ymd_opt(2026, 8, 19).expect("date")),
    }
}

fn payment(amount_minor: i64) -> RecordPayment {
    RecordPayment {
        amount_minor,
        currency: eur(),
        received_at: Utc::now(),
        method: PaymentMethod::BankTransfer,
        reference: None,
        notes: None,
    }
}

fn eur() -> CurrencyCode {
    CurrencyCode::parse("EUR").expect("currency")
}
