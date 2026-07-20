//! Invoice drafting, immutable issuing, and append-only payment workflows.

use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};

use crate::{
    domain::{
        CurrencyCode, CursorCodec, CursorResource, CustomerId, InterventionId, InterventionLineId,
        InvoiceId, InvoiceLineId, Money, Page, PageRequest, PaymentId, Quantity, ValidationCode,
        ValidationError, ValidationErrors, VehicleId,
    },
    models::{
        auth::UserId,
        customer::Customer,
        intervention::InterventionStatus,
        invoice::{
            BillingAddressSnapshot, Invoice, InvoiceError, InvoiceRecord, InvoiceStatus,
            InvoiceView,
        },
        invoice_line::{InvoiceLine, InvoiceLineError},
        payment::{Payment, PaymentError, PaymentMethod, PaymentRecord},
    },
    repositories::{
        customer::CustomerRepository,
        intervention::InterventionRepository,
        invoice::{
            DraftInvoiceUpdate, InvoiceFilter, InvoiceLineMoveDirection, InvoiceLineMutation,
            InvoiceLineMutationResult, InvoiceRepository, IssueInvoice, PaymentMutationResult,
        },
        vehicle::VehicleRepository,
    },
};

use super::WorkflowError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateInvoice {
    pub customer_id: CustomerId,
    pub vehicle_id: Option<VehicleId>,
    pub intervention_id: Option<InterventionId>,
    pub currency: CurrencyCode,
    pub notes: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UpdateInvoice {
    pub customer_id: Option<CustomerId>,
    pub vehicle_id: Option<Option<VehicleId>>,
    pub intervention_id: Option<Option<InterventionId>>,
    pub currency: Option<CurrencyCode>,
    pub notes: Option<Option<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WriteInvoiceLine {
    pub source_intervention_line_id: Option<InterventionLineId>,
    pub description: String,
    pub quantity: Quantity,
    pub unit_label: String,
    pub unit_price_minor: i64,
    pub position: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueInvoiceCommand {
    pub issue_date: NaiveDate,
    pub due_date: Option<NaiveDate>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordPayment {
    pub amount_minor: i64,
    pub currency: CurrencyCode,
    pub received_at: DateTime<Utc>,
    pub method: PaymentMethod,
    pub reference: Option<String>,
    pub notes: Option<String>,
}

#[derive(Clone)]
pub struct InvoiceService {
    invoices: Arc<dyn InvoiceRepository>,
    customers: Arc<dyn CustomerRepository>,
    vehicles: Arc<dyn VehicleRepository>,
    interventions: Arc<dyn InterventionRepository>,
    cursors: CursorCodec,
    resource: CursorResource,
}

impl InvoiceService {
    pub fn new(
        invoices: Arc<dyn InvoiceRepository>,
        customers: Arc<dyn CustomerRepository>,
        vehicles: Arc<dyn VehicleRepository>,
        interventions: Arc<dyn InterventionRepository>,
        cursors: CursorCodec,
    ) -> Self {
        Self {
            invoices,
            customers,
            vehicles,
            interventions,
            cursors,
            resource: CursorResource::parse("invoices").expect("static resource is valid"),
        }
    }

    pub async fn create(&self, command: CreateInvoice) -> Result<InvoiceView, WorkflowError> {
        self.validate_relationships(
            &command.customer_id,
            command.vehicle_id.as_ref(),
            command.intervention_id.as_ref(),
        )
        .await?;
        let invoice = Invoice::new(
            command.customer_id,
            command.vehicle_id,
            command.intervention_id,
            command.currency,
            command.notes,
            Utc::now(),
        )
        .map_err(invoice_validation)?;
        let record = self.invoices.create(&invoice).await?;
        self.get(&record.id).await
    }

    pub async fn get(&self, id: &InvoiceId) -> Result<InvoiceView, WorkflowError> {
        self.invoices.view(id).await?.ok_or(WorkflowError::NotFound)
    }

    pub async fn list(
        &self,
        request: PageRequest<InvoiceFilter>,
    ) -> Result<Page<InvoiceView>, WorkflowError> {
        let filter = request.filter;
        let after = request
            .after
            .as_ref()
            .map(|cursor| self.cursors.decode(cursor, &self.resource, &filter))
            .transpose()
            .map_err(|_| invalid_cursor())?;
        let page = self
            .invoices
            .list(&filter, request.limit, after.as_ref())
            .await?;
        let next_cursor = page
            .next
            .as_ref()
            .map(|tuple| self.cursors.encode(&self.resource, tuple, &filter))
            .transpose()
            .map_err(|_| WorkflowError::Internal)?;
        Ok(Page {
            items: page.items,
            next_cursor,
        })
    }

    pub async fn update(
        &self,
        id: &InvoiceId,
        command: UpdateInvoice,
    ) -> Result<InvoiceView, WorkflowError> {
        let current = self.require_draft(id).await?;
        let invoice = Invoice::new(
            command
                .customer_id
                .unwrap_or(current.invoice.customer_id.clone()),
            command
                .vehicle_id
                .unwrap_or(current.invoice.vehicle_id.clone()),
            command
                .intervention_id
                .unwrap_or(current.invoice.intervention_id.clone()),
            command.currency.unwrap_or(current.invoice.currency),
            command.notes.unwrap_or(current.invoice.notes.clone()),
            current.invoice.created_at,
        )
        .map_err(invoice_validation)?;
        self.validate_relationships(
            &invoice.customer_id,
            invoice.vehicle_id.as_ref(),
            invoice.intervention_id.as_ref(),
        )
        .await?;
        self.invoices
            .update_draft(id, &DraftInvoiceUpdate { invoice })
            .await?;
        self.get(id).await
    }

    pub async fn create_line(
        &self,
        id: &InvoiceId,
        command: WriteInvoiceLine,
    ) -> Result<InvoiceLineMutationResult, WorkflowError> {
        let invoice = self.require_draft(id).await?;
        self.validate_source_line(&invoice, command.source_intervention_line_id.as_ref())
            .await?;
        let line = validate_line(id, command, invoice.invoice.currency)?;
        self.invoices
            .mutate_line(id, InvoiceLineMutation::Create(line))
            .await
            .map_err(Into::into)
    }

    pub async fn update_line(
        &self,
        id: &InvoiceId,
        line_id: InvoiceLineId,
        command: WriteInvoiceLine,
    ) -> Result<InvoiceLineMutationResult, WorkflowError> {
        let invoice = self.require_draft(id).await?;
        self.validate_source_line(&invoice, command.source_intervention_line_id.as_ref())
            .await?;
        let line = validate_line(id, command, invoice.invoice.currency)?;
        self.invoices
            .mutate_line(id, InvoiceLineMutation::Update { id: line_id, line })
            .await
            .map_err(Into::into)
    }

    pub async fn delete_line(
        &self,
        id: &InvoiceId,
        line_id: InvoiceLineId,
    ) -> Result<InvoiceLineMutationResult, WorkflowError> {
        self.require_draft(id).await?;
        self.invoices
            .mutate_line(id, InvoiceLineMutation::Delete { id: line_id })
            .await
            .map_err(Into::into)
    }

    pub async fn move_line(
        &self,
        id: &InvoiceId,
        line_id: InvoiceLineId,
        direction: InvoiceLineMoveDirection,
    ) -> Result<InvoiceLineMutationResult, WorkflowError> {
        self.require_draft(id).await?;
        self.invoices
            .mutate_line(
                id,
                InvoiceLineMutation::Move {
                    id: line_id,
                    direction,
                },
            )
            .await
            .map_err(Into::into)
    }

    pub async fn list_lines(
        &self,
        id: &InvoiceId,
    ) -> Result<Vec<crate::models::invoice_line::InvoiceLineRecord>, WorkflowError> {
        self.invoices.list_lines(id).await.map_err(Into::into)
    }

    pub async fn issue(
        &self,
        id: &InvoiceId,
        command: IssueInvoiceCommand,
    ) -> Result<InvoiceView, WorkflowError> {
        let current = self.require_draft(id).await?;
        if command
            .due_date
            .is_some_and(|due_date| due_date < command.issue_date)
        {
            return Err(validation(
                "due_date",
                ValidationCode::InvalidFormat,
                "Due date cannot precede the issue date.",
            ));
        }
        let customer = self.require_customer(&current.invoice.customer_id).await?;
        let billing_address_snapshot = customer
            .address
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
            .map_err(invoice_validation)?;
        self.invoices
            .issue(
                id,
                &IssueInvoice {
                    issue_date: command.issue_date,
                    due_date: command.due_date,
                    customer_display_snapshot: customer.display_name,
                    billing_address_snapshot,
                    issued_at: Utc::now(),
                },
            )
            .await
            .map_err(Into::into)
    }

    pub async fn void(&self, id: &InvoiceId, reason: String) -> Result<InvoiceView, WorkflowError> {
        let current = self.get(id).await?;
        let mut candidate = current.invoice.invoice.clone();
        candidate
            .void(current.paid, reason.clone(), Utc::now())
            .map_err(invoice_validation)?;
        self.invoices
            .void(id, reason.trim())
            .await
            .map_err(Into::into)
    }

    pub async fn record_payment(
        &self,
        id: &InvoiceId,
        command: RecordPayment,
        created_by: UserId,
    ) -> Result<PaymentMutationResult, WorkflowError> {
        let current = self.get(id).await?;
        let amount = Money::new(command.amount_minor, command.currency)
            .map_err(|_| invalid_money("amount_minor"))?;
        let payment = Payment::new(
            id.clone(),
            current.invoice.invoice.status,
            current.invoice.invoice.total,
            &[],
            amount,
            command.received_at,
            command.method,
            command.reference,
            command.notes,
            Utc::now(),
            created_by,
        )
        .map_err(payment_validation)?;
        self.invoices
            .record_payment(id, &payment)
            .await
            .map_err(Into::into)
    }

    pub async fn list_payments(&self, id: &InvoiceId) -> Result<Vec<PaymentRecord>, WorkflowError> {
        self.invoices.list_payments(id).await.map_err(Into::into)
    }

    pub async fn get_payment(&self, id: &PaymentId) -> Result<PaymentRecord, WorkflowError> {
        self.invoices
            .find_payment(id)
            .await?
            .ok_or(WorkflowError::NotFound)
    }

    async fn require_draft(&self, id: &InvoiceId) -> Result<InvoiceRecord, WorkflowError> {
        let record = self
            .invoices
            .find_by_id(id)
            .await?
            .ok_or(WorkflowError::NotFound)?;
        if record.invoice.status != InvoiceStatus::Draft {
            return Err(WorkflowError::Conflict);
        }
        Ok(record)
    }

    async fn require_customer(&self, id: &CustomerId) -> Result<Customer, WorkflowError> {
        self.customers
            .find_by_id(id)
            .await?
            .ok_or(WorkflowError::NotFound)
    }

    async fn validate_relationships(
        &self,
        customer_id: &CustomerId,
        vehicle_id: Option<&VehicleId>,
        intervention_id: Option<&InterventionId>,
    ) -> Result<(), WorkflowError> {
        let customer = self.require_customer(customer_id).await?;
        if customer.is_archived() {
            return Err(WorkflowError::Conflict);
        }
        let vehicle = if let Some(vehicle_id) = vehicle_id {
            let vehicle = self
                .vehicles
                .find_by_id(vehicle_id)
                .await?
                .ok_or(WorkflowError::NotFound)?;
            if vehicle.is_archived() || vehicle.customer_id != *customer_id {
                return Err(WorkflowError::Conflict);
            }
            Some(vehicle)
        } else {
            None
        };
        if let Some(intervention_id) = intervention_id {
            let vehicle = vehicle.as_ref().ok_or(WorkflowError::Conflict)?;
            let intervention = self
                .interventions
                .find_by_id(intervention_id)
                .await?
                .ok_or(WorkflowError::NotFound)?;
            if intervention.vehicle_id != vehicle.id
                || intervention.status == InterventionStatus::Cancelled
            {
                return Err(WorkflowError::Conflict);
            }
        }
        Ok(())
    }

    async fn validate_source_line(
        &self,
        invoice: &InvoiceRecord,
        source_id: Option<&InterventionLineId>,
    ) -> Result<(), WorkflowError> {
        let Some(source_id) = source_id else {
            return Ok(());
        };
        let intervention_id = invoice
            .invoice
            .intervention_id
            .as_ref()
            .ok_or(WorkflowError::Conflict)?;
        let found = self
            .interventions
            .list_lines(intervention_id)
            .await?
            .into_iter()
            .any(|line| &line.id == source_id);
        if found {
            Ok(())
        } else {
            Err(WorkflowError::Conflict)
        }
    }
}

fn validate_line(
    invoice_id: &InvoiceId,
    command: WriteInvoiceLine,
    currency: CurrencyCode,
) -> Result<InvoiceLine, WorkflowError> {
    let unit_price = Money::new(command.unit_price_minor, currency)
        .map_err(|_| invalid_money("unit_price_minor"))?;
    InvoiceLine::new(
        invoice_id.clone(),
        command.source_intervention_line_id,
        command.description,
        command.quantity,
        command.unit_label,
        unit_price,
        command.position,
        currency,
    )
    .map_err(line_validation)
}

fn invoice_validation(error: InvoiceError) -> WorkflowError {
    match error {
        InvoiceError::Required => validation(
            "invoice",
            ValidationCode::Required,
            "Enter the required invoice value.",
        ),
        InvoiceError::TooLong => validation(
            "invoice",
            ValidationCode::TooLong,
            "Shorten the submitted invoice value.",
        ),
        InvoiceError::InvalidDueDate => validation(
            "due_date",
            ValidationCode::InvalidFormat,
            "Due date cannot precede the issue date.",
        ),
        InvoiceError::InvalidCountryCode => validation(
            "billing_address",
            ValidationCode::InvalidFormat,
            "The billing address is invalid.",
        ),
        InvoiceError::Money(_) => invalid_money("invoice"),
        InvoiceError::InvalidTransition
        | InvoiceError::Immutable
        | InvoiceError::CurrencyMismatch
        | InvoiceError::PaymentsRecorded
        | InvoiceError::Overpayment => WorkflowError::Conflict,
        InvoiceError::InvalidIssueNumber => WorkflowError::Internal,
    }
}

fn line_validation(error: InvoiceLineError) -> WorkflowError {
    match error {
        InvoiceLineError::Required => validation(
            "line",
            ValidationCode::Required,
            "Enter the required line details.",
        ),
        InvoiceLineError::TooLong => validation(
            "line",
            ValidationCode::TooLong,
            "Shorten the submitted line value.",
        ),
        InvoiceLineError::CurrencyMismatch => WorkflowError::Conflict,
        InvoiceLineError::Money(_) => invalid_money("line"),
    }
}

fn payment_validation(error: PaymentError) -> WorkflowError {
    match error {
        PaymentError::NotPositive => validation(
            "amount_minor",
            ValidationCode::InvalidFormat,
            "Enter a positive payment amount.",
        ),
        PaymentError::CurrencyMismatch
        | PaymentError::InvoiceNotIssued
        | PaymentError::Invoice(InvoiceError::Overpayment) => WorkflowError::Conflict,
        PaymentError::Invoice(error) => invoice_validation(error),
    }
}

fn invalid_money(field: &str) -> WorkflowError {
    validation(
        field,
        ValidationCode::InvalidFormat,
        "Enter a supported non-negative amount.",
    )
}

fn invalid_cursor() -> WorkflowError {
    validation(
        "cursor",
        ValidationCode::InvalidFormat,
        "Use the cursor returned by this invoice query.",
    )
}

fn validation(field: &str, code: ValidationCode, message: &str) -> WorkflowError {
    WorkflowError::Validation(ValidationErrors::one(
        ValidationError::new(field, code, message).expect("static validation metadata is valid"),
    ))
}
