//! Server-rendered invoice discovery, draft, and ordered line workflows.

use axum::{extract::Query, http::StatusCode, response::Response};
use chrono::{NaiveDate, NaiveDateTime};
use loco_rs::{
    controller::{
        extractor::shared_store::SharedStore, views::engines::TeraView, views::ViewEngine, Routes,
    },
    prelude::{get, post, Path},
    Result,
};
use serde::Deserialize;

use crate::{
    controllers::browser::{
        context::{BrowserRequestContext, ResponsePreference},
        forms::{
            body_limit, invalid_format_error as validation_error, optional_text,
            parse_minor_units as parse_money_input, AuthenticatedForm, FormState,
        },
        responses,
    },
    domain::{
        CurrencyCode, CustomerId, InterventionId, InterventionLineId, InvoiceId, InvoiceLineId,
        OpaqueCursor, Page, PageLimit, PageRequest, Quantity, ValidationError, ValidationErrors,
        VehicleId,
    },
    models::{
        customer::{ArchiveFilter, Customer, CustomerFilter, CustomerModel as CustomerService},
        intervention::{
            Intervention, InterventionFilter, InterventionModel as InterventionService,
        },
        invoice::{
            CreateInvoice, InvoiceFilter, InvoiceLineMoveDirection, InvoiceModel as InvoiceService,
            InvoiceStatus, InvoiceView, IssueInvoiceCommand, RecordPayment, UpdateInvoice,
            WriteInvoiceLine, NOTES_MAX_CHARS,
        },
        invoice_line::{DESCRIPTION_MAX_CHARS, UNIT_LABEL_MAX_CHARS},
        payment::PaymentMethod,
        vehicle::{Vehicle, VehicleFilter, VehicleModel as VehicleService},
        ModelError as WorkflowError,
    },
    settings::BusinessSettings,
    views::invoice::{
        InvoiceDetailPage, InvoiceFilterValues, InvoiceFormPage, InvoiceFormValues,
        InvoiceLineFormPage, InvoiceLineFormValues, InvoiceListPage, IssueInvoiceFormValues,
        IssueInvoicePage, PaymentFormPage, PaymentFormValues, VoidInvoiceFormValues,
        VoidInvoicePage,
    },
};

mod crud;
mod forms;
mod lifecycle;
mod lines;
mod payments;

use crud::*;
use lifecycle::*;
use lines::*;
use payments::*;

pub fn routes() -> Routes {
    Routes::new()
        .add("/invoices", get(list))
        .add("/invoices", post(create).layer(body_limit()))
        .add("/invoices/new", get(new_form))
        .add("/invoices/{id}", get(show))
        .add("/invoices/{id}/issue", get(issue_form))
        .add(
            "/invoices/{id}/issue",
            post(issue_invoice).layer(body_limit()),
        )
        .add("/invoices/{id}/payments/new", get(payment_form))
        .add(
            "/invoices/{id}/payments",
            post(record_payment).layer(body_limit()),
        )
        .add("/invoices/{id}/void", get(void_form))
        .add(
            "/invoices/{id}/void",
            post(void_invoice).layer(body_limit()),
        )
        .add("/invoices/{id}/edit", get(edit_form))
        .add("/invoices/{id}/edit", post(update).layer(body_limit()))
        .add("/invoices/{id}/lines/new", get(new_line_form))
        .add(
            "/invoices/{id}/lines",
            post(create_line).layer(body_limit()),
        )
        .add("/invoices/{id}/lines/{line_id}/edit", get(edit_line_form))
        .add(
            "/invoices/{id}/lines/{line_id}/edit",
            post(update_line).layer(body_limit()),
        )
        .add(
            "/invoices/{id}/lines/{line_id}/delete",
            post(delete_line).layer(body_limit()),
        )
        .add(
            "/invoices/{id}/lines/{line_id}/move-up",
            post(move_line_up).layer(body_limit()),
        )
        .add(
            "/invoices/{id}/lines/{line_id}/move-down",
            post(move_line_down).layer(body_limit()),
        )
}
