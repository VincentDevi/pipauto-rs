//! Invoice data, lifecycle, lines, payments, operations, and private persistence.

mod domain;
pub mod line;
mod operations;
pub mod payment;
pub(crate) mod persistence;
pub(crate) mod repository;

pub(crate) use crate::models::ModelError as WorkflowError;
pub use domain::*;
pub use operations::{
    CreateInvoice, InvoiceModel, IssueInvoiceCommand, RecordPayment, UpdateInvoice,
    WriteInvoiceLine,
};
pub use repository::{
    InvoiceFilter, InvoiceLineMoveDirection, InvoiceLineMutationResult, PaymentMutationResult,
};

use crate::models::{ModelContext, ModelError};

impl InvoiceView {
    /// Load this invoice's explicitly ordered snapshot lines.
    pub async fn lines(
        &self,
        context: &ModelContext,
    ) -> Result<Vec<line::InvoiceLineRecord>, ModelError> {
        InvoiceModel::from_context(context)?
            .list_lines(&self.invoice.id)
            .await
    }

    /// Load append-only payments for this invoice.
    pub async fn payments(
        &self,
        context: &ModelContext,
    ) -> Result<Vec<payment::PaymentRecord>, ModelError> {
        InvoiceModel::from_context(context)?
            .list_payments(&self.invoice.id)
            .await
    }
}
