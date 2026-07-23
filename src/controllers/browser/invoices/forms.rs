use super::*;
pub(super) const FORM_FIELDS: &[&str] = &[
    "customer_id",
    "vehicle_id",
    "intervention_id",
    "currency",
    "notes",
];
pub(super) const LINE_FORM_FIELDS: &[&str] = &[
    "source_intervention_line_id",
    "description",
    "quantity",
    "unit_label",
    "unit_price",
    "position",
];
pub(super) const ISSUE_FORM_FIELDS: &[&str] = &["issue_date", "due_date"];
pub(super) const PAYMENT_FORM_FIELDS: &[&str] =
    &["amount", "received_at", "method", "reference", "notes"];
pub(super) const VOID_FORM_FIELDS: &[&str] = &["reason"];

#[derive(Clone, Debug, Default, Deserialize)]
pub(super) struct NewInvoiceQuery {
    pub(super) customer: Option<String>,
    pub(super) vehicle: Option<String>,
    pub(super) intervention: Option<String>,
}

pub(super) fn create_command(
    values: &InvoiceFormValues,
    authoritative_currency: CurrencyCode,
) -> std::result::Result<CreateInvoice, ValidationErrors> {
    let (customer_id, vehicle_id, intervention_id, currency) = parsed_header(values)?;
    if currency != authoritative_currency {
        return Err(ValidationErrors::one(validation_error(
            "currency",
            "Use the authoritative workshop currency.",
        )));
    }
    Ok(CreateInvoice {
        customer_id,
        vehicle_id,
        intervention_id,
        currency,
        notes: optional_text(&values.notes),
    })
}

pub(super) fn issue_command(
    values: &IssueInvoiceFormValues,
) -> std::result::Result<IssueInvoiceCommand, ValidationErrors> {
    let mut errors = Vec::new();
    let issue_date = NaiveDate::parse_from_str(&values.issue_date, "%Y-%m-%d").map_err(|_| {
        errors.push(validation_error("issue_date", "Enter a valid issue date."));
    });
    let due_date = if values.due_date.trim().is_empty() {
        Ok(None)
    } else {
        NaiveDate::parse_from_str(&values.due_date, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| {
                errors.push(validation_error("due_date", "Enter a valid due date."));
            })
    };
    if let (Ok(issue_date), Ok(Some(due_date))) = (&issue_date, &due_date) {
        if due_date < issue_date {
            errors.push(validation_error(
                "due_date",
                "Due date cannot precede the issue date.",
            ));
        }
    }
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    Ok(IssueInvoiceCommand {
        issue_date: issue_date.expect("validated issue date"),
        due_date: due_date.expect("validated due date"),
    })
}

pub(super) fn payment_command(
    values: &PaymentFormValues,
    currency: CurrencyCode,
) -> std::result::Result<RecordPayment, ValidationErrors> {
    let mut errors = Vec::new();
    let amount_minor =
        parse_money_input(&values.amount).and_then(
            |amount| {
                if amount > 0 {
                    Ok(amount)
                } else {
                    Err(())
                }
            },
        );
    if amount_minor.is_err() {
        errors.push(validation_error(
            "amount",
            "Enter a positive amount with at most two decimal places.",
        ));
    }
    let received_at = NaiveDateTime::parse_from_str(&values.received_at, "%Y-%m-%dT%H:%M")
        .map(|received_at| received_at.and_utc());
    if received_at.is_err() {
        errors.push(validation_error(
            "received_at",
            "Enter the received date and time in UTC.",
        ));
    }
    let method = match values.method.as_str() {
        "cash" => Ok(PaymentMethod::Cash),
        "bank_transfer" => Ok(PaymentMethod::BankTransfer),
        "card" => Ok(PaymentMethod::Card),
        "other" => Ok(PaymentMethod::Other),
        _ => Err(()),
    };
    if method.is_err() {
        errors.push(validation_error("method", "Choose a payment method."));
    }
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    Ok(RecordPayment {
        amount_minor: amount_minor.expect("validated amount"),
        currency,
        received_at: received_at.expect("validated received time"),
        method: method.expect("validated payment method"),
        reference: optional_text(&values.reference),
        notes: optional_text(&values.notes),
    })
}

pub(super) fn void_reason(
    values: &VoidInvoiceFormValues,
) -> std::result::Result<String, ValidationErrors> {
    let value = values.reason.trim();
    if value.is_empty() {
        return Err(ValidationErrors::one(validation_error(
            "reason",
            "Enter the reason for voiding this invoice.",
        )));
    }
    if value.chars().count() > NOTES_MAX_CHARS {
        return Err(ValidationErrors::one(validation_error(
            "reason",
            "Use 10,000 characters or fewer.",
        )));
    }
    Ok(value.to_owned())
}

pub(super) fn update_command(
    values: &InvoiceFormValues,
) -> std::result::Result<UpdateInvoice, ValidationErrors> {
    let (customer_id, vehicle_id, intervention_id, currency) = parsed_header(values)?;
    Ok(UpdateInvoice {
        customer_id: Some(customer_id),
        vehicle_id: Some(vehicle_id),
        intervention_id: Some(intervention_id),
        currency: Some(currency),
        notes: Some(optional_text(&values.notes)),
    })
}

pub(super) fn parsed_header(
    values: &InvoiceFormValues,
) -> std::result::Result<
    (
        CustomerId,
        Option<VehicleId>,
        Option<InterventionId>,
        CurrencyCode,
    ),
    ValidationErrors,
> {
    let mut errors = Vec::new();
    let customer_id = CustomerId::parse(values.customer_id.clone()).map_err(|_| {
        errors.push(validation_error(
            "customer_id",
            "Choose an active customer.",
        ));
    });
    let vehicle_id = optional_id(&values.vehicle_id, VehicleId::parse).map_err(|_| {
        errors.push(validation_error("vehicle_id", "Choose a valid vehicle."));
    });
    let intervention_id =
        optional_id(&values.intervention_id, InterventionId::parse).map_err(|_| {
            errors.push(validation_error(
                "intervention_id",
                "Choose a valid intervention.",
            ));
        });
    let currency = CurrencyCode::parse(&values.currency).map_err(|_| {
        errors.push(validation_error(
            "currency",
            "Use the authoritative workshop currency.",
        ));
    });
    if values.notes.trim().chars().count() > 10_000 {
        errors.push(validation_error("notes", "Use 10,000 characters or fewer."));
    }
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    Ok((
        customer_id.expect("validated customer"),
        vehicle_id.expect("validated vehicle"),
        intervention_id.expect("validated intervention"),
        currency.expect("validated currency"),
    ))
}

pub(super) fn line_command(
    values: &InvoiceLineFormValues,
) -> std::result::Result<WriteInvoiceLine, ValidationErrors> {
    let mut errors = Vec::new();
    let source_intervention_line_id = optional_id(
        &values.source_intervention_line_id,
        InterventionLineId::parse,
    )
    .map_err(|_| {
        errors.push(validation_error(
            "source_intervention_line_id",
            "Choose a source line from the related intervention.",
        ));
    });
    validate_required_length(
        &mut errors,
        "description",
        &values.description,
        DESCRIPTION_MAX_CHARS,
        "Enter a line description.",
        "Use 500 characters or fewer.",
    );
    validate_required_length(
        &mut errors,
        "unit_label",
        &values.unit_label,
        UNIT_LABEL_MAX_CHARS,
        "Enter a unit label.",
        "Use 32 characters or fewer.",
    );
    let quantity = Quantity::parse(&values.quantity).map_err(|_| {
        errors.push(validation_error(
            "quantity",
            "Enter a positive quantity with up to three decimal places.",
        ));
    });
    let unit_price_minor = parse_money_input(&values.unit_price).map_err(|_| {
        errors.push(validation_error(
            "unit_price",
            "Enter a non-negative amount with at most two decimal places.",
        ));
    });
    let position = values.position.parse::<u32>().map_err(|_| {
        errors.push(validation_error(
            "position",
            "Enter a non-negative whole-number position.",
        ));
    });
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    Ok(WriteInvoiceLine {
        source_intervention_line_id: source_intervention_line_id.expect("validated source line"),
        description: values.description.clone(),
        quantity: quantity.expect("validated quantity"),
        unit_label: values.unit_label.clone(),
        unit_price_minor: unit_price_minor.expect("validated unit price"),
        position: position.expect("validated position"),
    })
}

pub(super) async fn prefill(
    customers: &CustomerService,
    vehicles: &VehicleService,
    interventions: &InterventionService,
    settings: &BusinessSettings,
    query: NewInvoiceQuery,
) -> (InvoiceFormValues, Option<String>) {
    let mut values = InvoiceFormValues {
        customer_id: query.customer.unwrap_or_default(),
        vehicle_id: query.vehicle.unwrap_or_default(),
        intervention_id: query.intervention.unwrap_or_default(),
        currency: settings.default_currency().as_str().to_owned(),
        notes: String::new(),
    };
    let result = async {
        if !values.intervention_id.is_empty() {
            let id = InterventionId::parse(values.intervention_id.clone()).map_err(|_| ())?;
            let intervention = interventions.get(&id).await.map_err(|_| ())?;
            values.vehicle_id = intervention.vehicle_id.as_str().to_owned();
        }
        if !values.vehicle_id.is_empty() {
            let id = VehicleId::parse(values.vehicle_id.clone()).map_err(|_| ())?;
            let vehicle = vehicles.get(&id).await.map_err(|_| ())?;
            values.customer_id = vehicle.customer_id.as_str().to_owned();
        }
        if !values.customer_id.is_empty() {
            let id = CustomerId::parse(values.customer_id.clone()).map_err(|_| ())?;
            let customer = customers.get(&id).await.map_err(|_| ())?;
            if customer.is_archived() {
                return Err(());
            }
        }
        Ok::<(), ()>(())
    }
    .await;
    let conflict = result.err().map(|()| {
        "The requested prefill is no longer an active, consistent relationship. Review the preserved references and choose valid records.".to_owned()
    });
    (values, conflict)
}

pub(super) async fn all_customers(
    service: &CustomerService,
) -> std::result::Result<Vec<Customer>, WorkflowError> {
    Ok(service
        .list(PageRequest {
            filter: CustomerFilter {
                query: None,
                archive: ArchiveFilter::All,
            },
            limit: maximum_limit(),
            after: None,
        })
        .await?
        .items)
}

pub(super) async fn all_vehicles(
    service: &VehicleService,
) -> std::result::Result<Vec<Vehicle>, WorkflowError> {
    Ok(service
        .list(PageRequest {
            filter: VehicleFilter {
                archive: ArchiveFilter::All,
                ..VehicleFilter::default()
            },
            limit: maximum_limit(),
            after: None,
        })
        .await?
        .items)
}

pub(super) async fn all_interventions(
    service: &InterventionService,
) -> std::result::Result<Vec<Intervention>, WorkflowError> {
    Ok(service
        .list(PageRequest {
            filter: InterventionFilter::default(),
            limit: maximum_limit(),
            after: None,
        })
        .await?
        .items
        .into_iter()
        .map(|summary| summary.intervention)
        .collect())
}

pub(super) async fn draft_invoice(
    invoices: &InvoiceService,
    raw_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<InvoiceView, Response> {
    let id = invoice_id(raw_id, context)?;
    let invoice = invoices
        .get(&id)
        .await
        .map_err(|error| workflow_response(context, error, "invoice"))?;
    if invoice.invoice.invoice.status != InvoiceStatus::Draft {
        return Err(responses::redirect(
            context.response_preference,
            &format!("/invoices/{}", id.as_str()),
        ));
    }
    Ok(invoice)
}

pub(super) async fn invoice_line(
    invoices: &InvoiceService,
    raw_id: String,
    raw_line_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<(InvoiceView, crate::models::invoice_line::InvoiceLineRecord), Response> {
    let invoice = draft_invoice(invoices, raw_id, context).await?;
    let line_id = InvoiceLineId::parse(raw_line_id)
        .map_err(|_| responses::not_found(context.response_preference, "invoice line"))?;
    let line = invoice
        .lines
        .iter()
        .find(|line| line.id == line_id)
        .cloned()
        .ok_or_else(|| responses::not_found(context.response_preference, "invoice line"))?;
    Ok((invoice, line))
}

pub(super) fn invoice_filter(value: &str) -> std::result::Result<InvoiceFilter, String> {
    let status = match value {
        "" | "all" => None,
        "draft" => Some(InvoiceStatus::Draft),
        "issued" => Some(InvoiceStatus::Issued),
        "void" => Some(InvoiceStatus::Void),
        _ => return Err("Choose Draft, Issued, Void, or All.".into()),
    };
    Ok(InvoiceFilter { status })
}

pub(super) fn parse_cursor(value: &str) -> std::result::Result<Option<OpaqueCursor>, String> {
    if value.is_empty() {
        Ok(None)
    } else {
        OpaqueCursor::parse(value.to_owned())
            .map(Some)
            .map_err(|_| "Use a valid invoice page link.".to_owned())
    }
}

pub(super) fn list_href(filters: &InvoiceFilterValues) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    if !filters.status.is_empty() {
        serializer.append_pair("status", &filters.status);
    }
    if !filters.cursor.is_empty() {
        serializer.append_pair("cursor", &filters.cursor);
    }
    format!("/invoices?{}", serializer.finish())
}

pub(super) fn optional_id<T, E>(
    value: &str,
    parse: impl FnOnce(String) -> std::result::Result<T, E>,
) -> std::result::Result<Option<T>, E> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        parse(value.to_owned()).map(Some)
    }
}

pub(super) fn validate_required_length(
    errors: &mut Vec<ValidationError>,
    field: &str,
    value: &str,
    maximum: usize,
    required_message: &str,
    length_message: &str,
) {
    if value.trim().is_empty() {
        errors.push(validation_error(field, required_message));
    } else if value.trim().chars().count() > maximum {
        errors.push(validation_error(field, length_message));
    }
}

pub(super) fn maximum_limit() -> PageLimit {
    PageLimit::new(200).expect("maximum page limit is valid")
}

pub(super) fn empty_page() -> Page<InvoiceView> {
    Page {
        items: Vec::new(),
        next_cursor: None,
    }
}

pub(super) fn detail_redirect(context: &BrowserRequestContext, id: &InvoiceId) -> Response {
    responses::redirect(
        context.response_preference,
        &format!("/invoices/{}", id.as_str()),
    )
}

#[allow(clippy::result_large_err)]
pub(super) fn invoice_id(
    raw_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<InvoiceId, Response> {
    InvoiceId::parse(raw_id)
        .map_err(|_| responses::not_found(context.response_preference, "invoice"))
}

pub(super) fn workflow_response(
    context: &BrowserRequestContext,
    error: WorkflowError,
    resource: &str,
) -> Response {
    responses::workflow_error(
        context.response_preference,
        error,
        resource,
        "Invoice information is temporarily unavailable. Try again shortly.",
    )
}
