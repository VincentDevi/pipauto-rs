//! Server-rendered customer workflows backed directly by application services.

use axum::{extract::Query, http::StatusCode, response::Response};
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
        context::BrowserRequestContext,
        forms::{body_limit, AuthenticatedForm, FormState},
        responses,
    },
    domain::{
        CustomerId, OpaqueCursor, Page, PageRequest, ValidationCode, ValidationError,
        ValidationErrors,
    },
    models::{
        customer::{
            ArchiveFilter, CreateCustomer, CustomerAddressInput, CustomerFilter,
            CustomerModel as CustomerService, UpdateCustomer, ADDRESS_LINE_MAX_CHARS,
            CITY_MAX_CHARS, DISPLAY_NAME_MAX_CHARS, EMAIL_MAX_CHARS, NOTES_MAX_CHARS,
            PHONE_MAX_CHARS, POSTAL_CODE_MAX_CHARS,
        },
        vehicle::{VehicleFilter, VehicleModel as VehicleService},
        ModelError as WorkflowError,
    },
    settings::BusinessSettings,
    views::{
        customer::{CustomerDetailPage, CustomerFormPage, CustomerFormValues, CustomerListPage},
        layout::AuthenticatedLayout,
    },
};

#[derive(Clone, Debug, Default, Deserialize)]
struct CustomerQuery {
    #[serde(default)]
    q: String,
    archived: Option<String>,
    cursor: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct CustomerDetailQuery {
    vehicle_cursor: Option<String>,
}

const CUSTOMER_FORM_FIELDS: &[&str] = &[
    "display_name",
    "email",
    "phone",
    "address.line_1",
    "address.line_2",
    "address.postal_code",
    "address.city",
    "address.country_code",
    "notes",
];

async fn list(
    context: BrowserRequestContext,
    SharedStore(service): SharedStore<CustomerService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Query(query): Query<CustomerQuery>,
) -> Result<Response> {
    let (archive, archive_name) = match parse_archive(query.archived.as_deref()) {
        Ok(value) => value,
        Err(message) => {
            return render_list(
                &context,
                &engine,
                query.q,
                "active",
                Page {
                    items: Vec::new(),
                    next_cursor: None,
                },
                false,
                Some(message),
                StatusCode::UNPROCESSABLE_ENTITY,
            );
        }
    };
    let cursor = match parse_cursor(query.cursor) {
        Ok(cursor) => cursor,
        Err(message) => {
            return render_list(
                &context,
                &engine,
                query.q,
                archive_name,
                Page {
                    items: Vec::new(),
                    next_cursor: None,
                },
                false,
                Some(message),
                StatusCode::UNPROCESSABLE_ENTITY,
            );
        }
    };
    let page = match service
        .list(PageRequest {
            filter: CustomerFilter {
                query: Some(query.q.clone()),
                archive,
            },
            limit: settings.default_collection_limit(),
            after: cursor,
        })
        .await
    {
        Ok(page) => page,
        Err(WorkflowError::Validation(_)) => {
            return render_list(
                &context,
                &engine,
                query.q,
                archive_name,
                Page {
                    items: Vec::new(),
                    next_cursor: None,
                },
                false,
                Some("This page link does not match the current filters. Start from the customer list."),
                StatusCode::UNPROCESSABLE_ENTITY,
            );
        }
        Err(error) => return Ok(workflow_response(&context, error, "customer")),
    };
    let first_customer =
        if page.items.is_empty() && query.q.trim().is_empty() && archive == ArchiveFilter::Active {
            match service
                .list(PageRequest {
                    filter: CustomerFilter {
                        query: None,
                        archive: ArchiveFilter::All,
                    },
                    limit: crate::domain::PageLimit::new(1).expect("one is a valid page limit"),
                    after: None,
                })
                .await
            {
                Ok(any_page) => any_page.items.is_empty(),
                Err(_) => false,
            }
        } else {
            false
        };
    render_list(
        &context,
        &engine,
        query.q,
        archive_name,
        page,
        first_customer,
        None,
        StatusCode::OK,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_list(
    context: &BrowserRequestContext,
    engine: &TeraView,
    query: String,
    archive: &'static str,
    page: Page<crate::models::customer::Customer>,
    first_customer: bool,
    filter_error: Option<&'static str>,
    status: StatusCode,
) -> Result<Response> {
    let next_href = page
        .next_cursor
        .as_ref()
        .map(|cursor| customer_list_href(&query, archive, cursor.as_str()));
    let view = CustomerListPage::new(
        layout(context),
        query,
        archive,
        page,
        next_href,
        first_customer,
        filter_error,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_content(engine)?,
    ))
}

async fn new_form(
    context: BrowserRequestContext,
    ViewEngine(engine): ViewEngine<TeraView>,
) -> Result<Response> {
    render_create_form(
        &context,
        &engine,
        FormState::new(CustomerFormValues::default()),
        None,
        StatusCode::OK,
    )
}

async fn create(
    context: BrowserRequestContext,
    SharedStore(service): SharedStore<CustomerService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    form: AuthenticatedForm<CustomerFormValues>,
) -> Result<Response> {
    let values = form.fields;
    if let Some(errors) = validate_browser_form(&values) {
        return render_create_form(
            &context,
            &engine,
            FormState::with_validation(values, &errors),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        );
    }
    match service.create(create_command(&values)).await {
        Ok(customer) => Ok(responses::redirect(
            context.response_preference,
            &format!("/customers/{}", customer.id.as_str()),
        )),
        Err(WorkflowError::Validation(errors)) => render_create_form(
            &context,
            &engine,
            FormState::with_validation(values, &errors),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(WorkflowError::Conflict) => render_create_form(
            &context,
            &engine,
            FormState::new(values),
            Some("A customer already uses the submitted email or phone. Check the details."),
            StatusCode::CONFLICT,
        ),
        Err(error) => Ok(workflow_response(&context, error, "customer")),
    }
}

fn render_create_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    form: FormState<CustomerFormValues>,
    conflict: Option<&'static str>,
    status: StatusCode,
) -> Result<Response> {
    let view = CustomerFormPage::create(
        layout(context),
        form.with_known_fields(CUSTOMER_FORM_FIELDS),
        conflict,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_form(engine)?,
    ))
}

async fn show(
    context: BrowserRequestContext,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(id): Path<String>,
    Query(query): Query<CustomerDetailQuery>,
) -> Result<Response> {
    let id = match CustomerId::parse(id) {
        Ok(id) => id,
        Err(_) => {
            return Ok(responses::not_found(
                context.response_preference,
                "customer",
            ))
        }
    };
    render_detail(
        &context,
        &engine,
        &customers,
        &vehicles,
        &settings,
        &id,
        query.vehicle_cursor,
        None,
        StatusCode::OK,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn render_detail(
    context: &BrowserRequestContext,
    engine: &TeraView,
    customers: &CustomerService,
    vehicles: &VehicleService,
    settings: &BusinessSettings,
    id: &CustomerId,
    vehicle_cursor: Option<String>,
    lifecycle_message: Option<&'static str>,
    status: StatusCode,
) -> Result<Response> {
    let customer = match customers.get(id).await {
        Ok(customer) => customer,
        Err(error) => return Ok(workflow_response(context, error, "customer")),
    };
    let cursor = match parse_cursor(vehicle_cursor) {
        Ok(cursor) => cursor,
        Err(_) => {
            return Ok(responses::not_found(
                context.response_preference,
                "vehicle page",
            ));
        }
    };
    let vehicle_page = match vehicles
        .list_by_customer(
            id,
            PageRequest {
                filter: VehicleFilter {
                    archive: ArchiveFilter::All,
                    ..VehicleFilter::default()
                },
                limit: settings.default_collection_limit(),
                after: cursor,
            },
        )
        .await
    {
        Ok(page) => page,
        Err(error) => return Ok(workflow_response(context, error, "vehicle section")),
    };
    let next_vehicle_href = vehicle_page.next_cursor.as_ref().map(|cursor| {
        format!(
            "/customers/{}?vehicle_cursor={}",
            id.as_str(),
            cursor.as_str()
        )
    });
    let view = CustomerDetailPage::new(
        layout(context),
        customer,
        vehicle_page,
        next_vehicle_href,
        lifecycle_message,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_content(engine)?,
    ))
}

async fn edit_form(
    context: BrowserRequestContext,
    SharedStore(service): SharedStore<CustomerService>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(id): Path<String>,
) -> Result<Response> {
    let id = match CustomerId::parse(id) {
        Ok(id) => id,
        Err(_) => {
            return Ok(responses::not_found(
                context.response_preference,
                "customer",
            ))
        }
    };
    let customer = match service.get(&id).await {
        Ok(customer) => customer,
        Err(error) => return Ok(workflow_response(&context, error, "customer")),
    };
    if customer.is_archived() {
        return Ok(responses::redirect(
            context.response_preference,
            &format!("/customers/{}", id.as_str()),
        ));
    }
    render_edit_form(
        &context,
        &engine,
        id.as_str(),
        FormState::new(CustomerFormValues::from(customer)),
        None,
        StatusCode::OK,
    )
}

async fn update(
    context: BrowserRequestContext,
    SharedStore(service): SharedStore<CustomerService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(id): Path<String>,
    form: AuthenticatedForm<CustomerFormValues>,
) -> Result<Response> {
    let id = match CustomerId::parse(id) {
        Ok(id) => id,
        Err(_) => {
            return Ok(responses::not_found(
                context.response_preference,
                "customer",
            ))
        }
    };
    match service.get(&id).await {
        Ok(customer) if customer.is_archived() => {
            return render_detail(
                &context,
                &engine,
                &service,
                &vehicles,
                &settings,
                &id,
                None,
                Some("This customer was archived before the edit could be saved. The latest state is shown."),
                StatusCode::CONFLICT,
            )
            .await;
        }
        Ok(_) => {}
        Err(error) => return Ok(workflow_response(&context, error, "customer")),
    }
    let values = form.fields;
    if let Some(errors) = validate_browser_form(&values) {
        return render_edit_form(
            &context,
            &engine,
            id.as_str(),
            FormState::with_validation(values, &errors),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        );
    }
    match service.update(&id, update_command(&values)).await {
        Ok(_) => Ok(responses::redirect(
            context.response_preference,
            &format!("/customers/{}", id.as_str()),
        )),
        Err(WorkflowError::Validation(errors)) => render_edit_form(
            &context,
            &engine,
            id.as_str(),
            FormState::with_validation(values, &errors),
            None,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        Err(WorkflowError::Conflict) => {
            let current = match service.get(&id).await {
                Ok(customer) => customer,
                Err(error) => return Ok(workflow_response(&context, error, "customer")),
            };
            if current.is_archived() {
                return Ok(responses::redirect(
                    context.response_preference,
                    &format!("/customers/{}", id.as_str()),
                ));
            }
            render_edit_form(
                &context,
                &engine,
                id.as_str(),
                FormState::new(CustomerFormValues::from(current)),
                Some("The customer changed while you were editing. The latest saved details are shown."),
                StatusCode::CONFLICT,
            )
        }
        Err(error) => Ok(workflow_response(&context, error, "customer")),
    }
}

fn render_edit_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    id: &str,
    form: FormState<CustomerFormValues>,
    conflict: Option<&'static str>,
    status: StatusCode,
) -> Result<Response> {
    let view = CustomerFormPage::edit(
        layout(context),
        id,
        form.with_known_fields(CUSTOMER_FORM_FIELDS),
        conflict,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_form(engine)?,
    ))
}

async fn archive(
    context: BrowserRequestContext,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(id): Path<String>,
    _form: AuthenticatedForm<LifecycleForm>,
) -> Result<Response> {
    lifecycle(
        context,
        customers,
        vehicles,
        settings,
        engine,
        id,
        Lifecycle::Archive,
    )
    .await
}

async fn restore(
    context: BrowserRequestContext,
    SharedStore(customers): SharedStore<CustomerService>,
    SharedStore(vehicles): SharedStore<VehicleService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    ViewEngine(engine): ViewEngine<TeraView>,
    Path(id): Path<String>,
    _form: AuthenticatedForm<LifecycleForm>,
) -> Result<Response> {
    lifecycle(
        context,
        customers,
        vehicles,
        settings,
        engine,
        id,
        Lifecycle::Restore,
    )
    .await
}

#[derive(Deserialize)]
struct LifecycleForm {}

#[derive(Clone, Copy)]
enum Lifecycle {
    Archive,
    Restore,
}

#[allow(clippy::too_many_arguments)]
async fn lifecycle(
    context: BrowserRequestContext,
    customers: CustomerService,
    vehicles: VehicleService,
    settings: BusinessSettings,
    engine: TeraView,
    raw_id: String,
    action: Lifecycle,
) -> Result<Response> {
    let id = match CustomerId::parse(raw_id) {
        Ok(id) => id,
        Err(_) => {
            return Ok(responses::not_found(
                context.response_preference,
                "customer",
            ))
        }
    };
    let result = match action {
        Lifecycle::Archive => customers.archive(&id).await,
        Lifecycle::Restore => customers.restore(&id).await,
    };
    match result {
        Ok(_) => Ok(responses::redirect(
            context.response_preference,
            &format!("/customers/{}", id.as_str()),
        )),
        Err(WorkflowError::Conflict) => {
            render_detail(
                &context,
                &engine,
                &customers,
                &vehicles,
                &settings,
                &id,
                None,
                Some(
                    "The customer changed before this action completed. The latest state is shown.",
                ),
                StatusCode::CONFLICT,
            )
            .await
        }
        Err(error) => Ok(workflow_response(&context, error, "customer")),
    }
}

fn create_command(values: &CustomerFormValues) -> CreateCustomer {
    CreateCustomer {
        display_name: values.display_name.clone(),
        email: Some(values.email.clone()),
        phone: Some(values.phone.clone()),
        address: Some(address_input(values)),
        notes: Some(values.notes.clone()),
    }
}

fn update_command(values: &CustomerFormValues) -> UpdateCustomer {
    UpdateCustomer {
        display_name: Some(values.display_name.clone()),
        email: Some(Some(values.email.clone())),
        phone: Some(Some(values.phone.clone())),
        address: Some(Some(address_input(values))),
        notes: Some(Some(values.notes.clone())),
    }
}

fn address_input(values: &CustomerFormValues) -> CustomerAddressInput {
    CustomerAddressInput {
        line_1: values.address_line_1.clone(),
        line_2: Some(values.address_line_2.clone()),
        postal_code: values.postal_code.clone(),
        city: values.city.clone(),
        country_code: values.country_code.clone(),
    }
}

fn validate_browser_form(values: &CustomerFormValues) -> Option<ValidationErrors> {
    let mut errors = Vec::new();
    required(
        &mut errors,
        "display_name",
        &values.display_name,
        "Enter a customer name.",
    );
    required(
        &mut errors,
        "address.line_1",
        &values.address_line_1,
        "Enter address line 1.",
    );
    required(
        &mut errors,
        "address.postal_code",
        &values.postal_code,
        "Enter a postal code.",
    );
    required(&mut errors, "address.city", &values.city, "Enter a city.");
    required(
        &mut errors,
        "address.country_code",
        &values.country_code,
        "Enter a two-letter country code.",
    );
    let country_code = values.country_code.trim();
    if !country_code.is_empty()
        && (country_code.len() != 2 || !country_code.bytes().all(|byte| byte.is_ascii_uppercase()))
    {
        push_error(
            &mut errors,
            "address.country_code",
            ValidationCode::InvalidFormat,
            "Use a two-letter uppercase country code.",
        );
    }
    for (field, value, maximum) in [
        (
            "display_name",
            values.display_name.as_str(),
            DISPLAY_NAME_MAX_CHARS,
        ),
        ("email", values.email.as_str(), EMAIL_MAX_CHARS),
        ("phone", values.phone.as_str(), PHONE_MAX_CHARS),
        (
            "address.line_1",
            values.address_line_1.as_str(),
            ADDRESS_LINE_MAX_CHARS,
        ),
        (
            "address.line_2",
            values.address_line_2.as_str(),
            ADDRESS_LINE_MAX_CHARS,
        ),
        (
            "address.postal_code",
            values.postal_code.as_str(),
            POSTAL_CODE_MAX_CHARS,
        ),
        ("address.city", values.city.as_str(), CITY_MAX_CHARS),
        ("notes", values.notes.as_str(), NOTES_MAX_CHARS),
    ] {
        if value.trim().chars().count() > maximum {
            push_error(
                &mut errors,
                field,
                ValidationCode::TooLong,
                "Shorten this value.",
            );
        }
    }
    ValidationErrors::from_vec(errors)
}

fn required(errors: &mut Vec<ValidationError>, field: &str, value: &str, message: &str) {
    if value.trim().is_empty() {
        push_error(errors, field, ValidationCode::Required, message);
    }
}

fn push_error(errors: &mut Vec<ValidationError>, field: &str, code: ValidationCode, message: &str) {
    errors.push(
        ValidationError::new(field, code, message)
            .expect("customer form validation metadata is static and valid"),
    );
}

fn parse_archive(
    value: Option<&str>,
) -> std::result::Result<(ArchiveFilter, &'static str), &'static str> {
    match value.unwrap_or("active") {
        "active" => Ok((ArchiveFilter::Active, "active")),
        "archived" => Ok((ArchiveFilter::Archived, "archived")),
        _ => Err("Choose Active or Archived customers."),
    }
}

fn parse_cursor(value: Option<String>) -> std::result::Result<Option<OpaqueCursor>, &'static str> {
    value
        .map(OpaqueCursor::parse)
        .transpose()
        .map_err(|_| "This page link is no longer valid. Start from the customer list.")
}

fn customer_list_href(query: &str, archive: &str, cursor: &str) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    if !query.is_empty() {
        serializer.append_pair("q", query);
    }
    serializer.append_pair("archived", archive);
    serializer.append_pair("cursor", cursor);
    format!("/customers?{}", serializer.finish())
}

fn layout(context: &BrowserRequestContext) -> AuthenticatedLayout<'_> {
    AuthenticatedLayout::new(
        &context.current_user,
        context.csrf_token.expose(),
        &context.current_path,
    )
}

fn workflow_response(
    context: &BrowserRequestContext,
    error: WorkflowError,
    resource: &str,
) -> Response {
    match error {
        WorkflowError::NotFound => responses::not_found(context.response_preference, resource),
        WorkflowError::Unavailable => responses::unavailable(
            context.response_preference,
            "Customer information is temporarily unavailable. Try again shortly.",
        ),
        WorkflowError::Validation(_) | WorkflowError::Conflict | WorkflowError::Internal => {
            responses::unexpected(context.response_preference)
        }
    }
}

/// Routes owned by customer browser workflows.
#[must_use]
pub fn routes() -> Routes {
    Routes::new()
        .add("/customers", get(list))
        .add("/customers", post(create).layer(body_limit()))
        .add("/customers/new", get(new_form))
        .add("/customers/{id}", get(show))
        .add("/customers/{id}/edit", get(edit_form))
        .add("/customers/{id}/edit", post(update).layer(body_limit()))
        .add("/customers/{id}/archive", post(archive).layer(body_limit()))
        .add("/customers/{id}/restore", post(restore).layer(body_limit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customer_cursor_links_preserve_display_filters() {
        let href = customer_list_href("Jean & Fils", "archived", "opaque_cursor");
        assert_eq!(
            href,
            "/customers?q=Jean+%26+Fils&archived=archived&cursor=opaque_cursor"
        );
    }

    #[test]
    fn browser_customer_validation_keeps_address_fields_distinct() {
        let values = CustomerFormValues {
            display_name: "Filippo".to_owned(),
            ..CustomerFormValues::default()
        };
        let errors = validate_browser_form(&values).expect("address should be required");
        let fields = errors
            .as_slice()
            .iter()
            .map(|error| error.field().as_str())
            .collect::<Vec<_>>();
        assert!(fields.contains(&"address.line_1"));
        assert!(fields.contains(&"address.postal_code"));
        assert!(fields.contains(&"address.city"));
        assert!(fields.contains(&"address.country_code"));
    }
}
