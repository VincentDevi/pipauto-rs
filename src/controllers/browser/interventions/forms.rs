use super::*;
pub(super) const FORM_FIELDS: &[&str] = &[
    "service_date",
    "start_time",
    "estimated_duration_minutes",
    "mileage",
    "customer_reported_problem",
    "diagnostics",
    "performed_work",
    "recommendations",
    "notes",
    "intervention",
];
pub(super) const LINE_FORM_FIELDS: &[&str] = &[
    "category",
    "description",
    "quantity",
    "unit_label",
    "unit_price",
    "unit_cost",
    "position",
];
pub(super) type OptionalUtcBounds = (Option<chrono::DateTime<Utc>>, Option<chrono::DateTime<Utc>>);

#[derive(serde::Deserialize)]
pub(super) struct EmptyForm {
    #[serde(default)]
    _unused: Option<String>,
}

pub(super) fn parse_filter(
    values: &InterventionFilterValues,
    settings: &BusinessSettings,
) -> std::result::Result<InterventionFilter, String> {
    let vehicle_id = if values.vehicle.trim().is_empty() {
        None
    } else {
        Some(
            VehicleId::parse(values.vehicle.clone())
                .map_err(|_| "Choose a valid vehicle.".to_owned())?,
        )
    };
    let status = match values.status.as_str() {
        "" | "all" => None,
        "draft" => Some(InterventionStatus::Draft),
        "completed" => Some(InterventionStatus::Completed),
        "cancelled" => Some(InterventionStatus::Cancelled),
        _ => return Err("Choose All, Draft, Completed, or Cancelled interventions.".to_owned()),
    };
    let from = parse_optional_date(&values.from, "Enter a valid From date.")?;
    let to = parse_optional_date(&values.to, "Enter a valid To date.")?;
    if from.zip(to).is_some_and(|(from, to)| from > to) {
        return Err("The From date must be on or before the To date.".to_owned());
    }
    let (service_date_from, service_date_until) = browser_date_bounds(from, to, settings)?;
    Ok(InterventionFilter {
        vehicle_id,
        status,
        service_date_from,
        service_date_until,
    })
}

pub(super) fn create_command(
    values: &InterventionFormValues,
    vehicle_id: VehicleId,
    currency: crate::domain::CurrencyCode,
    settings: &BusinessSettings,
) -> std::result::Result<CreateIntervention, ValidationErrors> {
    let (service_date, estimated_duration_minutes, mileage) = validate_form(values, settings)?;
    Ok(CreateIntervention {
        vehicle_id,
        service_date,
        estimated_duration_minutes,
        mileage,
        customer_reported_problem: optional_text(&values.customer_reported_problem),
        diagnostics: optional_text(&values.diagnostics),
        performed_work: optional_text(&values.performed_work),
        recommendations: optional_text(&values.recommendations),
        notes: optional_text(&values.notes),
        currency,
    })
}

pub(super) fn update_command(
    values: &InterventionFormValues,
    settings: &BusinessSettings,
) -> std::result::Result<UpdateIntervention, ValidationErrors> {
    let (service_date, estimated_duration_minutes, mileage) = validate_form(values, settings)?;
    Ok(UpdateIntervention {
        service_date: Some(service_date),
        estimated_duration_minutes: Some(estimated_duration_minutes),
        mileage: Some(mileage),
        customer_reported_problem: Some(optional_text(&values.customer_reported_problem)),
        diagnostics: Some(optional_text(&values.diagnostics)),
        performed_work: Some(optional_text(&values.performed_work)),
        recommendations: Some(optional_text(&values.recommendations)),
        notes: Some(optional_text(&values.notes)),
        currency: None,
    })
}

pub(super) fn validate_form(
    values: &InterventionFormValues,
    settings: &BusinessSettings,
) -> std::result::Result<(chrono::DateTime<Utc>, u16, Option<u64>), ValidationErrors> {
    let mut errors = Vec::new();
    let date = parse_exact_form_date(&values.service_date).map_err(|()| {
        errors.push(validation_error(
            "service_date",
            "Enter a valid service date.",
        ));
    });
    let time = parse_exact_form_time(&values.start_time).map_err(|()| {
        errors.push(validation_error("start_time", "Enter a valid start time."));
    });
    let service_date = match (date, time) {
        (Ok(date), Ok(time)) => WorkshopTime::system(settings.workshop_timezone())
            .local_to_utc(&format!("{date}T{}", time.format("%H:%M")))
            .map_err(|error| {
                errors.push(validation_error("start_time", &error.to_string()));
            }),
        _ => Err(()),
    };
    let estimated_duration = values
        .estimated_duration_minutes
        .parse::<u16>()
        .ok()
        .and_then(|minutes| crate::models::intervention::EstimatedDuration::new(minutes).ok())
        .map(crate::models::intervention::EstimatedDuration::minutes)
        .ok_or_else(|| {
            errors.push(validation_error(
                "estimated_duration_minutes",
                "Choose a duration from 30 minutes through 24 hours.",
            ));
        });
    let mileage = if values.mileage.trim().is_empty() {
        Ok(None)
    } else {
        values.mileage.parse::<u64>().map(Some).map_err(|_| {
            errors.push(validation_error(
                "mileage",
                "Enter a non-negative whole mileage.",
            ));
        })
    };
    match (service_date, estimated_duration, mileage) {
        (Ok(date), Ok(duration), Ok(mileage)) => Ok((date, duration, mileage)),
        _ => Err(ValidationErrors::from_vec(errors).expect("form validation errors are non-empty")),
    }
}

pub(super) fn parse_exact_form_date(value: &str) -> std::result::Result<NaiveDate, ()> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| ())
        .and_then(|date| {
            (date.format("%Y-%m-%d").to_string() == value)
                .then_some(date)
                .ok_or(())
        })
}

pub(super) fn parse_exact_form_time(value: &str) -> std::result::Result<NaiveTime, ()> {
    NaiveTime::parse_from_str(value, "%H:%M")
        .map_err(|_| ())
        .and_then(|time| {
            (time.format("%H:%M").to_string() == value)
                .then_some(time)
                .ok_or(())
        })
}

pub(super) fn form_values(
    intervention: &Intervention,
    settings: &BusinessSettings,
) -> InterventionFormValues {
    let local =
        WorkshopTime::system(settings.workshop_timezone()).utc_to_local(intervention.service_date);
    InterventionFormValues {
        service_date: local.format("%Y-%m-%d").to_string(),
        start_time: local.format("%H:%M").to_string(),
        estimated_duration_minutes: intervention.estimated_duration.minutes().to_string(),
        mileage: intervention
            .mileage
            .map_or_else(String::new, |value| value.to_string()),
        customer_reported_problem: intervention
            .customer_reported_problem
            .clone()
            .unwrap_or_default(),
        diagnostics: intervention.diagnostics.clone().unwrap_or_default(),
        performed_work: intervention.performed_work.clone().unwrap_or_default(),
        recommendations: intervention.recommendations.clone().unwrap_or_default(),
        notes: intervention.notes.clone().unwrap_or_default(),
    }
}

pub(super) fn browser_date_bounds(
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    settings: &BusinessSettings,
) -> std::result::Result<OptionalUtcBounds, String> {
    use chrono::Days;

    let workshop_time = WorkshopTime::system(settings.workshop_timezone());
    let from = from
        .map(|date| workshop_time.local_to_utc(&format!("{date}T00:00")))
        .transpose()
        .map_err(|error| error.to_string())?;
    let until = to
        .map(|date| {
            date.checked_add_days(Days::new(1))
                .ok_or(crate::domain::WorkshopTimeError::CalendarBoundaryOutOfRange)
                .and_then(|date| workshop_time.local_to_utc(&format!("{date}T00:00")))
        })
        .transpose()
        .map_err(|error| error.to_string())?;
    Ok((from, until))
}

pub(super) fn line_command(
    values: &InterventionLineFormValues,
) -> std::result::Result<WriteLine, ValidationErrors> {
    let mut errors = Vec::new();
    let category = match values.category.as_str() {
        "labour" => Some(InterventionLineCategory::Labour),
        "part" => Some(InterventionLineCategory::Part),
        "material" => Some(InterventionLineCategory::Material),
        "other" => Some(InterventionLineCategory::Other),
        _ => {
            errors.push(validation_error(
                "category",
                "Choose Labour, Part, Material, or Other.",
            ));
            None
        }
    };
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
    let unit_price = parse_money_input(&values.unit_price).map_err(|_| {
        errors.push(validation_error(
            "unit_price",
            "Enter a non-negative amount with at most two decimal places.",
        ));
    });
    let unit_cost = if values.unit_cost.is_empty() {
        Ok(None)
    } else {
        parse_money_input(&values.unit_cost).map(Some).map_err(|_| {
            errors.push(validation_error(
                "unit_cost",
                "Enter a non-negative amount with at most two decimal places.",
            ));
        })
    };
    let position = values.position.parse::<u32>().map_err(|_| {
        errors.push(validation_error(
            "position",
            "Enter a non-negative whole-number position.",
        ));
    });
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    Ok(WriteLine {
        category: category.expect("validated category"),
        description: values.description.clone(),
        quantity: quantity.expect("validated quantity"),
        unit_label: values.unit_label.clone(),
        unit_price_minor: unit_price.expect("validated unit price"),
        unit_cost_minor: unit_cost.expect("validated unit cost"),
        position: position.expect("validated position"),
    })
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

pub(super) fn attachment_update_command(
    values: &AttachmentFormValues,
    attachment: &AttachmentMetadata,
) -> std::result::Result<WriteAttachmentMetadata, ValidationErrors> {
    let mut errors = Vec::new();
    validate_required_length(
        &mut errors,
        "display_name",
        &values.display_name,
        DISPLAY_NAME_MAX_CHARS,
        "Enter a display name.",
        "Use 255 characters or fewer.",
    );
    if values.caption.trim().chars().count() > CAPTION_MAX_CHARS {
        errors.push(validation_error(
            "caption",
            "Use 1,000 characters or fewer.",
        ));
    }
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    Ok(WriteAttachmentMetadata {
        display_name: values.display_name.clone(),
        media_type: attachment.media_type.as_str().to_owned(),
        byte_size: Some(attachment.byte_size),
        caption: (!values.caption.trim().is_empty()).then(|| values.caption.clone()),
    })
}

pub(super) fn parse_optional_date(
    value: &str,
    message: &str,
) -> std::result::Result<Option<NaiveDate>, String> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| message.to_owned())
    }
}

pub(super) fn parse_cursor(value: &str) -> std::result::Result<Option<OpaqueCursor>, String> {
    if value.is_empty() {
        Ok(None)
    } else {
        OpaqueCursor::parse(value.to_owned())
            .map(Some)
            .map_err(|_| "Use a valid intervention page link.".to_owned())
    }
}

pub(super) fn list_href(filters: &InterventionFilterValues) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in [
        ("vehicle", filters.vehicle.as_str()),
        ("status", filters.status.as_str()),
        ("from", filters.from.as_str()),
        ("to", filters.to.as_str()),
        ("cursor", filters.cursor.as_str()),
    ] {
        if !value.is_empty() {
            serializer.append_pair(key, value);
        }
    }
    format!("/interventions?{}", serializer.finish())
}

pub(super) async fn all_vehicles(
    vehicles: &VehicleService,
) -> std::result::Result<Vec<Vehicle>, WorkflowError> {
    Ok(vehicles
        .list(PageRequest {
            filter: VehicleFilter {
                archive: ArchiveFilter::All,
                ..VehicleFilter::default()
            },
            limit: PageLimit::new(200).expect("maximum page limit is valid"),
            after: None,
        })
        .await?
        .items)
}

pub(super) fn empty_page() -> Page<crate::models::intervention::ServiceHistorySummary> {
    Page {
        items: Vec::new(),
        next_cursor: None,
    }
}

pub(super) fn mileage_error(errors: &ValidationErrors) -> bool {
    errors
        .as_slice()
        .iter()
        .any(|error| error.field().as_str() == "mileage")
}

pub(super) async fn intervention_line(
    interventions: &InterventionService,
    raw_id: String,
    raw_line_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<(Intervention, InterventionLine), Response> {
    let id = intervention_id(raw_id, context)?;
    let line_id = InterventionLineId::parse(raw_line_id)
        .map_err(|_| responses::not_found(context.response_preference, "intervention line"))?;
    let intervention = interventions
        .get(&id)
        .await
        .map_err(|error| workflow_response(context, error, "intervention"))?;
    let workspace = interventions
        .line_workspace(&id)
        .await
        .map_err(|error| workflow_response(context, error, "intervention lines"))?;
    let line = workspace
        .lines
        .into_iter()
        .find(|line| line.id == line_id)
        .ok_or_else(|| responses::not_found(context.response_preference, "intervention line"))?;
    Ok((intervention, line))
}

pub(super) async fn intervention_attachment(
    interventions: &InterventionService,
    attachments: &AttachmentService,
    raw_id: String,
    raw_attachment_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<(Intervention, crate::models::attachment::AttachmentMetadata), Response> {
    let id = intervention_id(raw_id, context)?;
    let attachment_id = AttachmentId::parse(raw_attachment_id)
        .map_err(|_| responses::not_found(context.response_preference, "attachment metadata"))?;
    let intervention = interventions
        .get(&id)
        .await
        .map_err(|error| workflow_response(context, error, "intervention"))?;
    let attachment = attachments
        .get(&attachment_id)
        .await
        .map_err(|error| workflow_response(context, error, "attachment metadata"))?;
    if attachment.owner != AttachmentOwner::Intervention(id) {
        return Err(responses::not_found(
            context.response_preference,
            "intervention attachment metadata",
        ));
    }
    Ok((intervention, attachment))
}

pub(super) fn detail_redirect(context: &BrowserRequestContext, id: &InterventionId) -> Response {
    responses::redirect(
        context.response_preference,
        &format!("/interventions/{}", id.as_str()),
    )
}

pub(super) async fn vehicle(
    vehicles: &VehicleService,
    raw_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<Vehicle, Response> {
    let id = VehicleId::parse(raw_id)
        .map_err(|_| responses::not_found(context.response_preference, "vehicle"))?;
    vehicles
        .get(&id)
        .await
        .map_err(|error| workflow_response(context, error, "vehicle"))
}

#[allow(clippy::result_large_err)]
pub(super) fn intervention_id(
    raw_id: String,
    context: &BrowserRequestContext,
) -> std::result::Result<InterventionId, Response> {
    InterventionId::parse(raw_id)
        .map_err(|_| responses::not_found(context.response_preference, "intervention"))
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
        "Intervention information is temporarily unavailable. Try again shortly.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_links_preserve_supported_filters_only() {
        let href = list_href(&InterventionFilterValues {
            vehicle: "vehicle-1".to_owned(),
            status: "completed".to_owned(),
            from: "2026-01-01".to_owned(),
            to: "2026-07-20".to_owned(),
            cursor: "opaque_cursor".to_owned(),
        });
        assert_eq!(
            href,
            "/interventions?vehicle=vehicle-1&status=completed&from=2026-01-01&to=2026-07-20&cursor=opaque_cursor"
        );
    }
}
