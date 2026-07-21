//! Presentation-safe vehicle, service-history, and attachment-metadata models.

use chrono::{DateTime, Utc};
use loco_rs::{
    controller::views::{engines::TeraView, ViewRenderer},
    Result,
};
use serde::{Deserialize, Serialize};

use crate::{
    controllers::browser::forms::FormState,
    domain::Page,
    models::{
        attachment::AttachmentMetadata,
        customer::Customer,
        intervention::{InterventionStatus, ServiceHistorySummary},
        vehicle::Vehicle,
    },
};

use super::layout::AuthenticatedLayout;

const LIST_PAGE: &str = "pages/vehicles.html";
const LIST_FRAGMENT: &str = "fragments/vehicle_list.html";
const FORM_PAGE: &str = "pages/vehicle_form.html";
const FORM_FRAGMENT: &str = "fragments/vehicle_form.html";
const DETAIL_PAGE: &str = "pages/vehicle_detail.html";
const DETAIL_FRAGMENT: &str = "fragments/vehicle_detail.html";
const HISTORY_PAGE: &str = "pages/service_history.html";
const HISTORY_FRAGMENT: &str = "fragments/service_history.html";
const REASSIGN_PAGE: &str = "pages/vehicle_reassign.html";
const REASSIGN_FRAGMENT: &str = "fragments/vehicle_reassign.html";
const ATTACHMENT_PAGE: &str = "pages/attachment_form.html";
const ATTACHMENT_FRAGMENT: &str = "fragments/attachment_form.html";

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct VehicleFormValues {
    #[serde(default)]
    pub customer_id: String,
    #[serde(default)]
    pub make: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub year: String,
    #[serde(default)]
    pub registration: String,
    #[serde(default)]
    pub vin: String,
    #[serde(default)]
    pub current_mileage: String,
    #[serde(default)]
    pub engine_type: String,
    #[serde(default)]
    pub notes: String,
}

impl From<Vehicle> for VehicleFormValues {
    fn from(vehicle: Vehicle) -> Self {
        Self {
            customer_id: vehicle.customer_id.as_str().to_owned(),
            make: vehicle.make,
            model: vehicle.model,
            year: vehicle
                .year
                .map_or(String::new(), |value| value.to_string()),
            registration: vehicle.registration.unwrap_or_default(),
            vin: vehicle.vin.unwrap_or_default(),
            current_mileage: vehicle
                .current_mileage
                .map_or(String::new(), |value| value.to_string()),
            engine_type: vehicle.engine_type.unwrap_or_default(),
            notes: vehicle.notes.unwrap_or_default(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct AttachmentFormValues {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub caption: String,
}

impl From<AttachmentMetadata> for AttachmentFormValues {
    fn from(value: AttachmentMetadata) -> Self {
        Self {
            display_name: value.display_name,
            caption: value.caption.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CustomerOption {
    pub id: String,
    pub name: String,
}

impl From<Customer> for CustomerOption {
    fn from(value: Customer) -> Self {
        Self {
            id: value.id.as_str().to_owned(),
            name: value.display_name,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct VehicleListPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    pub filters: VehicleFilterValues,
    items: Vec<VehicleListItem>,
    next_href: Option<String>,
    filter_error: Option<String>,
    has_filters: bool,
    customers: Vec<CustomerOption>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct VehicleFilterValues {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub customer: String,
    #[serde(default)]
    pub registration: String,
    #[serde(default)]
    pub vin: String,
    #[serde(default)]
    pub make: String,
    #[serde(default)]
    pub model: String,
    #[serde(default = "default_archive")]
    pub archived: String,
    #[serde(default)]
    pub cursor: String,
}

fn default_archive() -> String {
    "active".to_owned()
}

#[derive(Debug, Serialize)]
struct VehicleListItem {
    registration: String,
    name: String,
    year: Option<i32>,
    owner: String,
    mileage: Option<u64>,
    archived: bool,
    href: String,
}

impl<'page> VehicleListPage<'page> {
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        filters: VehicleFilterValues,
        page: Page<Vehicle>,
        owners: Vec<Customer>,
        next_href: Option<String>,
        filter_error: Option<String>,
        customers: Vec<Customer>,
    ) -> Self {
        let has_filters = !filters.q.trim().is_empty()
            || !filters.customer.is_empty()
            || !filters.registration.trim().is_empty()
            || !filters.vin.trim().is_empty()
            || !filters.make.trim().is_empty()
            || !filters.model.trim().is_empty()
            || filters.archived != "active";
        let items = page
            .items
            .into_iter()
            .zip(owners)
            .map(|(vehicle, owner)| VehicleListItem {
                registration: vehicle
                    .registration
                    .clone()
                    .unwrap_or_else(|| "No registration".to_owned()),
                name: format!("{} {}", vehicle.make, vehicle.model),
                year: vehicle.year,
                owner: owner.display_name,
                mileage: vehicle.current_mileage,
                archived: vehicle.is_archived(),
                href: format!("/vehicles/{}", vehicle.id.as_str()),
            })
            .collect();
        Self {
            layout,
            title: "Vehicles · Pipauto",
            filters,
            items,
            next_href,
            filter_error,
            has_filters,
            customers: customers.into_iter().map(Into::into).collect(),
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(LIST_PAGE, self)
    }

    pub fn render_content(&self, engine: &TeraView) -> Result<String> {
        engine.render(LIST_FRAGMENT, self)
    }
}

#[derive(Debug, Serialize)]
pub struct VehicleFormPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    heading: &'static str,
    action: String,
    cancel_href: String,
    submit_label: &'static str,
    form: FormState<VehicleFormValues>,
    customers: Vec<CustomerOption>,
    owner_locked: bool,
    conflict_message: Option<String>,
}

impl<'page> VehicleFormPage<'page> {
    #[must_use]
    pub fn create(
        layout: AuthenticatedLayout<'page>,
        form: FormState<VehicleFormValues>,
        customers: Vec<Customer>,
        owner_locked: bool,
        cancel_href: String,
        conflict_message: Option<String>,
    ) -> Self {
        Self {
            layout,
            title: "Register vehicle · Pipauto",
            heading: "Register vehicle",
            action: "/vehicles".to_owned(),
            cancel_href,
            submit_label: "Save vehicle",
            form,
            customers: customers.into_iter().map(Into::into).collect(),
            owner_locked,
            conflict_message,
        }
    }

    #[must_use]
    pub fn edit(
        layout: AuthenticatedLayout<'page>,
        id: &str,
        form: FormState<VehicleFormValues>,
        owner: Customer,
        conflict_message: Option<String>,
    ) -> Self {
        Self {
            layout,
            title: "Edit vehicle · Pipauto",
            heading: "Edit vehicle",
            action: format!("/vehicles/{id}/edit"),
            cancel_href: format!("/vehicles/{id}"),
            submit_label: "Save changes",
            form,
            customers: vec![owner.into()],
            owner_locked: true,
            conflict_message,
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
struct HistoryItem {
    date: String,
    status: &'static str,
    status_class: &'static str,
    mileage: Option<u64>,
    summary: String,
    total: String,
    href: String,
}

fn history_items(items: Vec<ServiceHistorySummary>) -> Vec<HistoryItem> {
    items
        .into_iter()
        .map(|entry| {
            let intervention = entry.intervention;
            let (status, status_class) = match intervention.status {
                InterventionStatus::Draft => ("Draft", "warning"),
                InterventionStatus::Completed => ("Completed", "success"),
                InterventionStatus::Cancelled => ("Cancelled", "error"),
            };
            let summary = intervention
                .performed_work
                .as_deref()
                .or(intervention.customer_reported_problem.as_deref())
                .or(intervention.diagnostics.as_deref())
                .unwrap_or("No work summary recorded")
                .to_owned();
            let amount = entry.totals.price;
            let minor = amount.minor_units();
            HistoryItem {
                date: intervention.service_date.format("%d %b %Y").to_string(),
                status,
                status_class,
                mileage: intervention.mileage,
                summary,
                total: format!(
                    "{} {}.{:02}",
                    amount.currency().as_str(),
                    minor / 100,
                    minor % 100
                ),
                href: format!("/interventions/{}", intervention.id.as_str()),
            }
        })
        .collect()
}

#[derive(Debug, Serialize)]
struct AttachmentItem {
    display_name: String,
    media_type: String,
    byte_size: u64,
    caption: Option<String>,
    open_href: String,
    download_href: String,
    edit_href: String,
    delete_action: String,
}

#[derive(Debug, Serialize)]
pub struct VehicleDetailPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: String,
    vehicle: VehicleDetail,
    history: Vec<HistoryItem>,
    attachments: Vec<AttachmentItem>,
    lifecycle_message: Option<String>,
}

#[derive(Debug, Serialize)]
struct VehicleDetail {
    id: String,
    name: String,
    registration: String,
    year: Option<i32>,
    vin: Option<String>,
    mileage: Option<u64>,
    engine: Option<String>,
    notes: Option<String>,
    archived: bool,
    owner_name: String,
    owner_href: String,
    created_at: String,
    updated_at: String,
    archived_at: Option<String>,
}

impl<'page> VehicleDetailPage<'page> {
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        vehicle: Vehicle,
        owner: Customer,
        history: Vec<ServiceHistorySummary>,
        attachments: Vec<AttachmentMetadata>,
        lifecycle_message: Option<String>,
    ) -> Self {
        let id = vehicle.id.as_str().to_owned();
        let archived = vehicle.is_archived();
        let name = format!("{} {}", vehicle.make, vehicle.model);
        let detail = VehicleDetail {
            id: id.clone(),
            name: name.clone(),
            registration: vehicle
                .registration
                .unwrap_or_else(|| "No registration".to_owned()),
            year: vehicle.year,
            vin: vehicle.vin,
            mileage: vehicle.current_mileage,
            engine: vehicle.engine_type,
            notes: vehicle.notes,
            archived,
            owner_href: format!("/customers/{}", owner.id.as_str()),
            owner_name: owner.display_name,
            created_at: timestamp(vehicle.created_at),
            updated_at: timestamp(vehicle.updated_at),
            archived_at: vehicle.archived_at.map(timestamp),
        };
        Self {
            layout,
            title: format!("{name} · Pipauto"),
            vehicle: detail,
            history: history_items(history),
            attachments: attachments
                .into_iter()
                .map(|attachment| {
                    let attachment_id = attachment.id.as_str().to_owned();
                    AttachmentItem {
                        display_name: attachment.display_name,
                        media_type: attachment.media_type.as_str().to_owned(),
                        byte_size: attachment.byte_size,
                        caption: attachment.caption,
                        open_href: format!("/attachments/{attachment_id}/content"),
                        download_href: format!("/attachments/{attachment_id}/download"),
                        edit_href: format!("/attachments/{attachment_id}/edit"),
                        delete_action: format!("/attachments/{attachment_id}/delete"),
                    }
                })
                .collect(),
            lifecycle_message,
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(DETAIL_PAGE, self)
    }

    pub fn render_content(&self, engine: &TeraView) -> Result<String> {
        engine.render(DETAIL_FRAGMENT, self)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct HistoryFilterValues {
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
pub struct ServiceHistoryPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: String,
    vehicle_id: String,
    vehicle_name: String,
    registration: String,
    archived: bool,
    filters: HistoryFilterValues,
    history: Vec<HistoryItem>,
    next_href: Option<String>,
    filter_error: Option<String>,
}

impl<'page> ServiceHistoryPage<'page> {
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        vehicle: Vehicle,
        filters: HistoryFilterValues,
        page: Page<ServiceHistorySummary>,
        next_href: Option<String>,
        filter_error: Option<String>,
    ) -> Self {
        let name = format!("{} {}", vehicle.make, vehicle.model);
        let archived = vehicle.is_archived();
        Self {
            layout,
            title: format!("Service history · {name} · Pipauto"),
            vehicle_id: vehicle.id.as_str().to_owned(),
            vehicle_name: name,
            registration: vehicle
                .registration
                .unwrap_or_else(|| "No registration".to_owned()),
            archived,
            filters,
            history: history_items(page.items),
            next_href,
            filter_error,
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(HISTORY_PAGE, self)
    }

    pub fn render_content(&self, engine: &TeraView) -> Result<String> {
        engine.render(HISTORY_FRAGMENT, self)
    }
}

#[derive(Debug, Serialize)]
pub struct ReassignPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    vehicle_id: String,
    vehicle_name: String,
    old_owner: String,
    selected_owner: Option<CustomerOption>,
    customers: Vec<CustomerOption>,
    message: Option<String>,
}

impl<'page> ReassignPage<'page> {
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        vehicle: &Vehicle,
        old_owner: Customer,
        selected_owner: Option<Customer>,
        customers: Vec<Customer>,
        message: Option<String>,
    ) -> Self {
        Self {
            layout,
            title: "Reassign vehicle · Pipauto",
            vehicle_id: vehicle.id.as_str().to_owned(),
            vehicle_name: format!(
                "{} · {} {}",
                vehicle.registration.as_deref().unwrap_or("No registration"),
                vehicle.make,
                vehicle.model
            ),
            old_owner: old_owner.display_name,
            selected_owner: selected_owner.map(Into::into),
            customers: customers.into_iter().map(Into::into).collect(),
            message,
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(REASSIGN_PAGE, self)
    }

    pub fn render_content(&self, engine: &TeraView) -> Result<String> {
        engine.render(REASSIGN_FRAGMENT, self)
    }
}

#[derive(Debug, Serialize)]
pub struct AttachmentFormPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    heading: &'static str,
    owner_name: String,
    owner_description: &'static str,
    action: String,
    cancel_href: String,
    submit_label: &'static str,
    editing: bool,
    form: FormState<AttachmentFormValues>,
    conflict_message: Option<String>,
}

impl<'page> AttachmentFormPage<'page> {
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        vehicle: &Vehicle,
        attachment_id: Option<&str>,
        form: FormState<AttachmentFormValues>,
        conflict_message: Option<String>,
    ) -> Self {
        let id = vehicle.id.as_str();
        Self::build(
            layout,
            attachment_id,
            form,
            conflict_message,
            format!("{} {}", vehicle.make, vehicle.model),
            "vehicle",
            format!("/vehicles/{id}"),
            format!("/vehicles/{id}/attachments"),
            "/attachments".to_owned(),
        )
    }

    #[must_use]
    pub fn for_intervention(
        layout: AuthenticatedLayout<'page>,
        intervention: &crate::models::intervention::Intervention,
        vehicle: &Vehicle,
        attachment_id: Option<&str>,
        form: FormState<AttachmentFormValues>,
        conflict_message: Option<String>,
    ) -> Self {
        let id = intervention.id.as_str();
        Self::build(
            layout,
            attachment_id,
            form,
            conflict_message,
            format!(
                "{} {} · {}",
                vehicle.make, vehicle.model, intervention.service_date
            ),
            "intervention",
            format!("/interventions/{id}"),
            format!("/interventions/{id}/attachments"),
            format!("/interventions/{id}/attachments"),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        layout: AuthenticatedLayout<'page>,
        attachment_id: Option<&str>,
        form: FormState<AttachmentFormValues>,
        conflict_message: Option<String>,
        owner_name: String,
        owner_description: &'static str,
        cancel_href: String,
        create_action: String,
        edit_action_prefix: String,
    ) -> Self {
        let editing = attachment_id.is_some();
        Self {
            layout,
            title: if editing {
                "Edit attachment details · Pipauto"
            } else {
                "Upload attachment · Pipauto"
            },
            heading: if editing {
                "Edit attachment details"
            } else {
                "Upload attachment"
            },
            owner_name,
            owner_description,
            action: attachment_id.map_or_else(
                || create_action,
                |attachment_id| format!("{edit_action_prefix}/{attachment_id}/edit"),
            ),
            cancel_href,
            submit_label: if editing {
                "Save details"
            } else {
                "Upload file"
            },
            editing,
            form,
            conflict_message,
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(ATTACHMENT_PAGE, self)
    }

    pub fn render_form(&self, engine: &TeraView) -> Result<String> {
        engine.render(ATTACHMENT_FRAGMENT, self)
    }
}

fn timestamp(value: DateTime<Utc>) -> String {
    value.format("%d %b %Y, %H:%M UTC").to_string()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;
    use crate::{
        domain::{CustomerId, VehicleId},
        views::{context::PresentationUser, layout::AuthenticatedLayout},
    };

    fn user() -> PresentationUser {
        PresentationUser {
            display_name: "Filippo".to_owned(),
        }
    }

    fn customer() -> Customer {
        Customer {
            id: CustomerId::parse("customer-1").expect("customer id"),
            display_name: "Mario Rossi".to_owned(),
            email: None,
            phone: None,
            address: None,
            notes: None,
            created_at: Utc.with_ymd_and_hms(2026, 7, 20, 10, 0, 0).unwrap(),
            updated_at: Utc.with_ymd_and_hms(2026, 7, 20, 10, 0, 0).unwrap(),
            archived_at: None,
        }
    }

    fn vehicle() -> Vehicle {
        Vehicle {
            id: VehicleId::parse("vehicle-1").expect("vehicle id"),
            customer_id: CustomerId::parse("customer-1").expect("customer id"),
            make: "Volkswagen".to_owned(),
            model: "Golf".to_owned(),
            year: Some(2018),
            registration: Some("1-abc-234".to_owned()),
            vin: Some("wvwzzz1jzxw000001".to_owned()),
            current_mileage: Some(126_400),
            engine_type: Some("2.0 TDI".to_owned()),
            notes: Some("Workshop note".to_owned()),
            created_at: Utc.with_ymd_and_hms(2026, 7, 20, 10, 0, 0).unwrap(),
            updated_at: Utc.with_ymd_and_hms(2026, 7, 20, 10, 0, 0).unwrap(),
            archived_at: None,
        }
    }

    #[test]
    fn vehicle_browser_templates_render_list_forms_detail_and_attachment_pages() {
        let engine = TeraView::build().expect("view engine");
        let user = user();
        let list = VehicleListPage::new(
            AuthenticatedLayout::new(&user, "csrf", "/vehicles"),
            VehicleFilterValues::default(),
            Page {
                items: vec![vehicle()],
                next_cursor: None,
            },
            vec![customer()],
            None,
            None,
            vec![customer()],
        );
        list.render_page(&engine).expect("vehicle list template");

        let form = VehicleFormPage::create(
            AuthenticatedLayout::new(&user, "csrf", "/vehicles/new"),
            FormState::new(VehicleFormValues::default()).with_known_fields(&[
                "customer_id",
                "make",
                "model",
                "year",
                "registration",
                "vin",
                "current_mileage",
                "engine_type",
                "notes",
            ]),
            vec![customer()],
            false,
            "/vehicles".to_owned(),
            None,
        );
        form.render_page(&engine).expect("vehicle form template");

        let detail = VehicleDetailPage::new(
            AuthenticatedLayout::new(&user, "csrf", "/vehicles/vehicle-1"),
            vehicle(),
            customer(),
            Vec::new(),
            Vec::new(),
            None,
        );
        detail
            .render_page(&engine)
            .expect("vehicle detail template");

        let attachment = AttachmentFormPage::new(
            AuthenticatedLayout::new(&user, "csrf", "/vehicles/vehicle-1/attachments/new"),
            &vehicle(),
            None,
            FormState::new(AttachmentFormValues::default()).with_known_fields(&[
                "file",
                "display_name",
                "caption",
            ]),
            None,
        );
        let html = attachment
            .render_page(&engine)
            .expect("attachment template");
        assert!(html.contains("Upload attachment"));
        assert!(html.contains("type=\"file\""));
    }

    #[test]
    fn service_history_browser_template_keeps_authoritative_order_message() {
        let engine = TeraView::build().expect("view engine");
        let user = user();
        let history = ServiceHistoryPage::new(
            AuthenticatedLayout::new(&user, "csrf", "/vehicles/vehicle-1/history"),
            vehicle(),
            HistoryFilterValues::default(),
            Page {
                items: Vec::new(),
                next_cursor: None,
            },
            None,
            None,
        );
        let html = history.render_page(&engine).expect("history template");
        assert!(html.contains("Authoritative server order"));
    }
}
