//! Authenticated invoice, line, issue, void, and append-only payment JSON routes.

use axum::{extract::DefaultBodyLimit, http::StatusCode, Json};
use loco_rs::{
    controller::{extractor::shared_store::SharedStore, Routes},
    prelude::{delete, get, patch, post, Path, Query},
};
use serde::{Deserialize, Serialize};

use crate::{
    api::{
        ids::{
            CustomerIdDto, InterventionIdDto, InterventionLineIdDto, InvoiceIdDto,
            InvoiceLineIdDto, PaymentIdDto, VehicleIdDto,
        },
        DataEnvelope, MoneyDto, PaginationEnvelope, QuantityDto, TimestampDto,
    },
    auth::{csrf::AuthenticatedCsrfJson, extractors::CurrentUser},
    domain::{
        CurrencyCode, CustomerId, InterventionId, InterventionLineId, InvoiceId, InvoiceLineId,
        PageRequest, PaymentId, Quantity, ValidationCode, ValidationError, ValidationErrors,
        VehicleId,
    },
    errors::AppError,
    models::{
        invoice::{
            BillingAddressSnapshot, CreateInvoice, InvoiceFilter, InvoiceLineMutationResult,
            InvoiceModel as InvoiceService, InvoiceStatus, InvoiceView, IssueInvoiceCommand,
            PaymentMutationResult, PaymentStatus, RecordPayment, UpdateInvoice, WriteInvoiceLine,
        },
        invoice_line::InvoiceLineRecord,
        payment::{PaymentMethod, PaymentRecord},
    },
    settings::BusinessSettings,
};

const BODY_LIMIT: usize = 64 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InvoiceQuery {
    limit: Option<u16>,
    cursor: Option<String>,
    status: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateInvoiceRequest {
    customer_id: String,
    vehicle_id: Option<String>,
    intervention_id: Option<String>,
    currency: Option<String>,
    notes: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateInvoiceRequest {
    customer_id: Option<String>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    vehicle_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    intervention_id: Option<Option<String>>,
    currency: Option<String>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    notes: Option<Option<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteLineRequest {
    source_intervention_line_id: Option<String>,
    description: String,
    quantity: String,
    unit_label: String,
    unit_price_minor: i64,
    position: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IssueRequest {
    issue_date: chrono::NaiveDate,
    due_date: Option<chrono::NaiveDate>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VoidRequest {
    reason: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PaymentRequest {
    amount_minor: i64,
    currency: String,
    received_at: chrono::DateTime<chrono::Utc>,
    method: String,
    reference: Option<String>,
    notes: Option<String>,
}

#[derive(Serialize)]
struct AddressDto {
    line_1: String,
    line_2: Option<String>,
    postal_code: String,
    city: String,
    country_code: String,
}

impl From<BillingAddressSnapshot> for AddressDto {
    fn from(value: BillingAddressSnapshot) -> Self {
        Self {
            line_1: value.line_1,
            line_2: value.line_2,
            postal_code: value.postal_code,
            city: value.city,
            country_code: value.country_code,
        }
    }
}

#[derive(Serialize)]
struct InvoiceLineDto {
    id: InvoiceLineIdDto,
    invoice_id: InvoiceIdDto,
    source_intervention_line_id: Option<InterventionLineIdDto>,
    description: String,
    quantity: QuantityDto,
    unit_label: String,
    unit_price: MoneyDto,
    line_total: MoneyDto,
    position: u32,
    created_at: TimestampDto,
    updated_at: TimestampDto,
}

impl From<InvoiceLineRecord> for InvoiceLineDto {
    fn from(value: InvoiceLineRecord) -> Self {
        Self {
            id: InvoiceLineIdDto::from(&value.id),
            invoice_id: InvoiceIdDto::from(&value.line.invoice_id),
            source_intervention_line_id: value
                .line
                .source_intervention_line_id
                .as_ref()
                .map(InterventionLineIdDto::from),
            description: value.line.description,
            quantity: value.line.quantity.into(),
            unit_label: value.line.unit_label,
            unit_price: value.line.unit_price.into(),
            line_total: value.line.line_total.into(),
            position: value.line.position,
            created_at: value.created_at.into(),
            updated_at: value.updated_at.into(),
        }
    }
}

#[derive(Serialize)]
struct PaymentDto {
    id: PaymentIdDto,
    invoice_id: InvoiceIdDto,
    amount: MoneyDto,
    received_at: TimestampDto,
    method: &'static str,
    reference: Option<String>,
    notes: Option<String>,
    created_at: TimestampDto,
}

impl From<PaymentRecord> for PaymentDto {
    fn from(value: PaymentRecord) -> Self {
        Self {
            id: PaymentIdDto::from(&value.id),
            invoice_id: InvoiceIdDto::from(&value.payment.invoice_id),
            amount: value.payment.amount.into(),
            received_at: value.payment.received_at.into(),
            method: payment_method_value(value.payment.method),
            reference: value.payment.reference,
            notes: value.payment.notes,
            created_at: value.payment.created_at.into(),
        }
    }
}

#[derive(Serialize)]
struct InvoiceDto {
    id: InvoiceIdDto,
    customer_id: CustomerIdDto,
    vehicle_id: Option<VehicleIdDto>,
    intervention_id: Option<InterventionIdDto>,
    status: &'static str,
    payment_status: &'static str,
    currency: String,
    number: Option<String>,
    issue_date: Option<String>,
    due_date: Option<String>,
    customer_display_snapshot: Option<String>,
    billing_address_snapshot: Option<AddressDto>,
    notes: Option<String>,
    void_reason: Option<String>,
    lines: Vec<InvoiceLineDto>,
    payments: Vec<PaymentDto>,
    subtotal: MoneyDto,
    total: MoneyDto,
    paid: MoneyDto,
    outstanding: MoneyDto,
    created_at: TimestampDto,
    updated_at: TimestampDto,
    issued_at: Option<TimestampDto>,
    voided_at: Option<TimestampDto>,
}

impl From<InvoiceView> for InvoiceDto {
    fn from(value: InvoiceView) -> Self {
        let record = value.invoice;
        let invoice = record.invoice;
        Self {
            id: InvoiceIdDto::from(&record.id),
            customer_id: CustomerIdDto::from(&invoice.customer_id),
            vehicle_id: invoice.vehicle_id.as_ref().map(VehicleIdDto::from),
            intervention_id: invoice
                .intervention_id
                .as_ref()
                .map(InterventionIdDto::from),
            status: invoice_status_value(invoice.status),
            payment_status: payment_status_value(value.payment_status),
            currency: invoice.currency.as_str().to_owned(),
            number: invoice.number.map(|number| number.as_str().to_owned()),
            issue_date: invoice.issue_date.map(|date| date.to_string()),
            due_date: invoice.due_date.map(|date| date.to_string()),
            customer_display_snapshot: invoice.customer_display_snapshot,
            billing_address_snapshot: invoice.billing_address_snapshot.map(Into::into),
            notes: invoice.notes,
            void_reason: invoice.void_reason,
            lines: value.lines.into_iter().map(Into::into).collect(),
            payments: value.payments.into_iter().map(Into::into).collect(),
            subtotal: invoice.subtotal.into(),
            total: invoice.total.into(),
            paid: value.paid.into(),
            outstanding: value.outstanding.into(),
            created_at: invoice.created_at.into(),
            updated_at: invoice.updated_at.into(),
            issued_at: invoice.issued_at.map(Into::into),
            voided_at: invoice.voided_at.map(Into::into),
        }
    }
}

#[derive(Serialize)]
struct LineMutationDto {
    line: Option<InvoiceLineDto>,
    subtotal: MoneyDto,
    total: MoneyDto,
}

impl From<InvoiceLineMutationResult> for LineMutationDto {
    fn from(value: InvoiceLineMutationResult) -> Self {
        Self {
            line: value.line.map(Into::into),
            subtotal: value.invoice.invoice.subtotal.into(),
            total: value.invoice.invoice.total.into(),
        }
    }
}

#[derive(Serialize)]
struct PaymentMutationDto {
    payment: PaymentDto,
    invoice: InvoiceDto,
}

impl From<PaymentMutationResult> for PaymentMutationDto {
    fn from(value: PaymentMutationResult) -> Self {
        Self {
            payment: value.payment.into(),
            invoice: value.invoice.into(),
        }
    }
}

async fn list(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InvoiceService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    Query(query): Query<InvoiceQuery>,
) -> Result<Json<PaginationEnvelope<InvoiceDto>>, AppError> {
    let pagination = crate::api::PaginationQuery {
        limit: query.limit,
        cursor: query.cursor,
    }
    .resolve(&settings)
    .map_err(AppError::Validation)?;
    let status = query.status.map(|value| parse_status(&value)).transpose()?;
    Ok(Json(
        service
            .list(PageRequest {
                filter: InvoiceFilter { status },
                limit: pagination.limit,
                after: pagination.after,
            })
            .await?
            .into(),
    ))
}

async fn create(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InvoiceService>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<CreateInvoiceRequest>,
) -> Result<(StatusCode, Json<DataEnvelope<InvoiceDto>>), AppError> {
    let invoice = service.create(create_command(request)?).await?;
    Ok((StatusCode::CREATED, Json(DataEnvelope::new(invoice.into()))))
}

async fn show(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InvoiceService>,
    Path(id): Path<String>,
) -> Result<Json<DataEnvelope<InvoiceDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service.get(&parse_invoice_id(id)?).await?.into(),
    )))
}

async fn update(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InvoiceService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<UpdateInvoiceRequest>,
) -> Result<Json<DataEnvelope<InvoiceDto>>, AppError> {
    let command = UpdateInvoice {
        customer_id: request.customer_id.map(parse_customer_id).transpose()?,
        vehicle_id: request
            .vehicle_id
            .map(|value| value.map(parse_vehicle_id).transpose())
            .transpose()?,
        intervention_id: request
            .intervention_id
            .map(|value| value.map(parse_intervention_id).transpose())
            .transpose()?,
        currency: request.currency.map(parse_currency).transpose()?,
        notes: request.notes,
    };
    Ok(Json(DataEnvelope::new(
        service
            .update(&parse_invoice_id(id)?, command)
            .await?
            .into(),
    )))
}

async fn issue(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InvoiceService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<IssueRequest>,
) -> Result<Json<DataEnvelope<InvoiceDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service
            .issue(
                &parse_invoice_id(id)?,
                IssueInvoiceCommand {
                    issue_date: request.issue_date,
                    due_date: request.due_date,
                },
            )
            .await?
            .into(),
    )))
}

async fn void(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InvoiceService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<VoidRequest>,
) -> Result<Json<DataEnvelope<InvoiceDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service
            .void(&parse_invoice_id(id)?, request.reason)
            .await?
            .into(),
    )))
}

async fn lines(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InvoiceService>,
    Path(id): Path<String>,
) -> Result<Json<DataEnvelope<Vec<InvoiceLineDto>>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service
            .list_lines(&parse_invoice_id(id)?)
            .await?
            .into_iter()
            .map(Into::into)
            .collect(),
    )))
}

async fn create_line(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InvoiceService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<WriteLineRequest>,
) -> Result<(StatusCode, Json<DataEnvelope<LineMutationDto>>), AppError> {
    let result = service
        .create_line(&parse_invoice_id(id)?, line_command(request)?)
        .await?;
    Ok((StatusCode::CREATED, Json(DataEnvelope::new(result.into()))))
}

async fn update_line(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InvoiceService>,
    Path((id, line_id)): Path<(String, String)>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<WriteLineRequest>,
) -> Result<Json<DataEnvelope<LineMutationDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service
            .update_line(
                &parse_invoice_id(id)?,
                parse_line_id(line_id)?,
                line_command(request)?,
            )
            .await?
            .into(),
    )))
}

async fn delete_line(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InvoiceService>,
    Path((id, line_id)): Path<(String, String)>,
    AuthenticatedCsrfJson(()): AuthenticatedCsrfJson<()>,
) -> Result<Json<DataEnvelope<LineMutationDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service
            .delete_line(&parse_invoice_id(id)?, parse_line_id(line_id)?)
            .await?
            .into(),
    )))
}

async fn payments(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InvoiceService>,
    Path(id): Path<String>,
) -> Result<Json<DataEnvelope<Vec<PaymentDto>>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service
            .list_payments(&parse_invoice_id(id)?)
            .await?
            .into_iter()
            .map(Into::into)
            .collect(),
    )))
}

async fn record_payment(
    CurrentUser(user): CurrentUser,
    SharedStore(service): SharedStore<InvoiceService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<PaymentRequest>,
) -> Result<(StatusCode, Json<DataEnvelope<PaymentMutationDto>>), AppError> {
    let result = service
        .record_payment(
            &parse_invoice_id(id)?,
            RecordPayment {
                amount_minor: request.amount_minor,
                currency: parse_currency(request.currency)?,
                received_at: request.received_at,
                method: parse_payment_method(&request.method)?,
                reference: request.reference,
                notes: request.notes,
            },
            user.id,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(DataEnvelope::new(result.into()))))
}

async fn show_payment(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InvoiceService>,
    Path(id): Path<String>,
) -> Result<Json<DataEnvelope<PaymentDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service.get_payment(&parse_payment_id(id)?).await?.into(),
    )))
}

fn create_command(request: CreateInvoiceRequest) -> Result<CreateInvoice, AppError> {
    Ok(CreateInvoice {
        customer_id: parse_customer_id(request.customer_id)?,
        vehicle_id: request.vehicle_id.map(parse_vehicle_id).transpose()?,
        intervention_id: request
            .intervention_id
            .map(parse_intervention_id)
            .transpose()?,
        currency: request
            .currency
            .map_or_else(|| parse_currency("EUR".into()), parse_currency)?,
        notes: request.notes,
    })
}

fn line_command(request: WriteLineRequest) -> Result<WriteInvoiceLine, AppError> {
    Ok(WriteInvoiceLine {
        source_intervention_line_id: request
            .source_intervention_line_id
            .map(parse_source_line_id)
            .transpose()?,
        description: request.description,
        quantity: Quantity::parse(&request.quantity).map_err(|_| {
            invalid(
                "quantity",
                "Enter a positive quantity with up to three decimals.",
            )
        })?,
        unit_label: request.unit_label,
        unit_price_minor: request.unit_price_minor,
        position: request.position,
    })
}

fn parse_status(value: &str) -> Result<InvoiceStatus, AppError> {
    match value {
        "draft" => Ok(InvoiceStatus::Draft),
        "issued" => Ok(InvoiceStatus::Issued),
        "void" => Ok(InvoiceStatus::Void),
        _ => Err(invalid("status", "Use draft, issued, or void.")),
    }
}

fn parse_payment_method(value: &str) -> Result<PaymentMethod, AppError> {
    match value {
        "cash" => Ok(PaymentMethod::Cash),
        "bank_transfer" => Ok(PaymentMethod::BankTransfer),
        "card" => Ok(PaymentMethod::Card),
        "other" => Ok(PaymentMethod::Other),
        _ => Err(invalid(
            "method",
            "Use cash, bank_transfer, card, or other.",
        )),
    }
}

fn invoice_status_value(value: InvoiceStatus) -> &'static str {
    match value {
        InvoiceStatus::Draft => "draft",
        InvoiceStatus::Issued => "issued",
        InvoiceStatus::Void => "void",
    }
}

fn payment_status_value(value: PaymentStatus) -> &'static str {
    match value {
        PaymentStatus::Unpaid => "unpaid",
        PaymentStatus::PartiallyPaid => "partially_paid",
        PaymentStatus::Paid => "paid",
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

fn parse_currency(value: String) -> Result<CurrencyCode, AppError> {
    CurrencyCode::parse(&value)
        .map_err(|_| invalid("currency", "Use an assigned uppercase currency code."))
}

fn parse_customer_id(value: String) -> Result<CustomerId, AppError> {
    CustomerId::parse(value).map_err(|_| invalid("customer_id", "Use a valid customer identifier."))
}

fn parse_vehicle_id(value: String) -> Result<VehicleId, AppError> {
    VehicleId::parse(value).map_err(|_| invalid("vehicle_id", "Use a valid vehicle identifier."))
}

fn parse_intervention_id(value: String) -> Result<InterventionId, AppError> {
    InterventionId::parse(value)
        .map_err(|_| invalid("intervention_id", "Use a valid intervention identifier."))
}

fn parse_invoice_id(value: String) -> Result<InvoiceId, AppError> {
    InvoiceId::parse(value).map_err(|_| invalid("id", "Use a valid invoice identifier."))
}

fn parse_line_id(value: String) -> Result<InvoiceLineId, AppError> {
    InvoiceLineId::parse(value)
        .map_err(|_| invalid("line_id", "Use a valid invoice-line identifier."))
}

fn parse_source_line_id(value: String) -> Result<InterventionLineId, AppError> {
    InterventionLineId::parse(value).map_err(|_| {
        invalid(
            "source_intervention_line_id",
            "Use a valid intervention-line identifier.",
        )
    })
}

fn parse_payment_id(value: String) -> Result<PaymentId, AppError> {
    PaymentId::parse(value).map_err(|_| invalid("id", "Use a valid payment identifier."))
}

fn invalid(field: &str, message: &str) -> AppError {
    AppError::Validation(ValidationErrors::one(
        ValidationError::new(field, ValidationCode::InvalidFormat, message)
            .expect("static validation metadata is valid"),
    ))
}

#[must_use]
pub fn routes() -> Routes {
    Routes::new()
        .add("/invoices", get(list))
        .add(
            "/invoices",
            post(create).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add("/invoices/{id}", get(show))
        .add(
            "/invoices/{id}",
            patch(update).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add(
            "/invoices/{id}/issue",
            post(issue).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add(
            "/invoices/{id}/void",
            post(void).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add("/invoices/{id}/lines", get(lines))
        .add(
            "/invoices/{id}/lines",
            post(create_line).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add(
            "/invoices/{id}/lines/{line_id}",
            patch(update_line).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add(
            "/invoices/{id}/lines/{line_id}",
            delete(delete_line).layer(DefaultBodyLimit::max(64)),
        )
        .add("/invoices/{id}/payments", get(payments))
        .add(
            "/invoices/{id}/payments",
            post(record_payment).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add("/payments/{id}", get(show_payment))
}
