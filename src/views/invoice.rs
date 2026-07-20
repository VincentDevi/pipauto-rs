//! Presentation-safe invoice draft and line-item browser models.

use loco_rs::{
    controller::views::{engines::TeraView, ViewRenderer},
    Result,
};
use serde::{Deserialize, Serialize};

use crate::{
    controllers::browser::forms::FormState,
    domain::{Money, Page},
    models::{
        customer::Customer,
        intervention::{Intervention, InterventionStatus},
        intervention_line::InterventionLine,
        invoice::{InvoiceStatus, InvoiceView, PaymentStatus},
        invoice_line::InvoiceLineRecord,
        vehicle::Vehicle,
    },
};

use super::layout::AuthenticatedLayout;

const LIST_PAGE: &str = "pages/invoices.html";
const LIST_FRAGMENT: &str = "fragments/invoice_list.html";
const FORM_PAGE: &str = "pages/invoice_form.html";
const FORM_FRAGMENT: &str = "fragments/invoice_form.html";
const DETAIL_PAGE: &str = "pages/invoice_detail.html";
const DETAIL_FRAGMENT: &str = "fragments/invoice_detail.html";
const LINE_FORM_PAGE: &str = "pages/invoice_line_form.html";
const LINE_FORM_FRAGMENT: &str = "fragments/invoice_line_form.html";
const LINE_REGION_FRAGMENT: &str = "fragments/invoice_line_region.html";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct InvoiceFilterValues {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub cursor: String,
}

#[derive(Debug, Serialize)]
struct InvoiceListItem {
    reference: String,
    internal_reference: Option<String>,
    customer: String,
    issue_date: Option<String>,
    due_date: Option<String>,
    lifecycle: &'static str,
    lifecycle_class: &'static str,
    total: String,
    paid: String,
    outstanding: String,
    payment_state: &'static str,
    payment_class: &'static str,
    href: String,
}

#[derive(Debug, Serialize)]
pub struct InvoiceListPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    filters: InvoiceFilterValues,
    items: Vec<InvoiceListItem>,
    next_href: Option<String>,
    filter_error: Option<String>,
}

impl<'page> InvoiceListPage<'page> {
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        mut filters: InvoiceFilterValues,
        page: Page<InvoiceView>,
        draft_customers: Vec<Option<Customer>>,
        next_href: Option<String>,
        filter_error: Option<String>,
    ) -> Self {
        filters.cursor.clear();
        let items = page
            .items
            .into_iter()
            .zip(draft_customers)
            .map(|(invoice, customer)| list_item(invoice, customer))
            .collect();
        Self {
            layout,
            title: "Invoices · Pipauto",
            filters,
            items,
            next_href,
            filter_error,
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(LIST_PAGE, self)
    }

    pub fn render_content(&self, engine: &TeraView) -> Result<String> {
        engine.render(LIST_FRAGMENT, self)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct InvoiceFormValues {
    #[serde(default)]
    pub customer_id: String,
    #[serde(default)]
    pub vehicle_id: String,
    #[serde(default)]
    pub intervention_id: String,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Serialize)]
struct SelectOption {
    id: String,
    label: String,
    selected: bool,
    unavailable: bool,
    owner_id: Option<String>,
    vehicle_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InvoiceFormPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    heading: &'static str,
    action: String,
    submit_label: &'static str,
    cancel_href: String,
    form: FormState<InvoiceFormValues>,
    customers: Vec<SelectOption>,
    vehicles: Vec<SelectOption>,
    interventions: Vec<SelectOption>,
    conflict: Option<String>,
}

impl<'page> InvoiceFormPage<'page> {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        invoice_id: Option<&str>,
        form: FormState<InvoiceFormValues>,
        customers: Vec<Customer>,
        vehicles: Vec<Vehicle>,
        interventions: Vec<Intervention>,
        conflict: Option<String>,
    ) -> Self {
        let editing = invoice_id.is_some();
        let values = &form.values;
        Self {
            layout,
            title: if editing {
                "Edit invoice draft · Pipauto"
            } else {
                "New invoice draft · Pipauto"
            },
            heading: if editing {
                "Edit invoice draft"
            } else {
                "New invoice draft"
            },
            action: invoice_id.map_or_else(
                || "/invoices".to_owned(),
                |id| format!("/invoices/{id}/edit"),
            ),
            submit_label: if editing {
                "Save changes"
            } else {
                "Create draft"
            },
            cancel_href: invoice_id
                .map_or_else(|| "/invoices".to_owned(), |id| format!("/invoices/{id}")),
            customers: customers
                .into_iter()
                .map(|customer| {
                    let unavailable = customer.is_archived();
                    SelectOption {
                        selected: values.customer_id == customer.id.as_str(),
                        id: customer.id.as_str().to_owned(),
                        label: customer.display_name,
                        unavailable,
                        owner_id: None,
                        vehicle_id: None,
                    }
                })
                .collect(),
            vehicles: vehicles
                .into_iter()
                .map(|vehicle| SelectOption {
                    selected: values.vehicle_id == vehicle.id.as_str(),
                    id: vehicle.id.as_str().to_owned(),
                    label: vehicle_label(&vehicle),
                    unavailable: vehicle.is_archived(),
                    owner_id: Some(vehicle.customer_id.as_str().to_owned()),
                    vehicle_id: None,
                })
                .collect(),
            interventions: interventions
                .into_iter()
                .map(|intervention| SelectOption {
                    selected: values.intervention_id == intervention.id.as_str(),
                    id: intervention.id.as_str().to_owned(),
                    label: format!(
                        "{} · {}",
                        intervention.service_date,
                        intervention_status(intervention.status)
                    ),
                    unavailable: intervention.status == InterventionStatus::Cancelled,
                    owner_id: None,
                    vehicle_id: Some(intervention.vehicle_id.as_str().to_owned()),
                })
                .collect(),
            form,
            conflict,
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(FORM_PAGE, self)
    }

    pub fn render_form(&self, engine: &TeraView) -> Result<String> {
        engine.render(FORM_FRAGMENT, self)
    }
}

#[derive(Debug, Serialize)]
struct InvoiceLineItem {
    description: String,
    source_reference: Option<String>,
    quantity: String,
    unit_label: String,
    unit_price: String,
    line_total: String,
    position: u32,
    edit_href: String,
    delete_action: String,
    move_up_action: String,
    move_down_action: String,
    can_move_up: bool,
    can_move_down: bool,
}

#[derive(Debug, Serialize)]
pub struct InvoiceDetailPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    id: String,
    customer: String,
    customer_href: String,
    vehicle: Option<String>,
    vehicle_href: Option<String>,
    intervention: Option<String>,
    intervention_href: Option<String>,
    currency: String,
    notes: Option<String>,
    lines: Vec<InvoiceLineItem>,
    subtotal: String,
    total: String,
    created_at: String,
    updated_at: String,
    conflict: Option<String>,
}

impl<'page> InvoiceDetailPage<'page> {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        view: InvoiceView,
        customer: Customer,
        vehicle: Option<Vehicle>,
        intervention: Option<Intervention>,
        conflict: Option<String>,
    ) -> Self {
        let invoice = view.invoice.invoice;
        let id = view.invoice.id.as_str().to_owned();
        Self {
            layout,
            title: "Invoice draft · Pipauto",
            customer: customer.display_name,
            customer_href: format!("/customers/{}", customer.id.as_str()),
            vehicle: vehicle.as_ref().map(vehicle_label),
            vehicle_href: vehicle
                .as_ref()
                .map(|value| format!("/vehicles/{}", value.id.as_str())),
            intervention: intervention
                .as_ref()
                .map(|value| value.service_date.to_string()),
            intervention_href: intervention
                .as_ref()
                .map(|value| format!("/interventions/{}", value.id.as_str())),
            currency: invoice.currency.as_str().to_owned(),
            notes: invoice.notes,
            lines: line_items(view.lines, &id),
            subtotal: money(invoice.subtotal),
            total: money(invoice.total),
            created_at: timestamp(invoice.created_at),
            updated_at: timestamp(invoice.updated_at),
            id,
            conflict,
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(DETAIL_PAGE, self)
    }

    pub fn render_content(&self, engine: &TeraView) -> Result<String> {
        engine.render(DETAIL_FRAGMENT, self)
    }

    pub fn render_lines(&self, engine: &TeraView) -> Result<String> {
        engine.render(LINE_REGION_FRAGMENT, self)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct InvoiceLineFormValues {
    #[serde(default)]
    pub source_intervention_line_id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub quantity: String,
    #[serde(default)]
    pub unit_label: String,
    #[serde(default)]
    pub unit_price: String,
    #[serde(default)]
    pub position: String,
}

impl From<&InvoiceLineRecord> for InvoiceLineFormValues {
    fn from(value: &InvoiceLineRecord) -> Self {
        Self {
            source_intervention_line_id: value
                .line
                .source_intervention_line_id
                .as_ref()
                .map_or_else(String::new, |id| id.as_str().to_owned()),
            description: value.line.description.clone(),
            quantity: value.line.quantity.to_string(),
            unit_label: value.line.unit_label.clone(),
            unit_price: money_input(value.line.unit_price),
            position: value.line.position.to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
struct SourceLineOption {
    id: String,
    label: String,
    selected: bool,
}

#[derive(Debug, Serialize)]
pub struct InvoiceLineFormPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    heading: &'static str,
    action: String,
    submit_label: &'static str,
    cancel_href: String,
    currency: String,
    form: FormState<InvoiceLineFormValues>,
    source_lines: Vec<SourceLineOption>,
    conflict: Option<String>,
}

impl<'page> InvoiceLineFormPage<'page> {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        invoice_id: &str,
        line_id: Option<&str>,
        currency: &str,
        form: FormState<InvoiceLineFormValues>,
        source_lines: Vec<InterventionLine>,
        conflict: Option<String>,
    ) -> Self {
        let editing = line_id.is_some();
        let selected = form.values.source_intervention_line_id.clone();
        let action = line_id.map_or_else(
            || format!("/invoices/{invoice_id}/lines"),
            |line_id| format!("/invoices/{invoice_id}/lines/{line_id}/edit"),
        );
        Self {
            layout,
            title: if editing {
                "Edit invoice line · Pipauto"
            } else {
                "Add invoice line · Pipauto"
            },
            heading: if editing {
                "Edit invoice line"
            } else {
                "Add invoice line"
            },
            action,
            submit_label: if editing { "Save line" } else { "Add line" },
            cancel_href: format!("/invoices/{invoice_id}"),
            currency: currency.to_owned(),
            source_lines: source_lines
                .into_iter()
                .map(|line| SourceLineOption {
                    selected: selected == line.id.as_str(),
                    id: line.id.as_str().to_owned(),
                    label: format!(
                        "{} · {} {}",
                        line.description, line.quantity, line.unit_label
                    ),
                })
                .collect(),
            form,
            conflict,
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(LINE_FORM_PAGE, self)
    }

    pub fn render_form(&self, engine: &TeraView) -> Result<String> {
        engine.render(LINE_FORM_FRAGMENT, self)
    }
}

fn list_item(view: InvoiceView, draft_customer: Option<Customer>) -> InvoiceListItem {
    let invoice = view.invoice.invoice;
    let draft = invoice.status == InvoiceStatus::Draft;
    let (lifecycle, lifecycle_class) = lifecycle(invoice.status);
    let (payment_state, payment_class) = payment_status(view.payment_status);
    InvoiceListItem {
        reference: invoice
            .number
            .as_ref()
            .map_or_else(|| "Draft".to_owned(), |number| number.as_str().to_owned()),
        internal_reference: draft.then(|| view.invoice.id.as_str().to_owned()),
        customer: invoice.customer_display_snapshot.unwrap_or_else(|| {
            draft_customer.map_or_else(|| "Unavailable customer".to_owned(), |c| c.display_name)
        }),
        issue_date: invoice.issue_date.map(|date| date.to_string()),
        due_date: invoice.due_date.map(|date| date.to_string()),
        lifecycle,
        lifecycle_class,
        total: money(invoice.total),
        paid: money(view.paid),
        outstanding: money(view.outstanding),
        payment_state,
        payment_class,
        href: format!("/invoices/{}", view.invoice.id.as_str()),
    }
}

fn line_items(lines: Vec<InvoiceLineRecord>, invoice_id: &str) -> Vec<InvoiceLineItem> {
    let last = lines.len().saturating_sub(1);
    lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let base = format!("/invoices/{invoice_id}/lines/{}", line.id.as_str());
            InvoiceLineItem {
                description: line.line.description,
                source_reference: line
                    .line
                    .source_intervention_line_id
                    .map(|id| id.as_str().to_owned()),
                quantity: line.line.quantity.to_string(),
                unit_label: line.line.unit_label,
                unit_price: money(line.line.unit_price),
                line_total: money(line.line.line_total),
                position: line.line.position,
                edit_href: format!("{base}/edit"),
                delete_action: format!("{base}/delete"),
                move_up_action: format!("{base}/move-up"),
                move_down_action: format!("{base}/move-down"),
                can_move_up: index > 0,
                can_move_down: index < last,
            }
        })
        .collect()
}

fn lifecycle(value: InvoiceStatus) -> (&'static str, &'static str) {
    match value {
        InvoiceStatus::Draft => ("Draft", "warning"),
        InvoiceStatus::Issued => ("Issued", "success"),
        InvoiceStatus::Void => ("Void", "error"),
    }
}

fn payment_status(value: PaymentStatus) -> (&'static str, &'static str) {
    match value {
        PaymentStatus::Unpaid => ("Unpaid", "warning"),
        PaymentStatus::PartiallyPaid => ("Partially paid", "warning"),
        PaymentStatus::Paid => ("Paid", "success"),
    }
}

fn intervention_status(value: InterventionStatus) -> &'static str {
    match value {
        InterventionStatus::Draft => "Draft",
        InterventionStatus::Completed => "Completed",
        InterventionStatus::Cancelled => "Cancelled",
    }
}

fn vehicle_label(vehicle: &Vehicle) -> String {
    format!(
        "{} · {} {}",
        vehicle.registration.as_deref().unwrap_or("No registration"),
        vehicle.make,
        vehicle.model
    )
}

fn money(value: Money) -> String {
    let minor = value.minor_units();
    format!(
        "{} {}.{:02}",
        value.currency().as_str(),
        minor / 100,
        minor % 100
    )
}

fn money_input(value: Money) -> String {
    let minor = value.minor_units();
    format!("{}.{:02}", minor / 100, minor % 100)
}

fn timestamp(value: chrono::DateTime<chrono::Utc>) -> String {
    value.format("%d %b %Y, %H:%M UTC").to_string()
}
