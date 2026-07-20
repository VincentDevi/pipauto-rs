//! Presentation-safe customer collection, form, and detail models.

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
        customer::{Address, Customer},
        vehicle::Vehicle,
    },
};

use super::layout::AuthenticatedLayout;

const LIST_PAGE_TEMPLATE: &str = "pages/customers.html";
const LIST_CONTENT_TEMPLATE: &str = "fragments/customer_list.html";
const FORM_PAGE_TEMPLATE: &str = "pages/customer_form.html";
const FORM_TEMPLATE: &str = "fragments/customer_form.html";
const DETAIL_PAGE_TEMPLATE: &str = "pages/customer_detail.html";
const DETAIL_CONTENT_TEMPLATE: &str = "fragments/customer_detail.html";

#[derive(Debug, Serialize)]
pub struct CustomerListPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    query: String,
    archive: &'static str,
    active_selected: bool,
    archived_selected: bool,
    items: Vec<CustomerListItem>,
    next_href: Option<String>,
    has_filters: bool,
    first_customer: bool,
    filter_error: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct CustomerListItem {
    name: String,
    contact_summary: String,
    has_contact: bool,
    archived: bool,
    href: String,
}

impl<'page> CustomerListPage<'page> {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        query: String,
        archive: &'static str,
        page: Page<Customer>,
        next_href: Option<String>,
        first_customer: bool,
        filter_error: Option<&'static str>,
    ) -> Self {
        let has_filters = !query.trim().is_empty() || archive != "active";
        Self {
            layout,
            title: "Customers · Pipauto",
            query,
            archive,
            active_selected: archive == "active",
            archived_selected: archive == "archived",
            items: page.items.into_iter().map(CustomerListItem::from).collect(),
            next_href,
            has_filters,
            first_customer,
            filter_error,
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(LIST_PAGE_TEMPLATE, self)
    }

    pub fn render_content(&self, engine: &TeraView) -> Result<String> {
        engine.render(LIST_CONTENT_TEMPLATE, self)
    }
}

impl From<Customer> for CustomerListItem {
    fn from(customer: Customer) -> Self {
        let archived = customer.is_archived();
        let contact = [customer.email.as_deref(), customer.phone.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        Self {
            href: format!("/customers/{}", customer.id.as_str()),
            name: customer.display_name,
            has_contact: !contact.is_empty(),
            contact_summary: contact,
            archived,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct CustomerFormValues {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub phone: String,
    #[serde(default)]
    pub address_line_1: String,
    #[serde(default)]
    pub address_line_2: String,
    #[serde(default)]
    pub postal_code: String,
    #[serde(default)]
    pub city: String,
    #[serde(default)]
    pub country_code: String,
    #[serde(default)]
    pub notes: String,
}

impl From<Customer> for CustomerFormValues {
    fn from(customer: Customer) -> Self {
        let address = customer.address.unwrap_or(Address {
            line_1: String::new(),
            line_2: None,
            postal_code: String::new(),
            city: String::new(),
            country_code: String::new(),
        });
        Self {
            display_name: customer.display_name,
            email: customer.email.unwrap_or_default(),
            phone: customer.phone.unwrap_or_default(),
            address_line_1: address.line_1,
            address_line_2: address.line_2.unwrap_or_default(),
            postal_code: address.postal_code,
            city: address.city,
            country_code: address.country_code,
            notes: customer.notes.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CustomerFormPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: &'static str,
    heading: &'static str,
    action: String,
    submit_label: &'static str,
    cancel_href: String,
    form: FormState<CustomerFormValues>,
    has_errors: bool,
    conflict_message: Option<&'static str>,
}

impl<'page> CustomerFormPage<'page> {
    #[must_use]
    pub fn create(
        layout: AuthenticatedLayout<'page>,
        form: FormState<CustomerFormValues>,
        conflict_message: Option<&'static str>,
    ) -> Self {
        let has_errors = !form.errors.is_empty();
        Self {
            layout,
            title: "New customer · Pipauto",
            heading: "New customer",
            action: "/customers".to_owned(),
            submit_label: "Create customer",
            cancel_href: "/customers".to_owned(),
            form,
            has_errors,
            conflict_message,
        }
    }

    #[must_use]
    pub fn edit(
        layout: AuthenticatedLayout<'page>,
        id: &str,
        form: FormState<CustomerFormValues>,
        conflict_message: Option<&'static str>,
    ) -> Self {
        let has_errors = !form.errors.is_empty();
        Self {
            layout,
            title: "Edit customer · Pipauto",
            heading: "Edit customer",
            action: format!("/customers/{id}/edit"),
            submit_label: "Save changes",
            cancel_href: format!("/customers/{id}"),
            form,
            has_errors,
            conflict_message,
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(FORM_PAGE_TEMPLATE, self)
    }

    pub fn render_form(&self, engine: &TeraView) -> Result<String> {
        engine.render(FORM_TEMPLATE, self)
    }
}

#[derive(Debug, Serialize)]
pub struct CustomerDetailPage<'page> {
    #[serde(flatten)]
    layout: AuthenticatedLayout<'page>,
    title: String,
    customer: CustomerDetail,
    vehicles: Vec<CustomerVehicleItem>,
    next_vehicle_href: Option<String>,
    lifecycle_message: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct CustomerDetail {
    name: String,
    email: Option<String>,
    phone: Option<String>,
    address: Option<AddressView>,
    notes: Option<String>,
    created_at: String,
    updated_at: String,
    archived_at: Option<String>,
    archived: bool,
    edit_href: String,
    archive_action: String,
    restore_action: String,
    new_vehicle_href: String,
}

#[derive(Debug, Serialize)]
struct AddressView {
    line_1: String,
    line_2: Option<String>,
    postal_code: String,
    city: String,
    country_code: String,
}

#[derive(Debug, Serialize)]
struct CustomerVehicleItem {
    name: String,
    reference: Option<String>,
    archived: bool,
    href: String,
}

impl<'page> CustomerDetailPage<'page> {
    #[must_use]
    pub fn new(
        layout: AuthenticatedLayout<'page>,
        customer: Customer,
        vehicles: Page<Vehicle>,
        next_vehicle_href: Option<String>,
        lifecycle_message: Option<&'static str>,
    ) -> Self {
        let id = customer.id.as_str();
        let archived = customer.is_archived();
        let detail = CustomerDetail {
            name: customer.display_name.clone(),
            email: customer.email,
            phone: customer.phone,
            address: customer.address.map(AddressView::from),
            notes: customer.notes,
            created_at: timestamp(customer.created_at),
            updated_at: timestamp(customer.updated_at),
            archived_at: customer.archived_at.map(timestamp),
            archived,
            edit_href: format!("/customers/{id}/edit"),
            archive_action: format!("/customers/{id}/archive"),
            restore_action: format!("/customers/{id}/restore"),
            new_vehicle_href: format!("/customers/{id}/vehicles/new"),
        };
        Self {
            layout,
            title: format!("{} · Pipauto", customer.display_name),
            customer: detail,
            vehicles: vehicles
                .items
                .into_iter()
                .map(CustomerVehicleItem::from)
                .collect(),
            next_vehicle_href,
            lifecycle_message,
        }
    }

    pub fn render_page(&self, engine: &TeraView) -> Result<String> {
        engine.render(DETAIL_PAGE_TEMPLATE, self)
    }

    pub fn render_content(&self, engine: &TeraView) -> Result<String> {
        engine.render(DETAIL_CONTENT_TEMPLATE, self)
    }
}

impl From<Address> for AddressView {
    fn from(address: Address) -> Self {
        Self {
            line_1: address.line_1,
            line_2: address.line_2,
            postal_code: address.postal_code,
            city: address.city,
            country_code: address.country_code,
        }
    }
}

impl From<Vehicle> for CustomerVehicleItem {
    fn from(vehicle: Vehicle) -> Self {
        let reference = vehicle.registration.clone().or_else(|| vehicle.vin.clone());
        Self {
            href: format!("/vehicles/{}", vehicle.id.as_str()),
            name: format!("{} {}", vehicle.make, vehicle.model),
            reference,
            archived: vehicle.is_archived(),
        }
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
        domain::CustomerId,
        views::{context::PresentationUser, layout::AuthenticatedLayout},
    };

    #[test]
    fn customer_templates_render_empty_collection_and_form_states() {
        let engine = TeraView::build().expect("view engine");
        let user = PresentationUser {
            display_name: "Filippo".to_owned(),
        };
        let list = CustomerListPage::new(
            AuthenticatedLayout::new(&user, "csrf", "/customers"),
            String::new(),
            "active",
            Page {
                items: Vec::new(),
                next_cursor: None,
            },
            None,
            true,
            None,
        );
        list.render_page(&engine).expect("customer list template");

        let form = CustomerFormPage::create(
            AuthenticatedLayout::new(&user, "csrf", "/customers/new"),
            FormState::new(CustomerFormValues::default()).with_known_fields(&[
                "display_name",
                "email",
                "phone",
                "address.line_1",
                "address.line_2",
                "address.postal_code",
                "address.city",
                "address.country_code",
                "notes",
            ]),
            None,
        );
        form.render_page(&engine).expect("customer form template");
    }

    #[test]
    fn customer_form_uses_submitted_display_values_only() {
        let customer = Customer {
            id: CustomerId::parse("customer-1").expect("id"),
            display_name: "  Display Name  ".to_owned(),
            email: Some("Display@Example.COM".to_owned()),
            phone: Some("+32 (0) 475 12 34 56".to_owned()),
            address: None,
            notes: None,
            created_at: Utc.with_ymd_and_hms(2026, 7, 20, 10, 0, 0).unwrap(),
            updated_at: Utc.with_ymd_and_hms(2026, 7, 20, 10, 0, 0).unwrap(),
            archived_at: None,
        };

        let values = CustomerFormValues::from(customer);
        assert_eq!(values.email, "Display@Example.COM");
        assert_eq!(values.phone, "+32 (0) 475 12 34 56");
    }
}
