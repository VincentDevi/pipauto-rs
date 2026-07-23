use super::forms::*;
use super::*;
pub(super) async fn list(
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
pub(super) fn render_list(
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
        context.layout(),
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

pub(super) async fn new_form(
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

pub(super) async fn create(
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

pub(super) fn render_create_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    form: FormState<CustomerFormValues>,
    conflict: Option<&'static str>,
    status: StatusCode,
) -> Result<Response> {
    let view = CustomerFormPage::create(
        context.layout(),
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

pub(super) async fn show(
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
pub(super) async fn render_detail(
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
        context.layout(),
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

pub(super) async fn edit_form(
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

pub(super) async fn update(
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

pub(super) fn render_edit_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    id: &str,
    form: FormState<CustomerFormValues>,
    conflict: Option<&'static str>,
    status: StatusCode,
) -> Result<Response> {
    let view = CustomerFormPage::edit(
        context.layout(),
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
