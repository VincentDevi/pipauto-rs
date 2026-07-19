//! SurrealDB invoice and append-only payment repository adapter.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use surrealdb::{
    engine::any::Any,
    types::{RecordId, SurrealValue, ToSql as _},
    Surreal,
};

use crate::{
    domain::{
        CurrencyCode, CursorTuple, CustomerId, InterventionId, InvoiceId, InvoiceLineId, Money,
        PageLimit, PaymentId, Quantity, VehicleId,
    },
    models::{
        auth::UserId,
        invoice::{
            payment_summary, BillingAddressSnapshot, Invoice, InvoiceNumber, InvoiceRecord,
            InvoiceStatus, InvoiceView,
        },
        invoice_line::{InvoiceLine, InvoiceLineRecord},
        payment::{Payment, PaymentMethod, PaymentRecord},
    },
    repositories::{
        customer::RepositoryPage,
        invoice::{
            DraftInvoiceUpdate, InvoiceFilter, InvoiceLineMutation, InvoiceLineMutationResult,
            InvoiceRepository, IssueInvoice, PaymentMutationResult,
        },
        RepositoryError,
    },
};

use super::support;

const INVOICE_PROJECTION: &str = "id, customer, vehicle, intervention, status, currency, issue_number, issue_date, due_date, customer_display_snapshot, billing_address_snapshot, notes, void_reason, subtotal_minor, total_minor, created_at, updated_at, issued_at, voided_at";
const LINE_PROJECTION: &str = "id, invoice, source_intervention_line, description, type::int(quantity * 1000dec) AS quantity_thousandths, unit_label, currency, unit_price_minor, line_total_minor, position, created_at, updated_at";
const PAYMENT_PROJECTION: &str = "id, invoice, amount_minor, currency, received_at, method, reference, notes, created_at, created_by";

#[derive(Clone)]
pub struct SurrealInvoiceRepository {
    client: Surreal<Any>,
}

impl SurrealInvoiceRepository {
    #[must_use]
    pub fn new(client: Surreal<Any>) -> Self {
        Self { client }
    }
}

#[derive(Clone, Deserialize, SurrealValue)]
struct DbAddress {
    line_1: String,
    line_2: Option<String>,
    postal_code: String,
    city: String,
    country_code: String,
}

#[derive(Deserialize, SurrealValue)]
struct DbInvoice {
    id: RecordId,
    customer: RecordId,
    vehicle: Option<RecordId>,
    intervention: Option<RecordId>,
    status: String,
    currency: String,
    issue_number: Option<String>,
    issue_date: Option<DateTime<Utc>>,
    due_date: Option<DateTime<Utc>>,
    customer_display_snapshot: Option<String>,
    billing_address_snapshot: Option<DbAddress>,
    notes: Option<String>,
    void_reason: Option<String>,
    subtotal_minor: i64,
    total_minor: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    issued_at: Option<DateTime<Utc>>,
    voided_at: Option<DateTime<Utc>>,
}

impl TryFrom<DbInvoice> for InvoiceRecord {
    type Error = RepositoryError;

    fn try_from(value: DbInvoice) -> Result<Self, Self::Error> {
        let currency =
            CurrencyCode::parse(&value.currency).map_err(|_| RepositoryError::CorruptData)?;
        Ok(Self {
            id: InvoiceId::parse(support::record_key(&value.id, "invoice")?)
                .map_err(|_| RepositoryError::CorruptData)?,
            invoice: Invoice {
                customer_id: CustomerId::parse(support::record_key(&value.customer, "customer")?)
                    .map_err(|_| RepositoryError::CorruptData)?,
                vehicle_id: value
                    .vehicle
                    .map(|record| {
                        VehicleId::parse(support::record_key(&record, "vehicle")?)
                            .map_err(|_| RepositoryError::CorruptData)
                    })
                    .transpose()?,
                intervention_id: value
                    .intervention
                    .map(|record| {
                        InterventionId::parse(support::record_key(&record, "intervention")?)
                            .map_err(|_| RepositoryError::CorruptData)
                    })
                    .transpose()?,
                status: parse_invoice_status(&value.status)?,
                currency,
                number: value
                    .issue_number
                    .map(InvoiceNumber::parse)
                    .transpose()
                    .map_err(|_| RepositoryError::CorruptData)?,
                issue_date: value.issue_date.map(|date| date.date_naive()),
                due_date: value.due_date.map(|date| date.date_naive()),
                customer_display_snapshot: value.customer_display_snapshot,
                billing_address_snapshot: value
                    .billing_address_snapshot
                    .map(|address| {
                        BillingAddressSnapshot::new(
                            address.line_1,
                            address.line_2,
                            address.postal_code,
                            address.city,
                            address.country_code,
                        )
                    })
                    .transpose()
                    .map_err(|_| RepositoryError::CorruptData)?,
                notes: value.notes,
                void_reason: value.void_reason,
                subtotal: Money::new(value.subtotal_minor, currency)
                    .map_err(|_| RepositoryError::CorruptData)?,
                total: Money::new(value.total_minor, currency)
                    .map_err(|_| RepositoryError::CorruptData)?,
                created_at: value.created_at,
                updated_at: value.updated_at,
                issued_at: value.issued_at,
                voided_at: value.voided_at,
            },
        })
    }
}

#[derive(Deserialize, SurrealValue)]
struct DbLine {
    id: RecordId,
    invoice: RecordId,
    source_intervention_line: Option<RecordId>,
    description: String,
    quantity_thousandths: i64,
    unit_label: String,
    currency: String,
    unit_price_minor: i64,
    line_total_minor: i64,
    position: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<DbLine> for InvoiceLineRecord {
    type Error = RepositoryError;

    fn try_from(value: DbLine) -> Result<Self, Self::Error> {
        let currency =
            CurrencyCode::parse(&value.currency).map_err(|_| RepositoryError::CorruptData)?;
        Ok(Self {
            id: InvoiceLineId::parse(support::record_key(&value.id, "invoice_line")?)
                .map_err(|_| RepositoryError::CorruptData)?,
            line: InvoiceLine {
                invoice_id: InvoiceId::parse(support::record_key(&value.invoice, "invoice")?)
                    .map_err(|_| RepositoryError::CorruptData)?,
                source_intervention_line_id: value
                    .source_intervention_line
                    .map(|record| {
                        crate::domain::InterventionLineId::parse(support::record_key(
                            &record,
                            "intervention_line",
                        )?)
                        .map_err(|_| RepositoryError::CorruptData)
                    })
                    .transpose()?,
                description: value.description,
                quantity: Quantity::from_thousandths(
                    u64::try_from(value.quantity_thousandths)
                        .map_err(|_| RepositoryError::CorruptData)?,
                )
                .map_err(|_| RepositoryError::CorruptData)?,
                unit_label: value.unit_label,
                unit_price: Money::new(value.unit_price_minor, currency)
                    .map_err(|_| RepositoryError::CorruptData)?,
                line_total: Money::new(value.line_total_minor, currency)
                    .map_err(|_| RepositoryError::CorruptData)?,
                position: u32::try_from(value.position)
                    .map_err(|_| RepositoryError::CorruptData)?,
            },
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

#[derive(Deserialize, SurrealValue)]
struct DbPayment {
    id: RecordId,
    invoice: RecordId,
    amount_minor: i64,
    currency: String,
    received_at: DateTime<Utc>,
    method: String,
    reference: Option<String>,
    notes: Option<String>,
    created_at: DateTime<Utc>,
    created_by: RecordId,
}

impl TryFrom<DbPayment> for PaymentRecord {
    type Error = RepositoryError;

    fn try_from(value: DbPayment) -> Result<Self, Self::Error> {
        let currency =
            CurrencyCode::parse(&value.currency).map_err(|_| RepositoryError::CorruptData)?;
        Ok(Self {
            id: PaymentId::parse(support::record_key(&value.id, "payment")?)
                .map_err(|_| RepositoryError::CorruptData)?,
            payment: Payment {
                invoice_id: InvoiceId::parse(support::record_key(&value.invoice, "invoice")?)
                    .map_err(|_| RepositoryError::CorruptData)?,
                amount: Money::new(value.amount_minor, currency)
                    .map_err(|_| RepositoryError::CorruptData)?,
                received_at: value.received_at,
                method: parse_payment_method(&value.method)?,
                reference: value.reference,
                notes: value.notes,
                created_at: value.created_at,
                created_by: UserId::parse(value.created_by.to_sql())
                    .map_err(|_| RepositoryError::CorruptData)?,
            },
        })
    }
}

#[async_trait]
impl InvoiceRepository for SurrealInvoiceRepository {
    async fn create(&self, value: &Invoice) -> Result<InvoiceRecord, RepositoryError> {
        let mut response = support::checked_response(
            self.client
                .query("CREATE invoice SET customer = $customer, vehicle = $vehicle, intervention = $intervention, currency = $currency, notes = $notes RETURN AFTER;")
                .bind(("customer", support::record_id("customer", value.customer_id.as_str())?))
                .bind(("vehicle", optional_record("vehicle", value.vehicle_id.as_ref().map(VehicleId::as_str))?))
                .bind(("intervention", optional_record("intervention", value.intervention_id.as_ref().map(InterventionId::as_str))?))
                .bind(("currency", value.currency.as_str().to_owned()))
                .bind(("notes", value.notes.clone()))
                .await,
        )?;
        let row: Option<DbInvoice> = support::take(&mut response, 0)?;
        row.ok_or(RepositoryError::CorruptData)?.try_into()
    }

    async fn find_by_id(&self, id: &InvoiceId) -> Result<Option<InvoiceRecord>, RepositoryError> {
        find_invoice_with_client(&self.client, id).await
    }

    async fn view(&self, id: &InvoiceId) -> Result<Option<InvoiceView>, RepositoryError> {
        view_with_client(&self.client, id).await
    }

    async fn list(
        &self,
        filter: &InvoiceFilter,
        limit: PageLimit,
        after: Option<&CursorTuple>,
    ) -> Result<RepositoryPage<InvoiceView>, RepositoryError> {
        let (after_created_at, after_id) = after
            .map(|cursor| support::surreal_cursor_tuple(cursor, "invoice"))
            .transpose()?
            .map_or((None, None), |(timestamp, id)| (Some(timestamp), Some(id)));
        let query = format!(
            "SELECT {INVOICE_PROJECTION} FROM invoice WHERE ($status IS NONE OR status = $status) AND ($after_created_at IS NONE OR created_at < $after_created_at OR (created_at = $after_created_at AND id < $after_id)) ORDER BY created_at DESC, id DESC LIMIT $fetch_limit;"
        );
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind((
                    "status",
                    filter.status.map(invoice_status_value).map(str::to_owned),
                ))
                .bind(("after_created_at", after_created_at))
                .bind(("after_id", after_id))
                .bind(("fetch_limit", i64::from(limit.value()) + 1))
                .await,
        )?;
        let mut rows: Vec<DbInvoice> = support::take(&mut response, 0)?;
        let has_more = rows.len() > usize::from(limit.value());
        if has_more {
            rows.pop();
        }
        let next = if has_more {
            rows.last()
                .map(|row| support::cursor_tuple(row.created_at, &row.id, "invoice"))
                .transpose()?
        } else {
            None
        };
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let id = InvoiceId::parse(support::record_key(&row.id, "invoice")?)
                .map_err(|_| RepositoryError::CorruptData)?;
            items.push(
                view_with_client(&self.client, &id)
                    .await?
                    .ok_or(RepositoryError::CorruptData)?,
            );
        }
        Ok(RepositoryPage { items, next })
    }

    async fn update_draft(
        &self,
        id: &InvoiceId,
        update: &DraftInvoiceUpdate,
    ) -> Result<InvoiceRecord, RepositoryError> {
        let value = &update.invoice;
        let mut response = support::checked_response(
            self.client
                .query("UPDATE ONLY $record SET customer = $customer, vehicle = $vehicle, intervention = $intervention, currency = $currency, notes = $notes WHERE status = 'draft' RETURN AFTER;")
                .bind(("record", support::record_id("invoice", id.as_str())?))
                .bind(("customer", support::record_id("customer", value.customer_id.as_str())?))
                .bind(("vehicle", optional_record("vehicle", value.vehicle_id.as_ref().map(VehicleId::as_str))?))
                .bind(("intervention", optional_record("intervention", value.intervention_id.as_ref().map(InterventionId::as_str))?))
                .bind(("currency", value.currency.as_str().to_owned()))
                .bind(("notes", value.notes.clone()))
                .await,
        )?;
        let row: Option<DbInvoice> = support::take(&mut response, 0)?;
        conditional_row(&self.client, id, row).await
    }

    async fn mutate_line(
        &self,
        invoice_id: &InvoiceId,
        mutation: InvoiceLineMutation,
    ) -> Result<InvoiceLineMutationResult, RepositoryError> {
        let transaction = begin(&self.client).await?;
        let result = async {
            let invoice = find_invoice_with_client(&transaction, invoice_id)
                .await?
                .ok_or(RepositoryError::NotFound)?;
            if invoice.invoice.status != InvoiceStatus::Draft {
                return Err(RepositoryError::Conflict);
            }
            let line = mutate_line_with_client(&transaction, invoice_id, mutation).await?;
            let lines = list_lines_with_client(&transaction, invoice_id).await?;
            let subtotal = lines.iter().try_fold(
                Money::new(0, invoice.invoice.currency)
                    .map_err(|_| RepositoryError::CorruptData)?,
                |total, line| total.checked_add(line.line.line_total),
            ).map_err(|_| RepositoryError::Conflict)?;
            let mut response = support::checked_response(
                transaction
                    .query("UPDATE ONLY $invoice SET subtotal_minor = $subtotal, total_minor = $subtotal WHERE status = 'draft' RETURN AFTER;")
                    .bind(("invoice", support::record_id("invoice", invoice_id.as_str())?))
                    .bind(("subtotal", subtotal.minor_units()))
                    .await,
            )?;
            let row: Option<DbInvoice> = support::take(&mut response, 0)?;
            Ok(InvoiceLineMutationResult {
                line,
                invoice: row.ok_or(RepositoryError::Conflict)?.try_into()?,
            })
        }.await;
        finish(transaction, result).await
    }

    async fn list_lines(
        &self,
        invoice_id: &InvoiceId,
    ) -> Result<Vec<InvoiceLineRecord>, RepositoryError> {
        if self.find_by_id(invoice_id).await?.is_none() {
            return Err(RepositoryError::NotFound);
        }
        list_lines_with_client(&self.client, invoice_id).await
    }

    async fn issue(
        &self,
        id: &InvoiceId,
        command: &IssueInvoice,
    ) -> Result<InvoiceView, RepositoryError> {
        let transaction = begin(&self.client).await?;
        let result = async {
            let current = find_invoice_with_client(&transaction, id)
                .await?
                .ok_or(RepositoryError::NotFound)?;
            if current.invoice.status != InvoiceStatus::Draft {
                return Err(RepositoryError::Conflict);
            }
            let lines = list_lines_with_client(&transaction, id).await?;
            if lines.is_empty() {
                return Err(RepositoryError::Conflict);
            }
            let total = lines.iter().try_fold(
                Money::new(0, current.invoice.currency)
                    .map_err(|_| RepositoryError::CorruptData)?,
                |total, line| total.checked_add(line.line.line_total),
            ).map_err(|_| RepositoryError::Conflict)?;
            let mut sequence = support::checked_response(
                transaction.query("RETURN sequence::nextval('invoice_issue_number');").await,
            )?;
            let sequence: surrealdb::types::Value = support::take(&mut sequence, 0)?;
            let sequence = serde_json::Value::from_value(sequence)
                .map_err(|_| RepositoryError::CorruptData)?
                .as_i64()
                .ok_or(RepositoryError::CorruptData)?;
            let sequence = u64::try_from(sequence).map_err(|_| RepositoryError::CorruptData)?;
            let number = InvoiceNumber::from_sequence(command.issued_at, sequence)
                .map_err(|_| RepositoryError::CorruptData)?;
            let mut response = support::checked_response(
                transaction
                    .query("UPDATE ONLY $record SET status = 'issued', issue_number = $number, issue_date = $issue_date, due_date = $due_date, customer_display_snapshot = $customer_display, billing_address_snapshot = $billing_address, subtotal_minor = $total, total_minor = $total, issued_at = $issued_at WHERE status = 'draft' RETURN AFTER;")
                    .bind(("record", support::record_id("invoice", id.as_str())?))
                    .bind(("number", number.as_str().to_owned()))
                    .bind(("issue_date", midnight(command.issue_date)))
                    .bind(("due_date", command.due_date.map(midnight)))
                    .bind(("customer_display", command.customer_display_snapshot.clone()))
                    .bind(("billing_address", command.billing_address_snapshot.as_ref().map(address_value)))
                    .bind(("total", total.minor_units()))
                    .bind(("issued_at", command.issued_at))
                    .await,
            )?;
            let row: Option<DbInvoice> = support::take(&mut response, 0)?;
            let invoice = row.ok_or(RepositoryError::Conflict)?.try_into()?;
            view_from_parts(&transaction, invoice).await
        }.await;
        finish(transaction, result).await
    }

    async fn void(&self, id: &InvoiceId, reason: &str) -> Result<InvoiceView, RepositoryError> {
        let transaction = begin(&self.client).await?;
        let result = async {
            let current = find_invoice_with_client(&transaction, id)
                .await?
                .ok_or(RepositoryError::NotFound)?;
            if current.invoice.status == InvoiceStatus::Void {
                return Err(RepositoryError::Conflict);
            }
            let payments = list_payments_with_client(&transaction, id).await?;
            if !payments.is_empty() {
                return Err(RepositoryError::Conflict);
            }
            let mut response = support::checked_response(
                transaction
                    .query("UPDATE ONLY $record SET status = 'void', void_reason = $reason, voided_at = time::now() WHERE status != 'void' RETURN AFTER;")
                    .bind(("record", support::record_id("invoice", id.as_str())?))
                    .bind(("reason", reason.to_owned()))
                    .await,
            )?;
            let row: Option<DbInvoice> = support::take(&mut response, 0)?;
            let invoice = row.ok_or(RepositoryError::Conflict)?.try_into()?;
            view_from_parts(&transaction, invoice).await
        }.await;
        finish(transaction, result).await
    }

    async fn record_payment(
        &self,
        invoice_id: &InvoiceId,
        payment: &Payment,
    ) -> Result<PaymentMutationResult, RepositoryError> {
        if payment.invoice_id != *invoice_id {
            return Err(RepositoryError::Conflict);
        }
        let transaction = begin(&self.client).await?;
        let result = async {
            let invoice = find_invoice_with_client(&transaction, invoice_id)
                .await?
                .ok_or(RepositoryError::NotFound)?;
            if invoice.invoice.status != InvoiceStatus::Issued
                || payment.amount.currency() != invoice.invoice.currency
            {
                return Err(RepositoryError::Conflict);
            }
            let existing = list_payments_with_client(&transaction, invoice_id).await?;
            let existing_amounts = existing
                .iter()
                .map(|record| record.payment.amount)
                .collect::<Vec<_>>();
            let mut candidate = existing_amounts;
            candidate.push(payment.amount);
            payment_summary(invoice.invoice.total, &candidate)
                .map_err(|_| RepositoryError::Conflict)?;
            let mut locked = support::checked_response(
                transaction
                    .query("UPDATE ONLY $invoice SET updated_at = time::now() WHERE updated_at = $expected_updated_at RETURN VALUE id;")
                    .bind(("invoice", support::record_id("invoice", invoice_id.as_str())?))
                    .bind(("expected_updated_at", invoice.invoice.updated_at))
                    .await,
            )?;
            let locked: Option<RecordId> = support::take(&mut locked, 0)?;
            if locked.is_none() {
                return Err(RepositoryError::Conflict);
            }
            let created_by = user_record(&payment.created_by)?;
            let mut response = support::checked_response(
                transaction
                    .query("CREATE payment SET invoice = $invoice, amount_minor = $amount, currency = $currency, received_at = $received_at, method = $method, reference = $reference, notes = $notes, created_by = $created_by RETURN AFTER;")
                    .bind(("invoice", support::record_id("invoice", invoice_id.as_str())?))
                    .bind(("amount", payment.amount.minor_units()))
                    .bind(("currency", payment.amount.currency().as_str().to_owned()))
                    .bind(("received_at", payment.received_at))
                    .bind(("method", payment_method_value(payment.method).to_owned()))
                    .bind(("reference", payment.reference.clone()))
                    .bind(("notes", payment.notes.clone()))
                    .bind(("created_by", created_by))
                    .await,
            )?;
            let row: Option<DbPayment> = support::take(&mut response, 0)?;
            let payment = row.ok_or(RepositoryError::CorruptData)?.try_into()?;
            let invoice = find_invoice_with_client(&transaction, invoice_id)
                .await?
                .ok_or(RepositoryError::CorruptData)?;
            let view = view_from_parts(&transaction, invoice).await?;
            Ok(PaymentMutationResult { payment, invoice: view })
        }.await;
        finish(transaction, result).await
    }

    async fn list_payments(
        &self,
        invoice_id: &InvoiceId,
    ) -> Result<Vec<PaymentRecord>, RepositoryError> {
        if self.find_by_id(invoice_id).await?.is_none() {
            return Err(RepositoryError::NotFound);
        }
        list_payments_with_client(&self.client, invoice_id).await
    }

    async fn find_payment(&self, id: &PaymentId) -> Result<Option<PaymentRecord>, RepositoryError> {
        let query = format!("SELECT {PAYMENT_PROJECTION} FROM ONLY $record;");
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind(("record", support::record_id("payment", id.as_str())?))
                .await,
        )?;
        let row: Option<DbPayment> = support::take(&mut response, 0)?;
        row.map(TryInto::try_into).transpose()
    }
}

trait QueryClient {
    fn query<'a>(&'a self, query: &'a str) -> surrealdb::method::Query<'a, Any>;
}

impl QueryClient for Surreal<Any> {
    fn query<'a>(&'a self, query: &'a str) -> surrealdb::method::Query<'a, Any> {
        Surreal::query(self, query)
    }
}

impl QueryClient for surrealdb::method::Transaction<Any> {
    fn query<'a>(&'a self, query: &'a str) -> surrealdb::method::Query<'a, Any> {
        surrealdb::method::Transaction::query(self, query)
    }
}

async fn find_invoice_with_client(
    client: &impl QueryClient,
    id: &InvoiceId,
) -> Result<Option<InvoiceRecord>, RepositoryError> {
    let query = format!("SELECT {INVOICE_PROJECTION} FROM ONLY $record;");
    let mut response = support::checked_response(
        client
            .query(&query)
            .bind(("record", support::record_id("invoice", id.as_str())?))
            .await,
    )?;
    let row: Option<DbInvoice> = support::take(&mut response, 0)?;
    row.map(TryInto::try_into).transpose()
}

async fn conditional_row(
    client: &impl QueryClient,
    id: &InvoiceId,
    row: Option<DbInvoice>,
) -> Result<InvoiceRecord, RepositoryError> {
    if let Some(row) = row {
        return row.try_into();
    }
    if find_invoice_with_client(client, id).await?.is_some() {
        Err(RepositoryError::Conflict)
    } else {
        Err(RepositoryError::NotFound)
    }
}

async fn view_with_client(
    client: &impl QueryClient,
    id: &InvoiceId,
) -> Result<Option<InvoiceView>, RepositoryError> {
    let Some(invoice) = find_invoice_with_client(client, id).await? else {
        return Ok(None);
    };
    view_from_parts(client, invoice).await.map(Some)
}

async fn view_from_parts(
    client: &impl QueryClient,
    invoice: InvoiceRecord,
) -> Result<InvoiceView, RepositoryError> {
    let lines = list_lines_with_client(client, &invoice.id).await?;
    let payments = list_payments_with_client(client, &invoice.id).await?;
    let amounts = payments
        .iter()
        .map(|record| record.payment.amount)
        .collect::<Vec<_>>();
    let (payment_status, outstanding) = payment_summary(invoice.invoice.total, &amounts)
        .map_err(|_| RepositoryError::CorruptData)?;
    let paid = invoice
        .invoice
        .total
        .checked_sub(outstanding)
        .map_err(|_| RepositoryError::CorruptData)?;
    Ok(InvoiceView {
        invoice,
        lines,
        payments,
        paid,
        outstanding,
        payment_status,
    })
}

async fn list_lines_with_client(
    client: &impl QueryClient,
    invoice_id: &InvoiceId,
) -> Result<Vec<InvoiceLineRecord>, RepositoryError> {
    let query = format!(
        "SELECT {LINE_PROJECTION} FROM invoice_line WHERE invoice = $invoice ORDER BY position ASC, id ASC;"
    );
    let mut response = support::checked_response(
        client
            .query(&query)
            .bind((
                "invoice",
                support::record_id("invoice", invoice_id.as_str())?,
            ))
            .await,
    )?;
    let rows: Vec<DbLine> = support::take(&mut response, 0)?;
    rows.into_iter().map(TryInto::try_into).collect()
}

async fn list_payments_with_client(
    client: &impl QueryClient,
    invoice_id: &InvoiceId,
) -> Result<Vec<PaymentRecord>, RepositoryError> {
    let query = format!(
        "SELECT {PAYMENT_PROJECTION} FROM payment WHERE invoice = $invoice ORDER BY received_at ASC, created_at ASC, id ASC;"
    );
    let mut response = support::checked_response(
        client
            .query(&query)
            .bind((
                "invoice",
                support::record_id("invoice", invoice_id.as_str())?,
            ))
            .await,
    )?;
    let rows: Vec<DbPayment> = support::take(&mut response, 0)?;
    rows.into_iter().map(TryInto::try_into).collect()
}

async fn mutate_line_with_client(
    client: &impl QueryClient,
    invoice_id: &InvoiceId,
    mutation: InvoiceLineMutation,
) -> Result<Option<InvoiceLineRecord>, RepositoryError> {
    match mutation {
        InvoiceLineMutation::Create(line) => {
            if line.invoice_id != *invoice_id {
                return Err(RepositoryError::Conflict);
            }
            let mut response = line_query(client, "CREATE invoice_line SET invoice = $invoice, source_intervention_line = $source, description = $description, quantity = $quantity_thousandths / 1000dec, unit_label = $unit_label, currency = $currency, unit_price_minor = $unit_price, line_total_minor = $line_total, position = $position RETURN VALUE id;", None, &line).await?;
            let record: Option<RecordId> = support::take(&mut response, 0)?;
            find_line_with_client(client, &record.ok_or(RepositoryError::CorruptData)?)
                .await
                .map(Some)
        }
        InvoiceLineMutation::Update { id, line } => {
            if line.invoice_id != *invoice_id {
                return Err(RepositoryError::Conflict);
            }
            let mut response = line_query(client, "UPDATE ONLY $record SET source_intervention_line = $source, description = $description, quantity = $quantity_thousandths / 1000dec, unit_label = $unit_label, currency = $currency, unit_price_minor = $unit_price, line_total_minor = $line_total, position = $position WHERE invoice = $invoice RETURN VALUE id;", Some(support::record_id("invoice_line", id.as_str())?), &line).await?;
            let record: Option<RecordId> = support::take(&mut response, 0)?;
            find_line_with_client(client, &record.ok_or(RepositoryError::NotFound)?)
                .await
                .map(Some)
        }
        InvoiceLineMutation::Delete { id } => {
            let record = support::record_id("invoice_line", id.as_str())?;
            let query = format!("SELECT {LINE_PROJECTION} FROM ONLY $record;");
            let mut response = support::checked_response(
                client.query(&query).bind(("record", record.clone())).await,
            )?;
            let existing: Option<DbLine> = support::take(&mut response, 0)?;
            let existing: InvoiceLineRecord =
                existing.ok_or(RepositoryError::NotFound)?.try_into()?;
            if existing.line.invoice_id != *invoice_id {
                return Err(RepositoryError::NotFound);
            }
            support::checked_response(
                client
                    .query("DELETE ONLY $record;")
                    .bind(("record", record))
                    .await,
            )?;
            Ok(None)
        }
    }
}

async fn find_line_with_client(
    client: &impl QueryClient,
    record: &RecordId,
) -> Result<InvoiceLineRecord, RepositoryError> {
    let query = format!("SELECT {LINE_PROJECTION} FROM ONLY $record;");
    let mut response =
        support::checked_response(client.query(&query).bind(("record", record.clone())).await)?;
    let row: Option<DbLine> = support::take(&mut response, 0)?;
    row.ok_or(RepositoryError::NotFound)?.try_into()
}

async fn line_query(
    client: &impl QueryClient,
    query: &str,
    record: Option<RecordId>,
    line: &InvoiceLine,
) -> Result<surrealdb::IndexedResults, RepositoryError> {
    let quantity =
        i64::try_from(line.quantity.thousandths()).map_err(|_| RepositoryError::CorruptData)?;
    let mut builder = client
        .query(query)
        .bind((
            "invoice",
            support::record_id("invoice", line.invoice_id.as_str())?,
        ))
        .bind((
            "source",
            optional_record(
                "intervention_line",
                line.source_intervention_line_id
                    .as_ref()
                    .map(crate::domain::InterventionLineId::as_str),
            )?,
        ))
        .bind(("description", line.description.clone()))
        .bind(("quantity_thousandths", quantity))
        .bind(("unit_label", line.unit_label.clone()))
        .bind(("currency", line.unit_price.currency().as_str().to_owned()))
        .bind(("unit_price", line.unit_price.minor_units()))
        .bind(("line_total", line.line_total.minor_units()))
        .bind(("position", i64::from(line.position)));
    if let Some(record) = record {
        builder = builder.bind(("record", record));
    }
    support::checked_response(builder.await)
}

async fn begin(
    client: &Surreal<Any>,
) -> Result<surrealdb::method::Transaction<Any>, RepositoryError> {
    client
        .clone()
        .begin()
        .await
        .map_err(|error| support::classify_query_error(&error))
}

async fn finish<T>(
    transaction: surrealdb::method::Transaction<Any>,
    result: Result<T, RepositoryError>,
) -> Result<T, RepositoryError> {
    match result {
        Ok(value) => {
            transaction
                .commit()
                .await
                .map_err(|error| support::classify_query_error(&error))?;
            Ok(value)
        }
        Err(error) => {
            transaction
                .cancel()
                .await
                .map_err(|cancel| support::classify_query_error(&cancel))?;
            Err(error)
        }
    }
}

fn optional_record(
    table: &'static str,
    key: Option<&str>,
) -> Result<Option<RecordId>, RepositoryError> {
    key.map(|key| support::record_id(table, key)).transpose()
}

fn user_record(user: &UserId) -> Result<RecordId, RepositoryError> {
    let key = user
        .as_str()
        .strip_prefix("user:")
        .ok_or(RepositoryError::CorruptData)?;
    support::record_id("user", key)
}

fn address_value(address: &BillingAddressSnapshot) -> DbAddress {
    DbAddress {
        line_1: address.line_1.clone(),
        line_2: address.line_2.clone(),
        postal_code: address.postal_code.clone(),
        city: address.city.clone(),
        country_code: address.country_code.clone(),
    }
}

fn midnight(date: NaiveDate) -> DateTime<Utc> {
    date.and_hms_opt(0, 0, 0)
        .expect("a date always has a midnight")
        .and_utc()
}

fn parse_invoice_status(value: &str) -> Result<InvoiceStatus, RepositoryError> {
    match value {
        "draft" => Ok(InvoiceStatus::Draft),
        "issued" => Ok(InvoiceStatus::Issued),
        "void" => Ok(InvoiceStatus::Void),
        _ => Err(RepositoryError::CorruptData),
    }
}

fn invoice_status_value(value: InvoiceStatus) -> &'static str {
    match value {
        InvoiceStatus::Draft => "draft",
        InvoiceStatus::Issued => "issued",
        InvoiceStatus::Void => "void",
    }
}

fn parse_payment_method(value: &str) -> Result<PaymentMethod, RepositoryError> {
    match value {
        "cash" => Ok(PaymentMethod::Cash),
        "bank_transfer" => Ok(PaymentMethod::BankTransfer),
        "card" => Ok(PaymentMethod::Card),
        "other" => Ok(PaymentMethod::Other),
        _ => Err(RepositoryError::CorruptData),
    }
}

fn payment_method_value(value: PaymentMethod) -> &'static str {
    match value {
        PaymentMethod::Cash => "cash",
        PaymentMethod::BankTransfer => "bank_transfer",
        PaymentMethod::Card => "card",
        PaymentMethod::Other => "other",
    }
}
