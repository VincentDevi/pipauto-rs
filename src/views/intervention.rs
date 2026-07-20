//! Presentation-safe intervention browser models.

use loco_rs::{
    controller::views::{engines::TeraView, ViewRenderer},
    Result,
};
use serde::{Deserialize, Serialize};

use crate::{
    controllers::browser::forms::FormState,
    domain::{Money, Page},
    models::{
        attachment::AttachmentMetadata,
        customer::Customer,
        intervention::{Intervention, InterventionStatus, ServiceHistorySummary},
        intervention_line::{InterventionLine, InterventionLineCategory},
        vehicle::Vehicle,
    },
};

use super::layout::AuthenticatedLayout;

const LIST_PAGE: &str = "pages/interventions.html";
const LIST_FRAGMENT: &str = "fragments/intervention_list.html";
const FORM_PAGE: &str = "pages/intervention_form.html";
const FORM_FRAGMENT: &str = "fragments/intervention_form.html";
const DETAIL_PAGE: &str = "pages/intervention_detail.html";
const DETAIL_FRAGMENT: &str = "fragments/intervention_detail.html";
const TRANSITION_PAGE: &str = "pages/intervention_transition.html";
const TRANSITION_FRAGMENT: &str = "fragments/intervention_transition.html";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct InterventionFilterValues {
    #[serde(default)]
    pub vehicle: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub cursor: String,
}

#[derive(Debug, Serialize)]
struct VehicleOption {
    id: String,
    label: String,
    selected: bool,
    active: bool,
}

#[derive(Debug, Serialize)]
struct ListItem {
    date: String,
    vehicle: String,
    registration: String,
    mileage: Option<u64>,
    summary: String,
    status: &'static str,
    status_class: &'static str,
    total: String,
    href: String,
}

#[derive(Debug, Serialize)]
pub struct InterventionListPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    filters: InterventionFilterValues,
    vehicles: Vec<VehicleOption>,
    items: Vec<ListItem>,
    next_href: Option<String>,
    filter_error: Option<String>,
    has_active_vehicles: bool,
}

impl<'page> InterventionListPage<'page> {
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        filters: InterventionFilterValues,
        page: Page<ServiceHistorySummary>,
        row_vehicles: Vec<Vehicle>,
        filter_vehicles: Vec<Vehicle>,
        next_href: Option<String>,
        filter_error: Option<String>,
    ) -> Self {
        let selected = filters.vehicle.clone();
        let items = page
            .items
            .into_iter()
            .zip(row_vehicles)
            .map(|(entry, vehicle)| list_item(entry, vehicle))
            .collect();
        Self {
            layout,
            title: "Interventions · Pipauto",
            filters,
            has_active_vehicles: filter_vehicles.iter().any(|vehicle| !vehicle.is_archived()),
            vehicles: filter_vehicles
                .into_iter()
                .map(|vehicle| VehicleOption {
                    selected: selected == vehicle.id.as_str(),
                    active: !vehicle.is_archived(),
                    id: vehicle.id.as_str().to_owned(),
                    label: vehicle_label(&vehicle),
                })
                .collect(),
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
pub struct InterventionFormValues {
    #[serde(default)]
    pub service_date: String,
    #[serde(default)]
    pub mileage: String,
    #[serde(default)]
    pub customer_reported_problem: String,
    #[serde(default)]
    pub diagnostics: String,
    #[serde(default)]
    pub performed_work: String,
    #[serde(default)]
    pub recommendations: String,
    #[serde(default)]
    pub notes: String,
}

impl From<&Intervention> for InterventionFormValues {
    fn from(value: &Intervention) -> Self {
        Self {
            service_date: value.service_date.to_string(),
            mileage: value
                .mileage
                .map_or_else(String::new, |value| value.to_string()),
            customer_reported_problem: value.customer_reported_problem.clone().unwrap_or_default(),
            diagnostics: value.diagnostics.clone().unwrap_or_default(),
            performed_work: value.performed_work.clone().unwrap_or_default(),
            recommendations: value.recommendations.clone().unwrap_or_default(),
            notes: value.notes.clone().unwrap_or_default(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct InterventionFormPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    heading: &'static str,
    action: String,
    submit_label: &'static str,
    cancel_href: String,
    vehicle_name: String,
    registration: String,
    owner_name: String,
    current_mileage: Option<u64>,
    currency: String,
    form: FormState<InterventionFormValues>,
    conflict: Option<String>,
    history_href: String,
}

impl<'page> InterventionFormPage<'page> {
    #[must_use]
    pub fn create(
        layout: AuthenticatedLayout<'page>,
        vehicle: &Vehicle,
        owner: &Customer,
        currency: &str,
        form: FormState<InterventionFormValues>,
        conflict: Option<String>,
    ) -> Self {
        Self::new(
            layout,
            "New intervention · Pipauto",
            "New intervention",
            format!("/vehicles/{}/interventions", vehicle.id.as_str()),
            "Save draft",
            format!("/vehicles/{}", vehicle.id.as_str()),
            vehicle,
            owner,
            currency,
            form,
            conflict,
        )
    }

    #[must_use]
    pub fn edit(
        layout: AuthenticatedLayout<'page>,
        intervention_id: &str,
        vehicle: &Vehicle,
        owner: &Customer,
        currency: &str,
        form: FormState<InterventionFormValues>,
        conflict: Option<String>,
    ) -> Self {
        Self::new(
            layout,
            "Edit intervention · Pipauto",
            "Edit draft intervention",
            format!("/interventions/{intervention_id}/edit"),
            "Save changes",
            format!("/interventions/{intervention_id}"),
            vehicle,
            owner,
            currency,
            form,
            conflict,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        layout: AuthenticatedLayout<'page>,
        title: &'static str,
        heading: &'static str,
        action: String,
        submit_label: &'static str,
        cancel_href: String,
        vehicle: &Vehicle,
        owner: &Customer,
        currency: &str,
        form: FormState<InterventionFormValues>,
        conflict: Option<String>,
    ) -> Self {
        Self {
            layout,
            title,
            heading,
            action,
            submit_label,
            cancel_href,
            vehicle_name: format!("{} {}", vehicle.make, vehicle.model),
            registration: vehicle
                .registration
                .clone()
                .unwrap_or_else(|| "No registration".into()),
            owner_name: owner.display_name.clone(),
            current_mileage: vehicle.current_mileage,
            currency: currency.to_owned(),
            form,
            conflict,
            history_href: format!("/vehicles/{}/history", vehicle.id.as_str()),
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
struct LineItem {
    category: &'static str,
    description: String,
    quantity: String,
    unit_label: String,
    total: String,
}

#[derive(Debug, Serialize)]
struct AttachmentItem {
    display_name: String,
    media_type: &'static str,
    byte_size: Option<u64>,
    caption: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InterventionDetailPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    id: String,
    vehicle_id: String,
    vehicle_name: String,
    registration: String,
    owner_name: String,
    owner_href: String,
    service_date: String,
    mileage: Option<u64>,
    status: &'static str,
    status_class: &'static str,
    is_draft: bool,
    is_completed: bool,
    customer_reported_problem: Option<String>,
    diagnostics: Option<String>,
    performed_work: Option<String>,
    recommendations: Option<String>,
    notes: Option<String>,
    lines: Vec<LineItem>,
    total: String,
    attachments: Vec<AttachmentItem>,
    created_at: String,
    updated_at: String,
    completed_at: Option<String>,
    cancelled_at: Option<String>,
    conflict: Option<String>,
}

impl<'page> InterventionDetailPage<'page> {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        intervention: Intervention,
        vehicle: Vehicle,
        owner: Customer,
        lines: Vec<InterventionLine>,
        attachments: Vec<AttachmentMetadata>,
        total: Money,
        conflict: Option<String>,
    ) -> Self {
        let (status, status_class) = status(intervention.status);
        Self {
            layout,
            title: "Intervention · Pipauto",
            id: intervention.id.as_str().to_owned(),
            vehicle_id: vehicle.id.as_str().to_owned(),
            vehicle_name: format!("{} {}", vehicle.make, vehicle.model),
            registration: vehicle
                .registration
                .unwrap_or_else(|| "No registration".into()),
            owner_href: format!("/customers/{}", owner.id.as_str()),
            owner_name: owner.display_name,
            service_date: intervention.service_date.format("%d %b %Y").to_string(),
            mileage: intervention.mileage,
            status,
            status_class,
            is_draft: intervention.status == InterventionStatus::Draft,
            is_completed: intervention.status == InterventionStatus::Completed,
            customer_reported_problem: intervention.customer_reported_problem,
            diagnostics: intervention.diagnostics,
            performed_work: intervention.performed_work,
            recommendations: intervention.recommendations,
            notes: intervention.notes,
            lines: lines.into_iter().map(line_item).collect(),
            total: money(total),
            attachments: attachments
                .into_iter()
                .map(|attachment| AttachmentItem {
                    display_name: attachment.display_name,
                    media_type: attachment.media_type.as_str(),
                    byte_size: attachment.byte_size,
                    caption: attachment.caption,
                })
                .collect(),
            created_at: timestamp(intervention.created_at),
            updated_at: timestamp(intervention.updated_at),
            completed_at: intervention.completed_at.map(timestamp),
            cancelled_at: intervention.cancelled_at.map(timestamp),
            conflict,
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(DETAIL_PAGE, self)
    }

    pub fn render_content(&self, engine: &TeraView) -> Result<String> {
        engine.render(DETAIL_FRAGMENT, self)
    }
}

#[derive(Debug, Serialize)]
pub struct InterventionTransitionPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    heading: &'static str,
    action: String,
    action_label: &'static str,
    explanation: &'static str,
    destructive: bool,
    id: String,
    vehicle: String,
    service_date: String,
    mileage: Option<u64>,
    total: String,
    work_summary: String,
}

impl<'page> InterventionTransitionPage<'page> {
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        intervention: &Intervention,
        vehicle: &Vehicle,
        total: Money,
        completion: bool,
    ) -> Self {
        let (title, heading, action_label, explanation) = if completion {
            (
                "Complete intervention · Pipauto",
                "Complete intervention?",
                "Complete and lock intervention",
                "This permanently locks ordinary details and line items. Completion cannot be undone in this release.",
            )
        } else {
            (
                "Cancel intervention · Pipauto",
                "Cancel intervention?",
                "Cancel intervention",
                "The record remains visible as Cancelled in service history and cannot return to Draft.",
            )
        };
        Self {
            layout,
            title,
            heading,
            action: format!(
                "/interventions/{}/{}",
                intervention.id.as_str(),
                if completion { "complete" } else { "cancel" }
            ),
            action_label,
            explanation,
            destructive: !completion,
            id: intervention.id.as_str().to_owned(),
            vehicle: vehicle_label(vehicle),
            service_date: intervention.service_date.format("%d %b %Y").to_string(),
            mileage: intervention.mileage,
            total: money(total),
            work_summary: intervention
                .performed_work
                .clone()
                .unwrap_or_else(|| "No work performed has been recorded.".to_owned()),
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(TRANSITION_PAGE, self)
    }

    pub fn render_content(&self, engine: &TeraView) -> Result<String> {
        engine.render(TRANSITION_FRAGMENT, self)
    }
}

fn list_item(entry: ServiceHistorySummary, vehicle: Vehicle) -> ListItem {
    let intervention = entry.intervention;
    let (status, status_class) = status(intervention.status);
    ListItem {
        date: intervention.service_date.format("%d %b %Y").to_string(),
        vehicle: format!("{} {}", vehicle.make, vehicle.model),
        registration: vehicle
            .registration
            .unwrap_or_else(|| "No registration".into()),
        mileage: intervention.mileage,
        summary: intervention
            .performed_work
            .or(intervention.customer_reported_problem)
            .or(intervention.diagnostics)
            .unwrap_or_else(|| "No narrative recorded".to_owned()),
        status,
        status_class,
        total: money(entry.totals.price),
        href: format!("/interventions/{}", intervention.id.as_str()),
    }
}

fn line_item(line: InterventionLine) -> LineItem {
    LineItem {
        category: match line.category {
            InterventionLineCategory::Labour => "Labour",
            InterventionLineCategory::Part => "Part",
            InterventionLineCategory::Material => "Material",
            InterventionLineCategory::Other => "Other",
        },
        description: line.description,
        quantity: line.quantity.to_string(),
        unit_label: line.unit_label,
        total: money(line.total_price),
    }
}

fn status(value: InterventionStatus) -> (&'static str, &'static str) {
    match value {
        InterventionStatus::Draft => ("Draft", "warning"),
        InterventionStatus::Completed => ("Completed", "success"),
        InterventionStatus::Cancelled => ("Cancelled", "error"),
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

fn timestamp(value: chrono::DateTime<chrono::Utc>) -> String {
    value.format("%d %b %Y, %H:%M UTC").to_string()
}
