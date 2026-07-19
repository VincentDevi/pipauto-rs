//! Persistence-neutral invoice, invoice-line, and payment contracts.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};

use crate::{
    domain::{CollectionFilter, CursorTuple, InvoiceId, InvoiceLineId, PageLimit, PaymentId},
    models::{
        invoice::{BillingAddressSnapshot, Invoice, InvoiceRecord, InvoiceStatus, InvoiceView},
        invoice_line::{InvoiceLine, InvoiceLineRecord},
        payment::{Payment, PaymentRecord},
    },
};

use super::{customer::RepositoryPage, RepositoryError};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InvoiceFilter {
    pub status: Option<InvoiceStatus>,
}

impl CollectionFilter for InvoiceFilter {
    fn fingerprint_bytes(&self) -> Vec<u8> {
        format!("invoices:v1:{:?}", self.status).into_bytes()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DraftInvoiceUpdate {
    pub invoice: Invoice,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InvoiceLineMutation {
    Create(InvoiceLine),
    Update {
        id: InvoiceLineId,
        line: InvoiceLine,
    },
    Delete {
        id: InvoiceLineId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceLineMutationResult {
    pub line: Option<InvoiceLineRecord>,
    pub invoice: InvoiceRecord,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IssueInvoice {
    pub issue_date: NaiveDate,
    pub due_date: Option<NaiveDate>,
    pub customer_display_snapshot: String,
    pub billing_address_snapshot: Option<BillingAddressSnapshot>,
    pub issued_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentMutationResult {
    pub payment: PaymentRecord,
    pub invoice: InvoiceView,
}

#[async_trait]
pub trait InvoiceRepository: Send + Sync {
    async fn create(&self, invoice: &Invoice) -> Result<InvoiceRecord, RepositoryError>;
    async fn find_by_id(&self, id: &InvoiceId) -> Result<Option<InvoiceRecord>, RepositoryError>;
    async fn view(&self, id: &InvoiceId) -> Result<Option<InvoiceView>, RepositoryError>;
    async fn list(
        &self,
        filter: &InvoiceFilter,
        limit: PageLimit,
        after: Option<&CursorTuple>,
    ) -> Result<RepositoryPage<InvoiceView>, RepositoryError>;
    async fn update_draft(
        &self,
        id: &InvoiceId,
        update: &DraftInvoiceUpdate,
    ) -> Result<InvoiceRecord, RepositoryError>;
    async fn mutate_line(
        &self,
        invoice_id: &InvoiceId,
        mutation: InvoiceLineMutation,
    ) -> Result<InvoiceLineMutationResult, RepositoryError>;
    async fn list_lines(
        &self,
        invoice_id: &InvoiceId,
    ) -> Result<Vec<InvoiceLineRecord>, RepositoryError>;
    async fn issue(
        &self,
        id: &InvoiceId,
        command: &IssueInvoice,
    ) -> Result<InvoiceView, RepositoryError>;
    async fn void(&self, id: &InvoiceId, reason: &str) -> Result<InvoiceView, RepositoryError>;
    async fn record_payment(
        &self,
        invoice_id: &InvoiceId,
        payment: &Payment,
    ) -> Result<PaymentMutationResult, RepositoryError>;
    async fn list_payments(
        &self,
        invoice_id: &InvoiceId,
    ) -> Result<Vec<PaymentRecord>, RepositoryError>;
    async fn find_payment(&self, id: &PaymentId) -> Result<Option<PaymentRecord>, RepositoryError>;
}
