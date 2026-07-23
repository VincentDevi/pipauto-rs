use super::*;
pub(super) const VEHICLE_FORM_FIELDS: &[&str] = &[
    "customer_id",
    "make",
    "model",
    "year",
    "registration",
    "vin",
    "current_mileage",
    "engine_type",
    "notes",
];
#[derive(Deserialize)]
pub(super) struct EmptyForm {}

pub(super) async fn active_customers(
    customers: &CustomerService,
) -> std::result::Result<Vec<crate::models::customer::Customer>, WorkflowError> {
    Ok(customers
        .list(PageRequest {
            filter: CustomerFilter {
                query: None,
                archive: ArchiveFilter::Active,
            },
            limit: PageLimit::new(200).expect("maximum page limit is valid"),
            after: None,
        })
        .await?
        .items)
}

pub(super) async fn customers_for_filter(
    customers: &CustomerService,
) -> std::result::Result<Vec<crate::models::customer::Customer>, WorkflowError> {
    Ok(customers
        .list(PageRequest {
            filter: CustomerFilter {
                query: None,
                archive: ArchiveFilter::All,
            },
            limit: PageLimit::new(200).expect("maximum page limit is valid"),
            after: None,
        })
        .await?
        .items)
}

pub(super) fn parse_vehicle_filter(
    values: &VehicleFilterValues,
) -> std::result::Result<VehicleFilter, String> {
    let archive = match values.archived.as_str() {
        "active" | "" => ArchiveFilter::Active,
        "archived" => ArchiveFilter::Archived,
        _ => return Err("Choose Active or Archived vehicles.".to_owned()),
    };
    let customer_id = optional_parse(&values.customer, CustomerId::parse)
        .map_err(|_| "Choose a valid customer.".to_owned())?;
    let registration = optional_parse(&values.registration, |value| {
        NormalizedRegistration::parse(&value)
    })
    .map_err(|_| "Enter a valid exact registration filter.".to_owned())?;
    let vin = optional_parse(&values.vin, |value| NormalizedVin::parse(&value))
        .map_err(|_| "Enter a valid 17-character VIN filter.".to_owned())?;
    Ok(VehicleFilter {
        query: optional_text(&values.q),
        archive,
        customer_id,
        registration,
        vin,
        make: optional_text(&values.make),
        model: optional_text(&values.model),
    })
}

pub(super) fn parse_history_filter(
    values: &HistoryFilterValues,
    settings: &BusinessSettings,
) -> std::result::Result<InterventionFilter, String> {
    let status = match values.status.as_str() {
        "" | "all" => None,
        "draft" => Some(InterventionStatus::Draft),
        "completed" => Some(InterventionStatus::Completed),
        "cancelled" => Some(InterventionStatus::Cancelled),
        _ => return Err("Choose All, Draft, Completed, or Cancelled history.".to_owned()),
    };
    let from = parse_date(&values.from, "Enter a valid From date.")?;
    let to = parse_date(&values.to, "Enter a valid To date.")?;
    if from.zip(to).is_some_and(|(from, to)| from > to) {
        return Err("The From date must be on or before the To date.".to_owned());
    }
    let workshop_time = WorkshopTime::system(settings.workshop_timezone());
    let service_date_from = from
        .map(|date| workshop_time.local_to_utc(&format!("{date}T00:00")))
        .transpose()
        .map_err(|error| error.to_string())?;
    let service_date_until = to
        .map(|date| {
            date.checked_add_days(chrono::Days::new(1))
                .ok_or(crate::domain::WorkshopTimeError::CalendarBoundaryOutOfRange)
                .and_then(|date| workshop_time.local_to_utc(&format!("{date}T00:00")))
        })
        .transpose()
        .map_err(|error| error.to_string())?;
    Ok(InterventionFilter {
        vehicle_id: None,
        status,
        service_date_from,
        service_date_until,
    })
}

pub(super) fn parse_date(
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

pub(super) fn vehicle_create_command(
    values: &VehicleFormValues,
) -> std::result::Result<CreateVehicle, ValidationErrors> {
    let (customer_id, year, mileage) = validate_vehicle_values(values)?;
    Ok(CreateVehicle {
        customer_id,
        make: values.make.clone(),
        model: values.model.clone(),
        year,
        registration: Some(values.registration.clone()),
        vin: Some(values.vin.clone()),
        current_mileage: mileage,
        engine_type: Some(values.engine_type.clone()),
        notes: Some(values.notes.clone()),
    })
}

pub(super) fn vehicle_update_command(
    values: &VehicleFormValues,
) -> std::result::Result<UpdateVehicle, ValidationErrors> {
    let (_, year, mileage) = validate_vehicle_values(values)?;
    Ok(UpdateVehicle {
        customer_id: None,
        make: Some(values.make.clone()),
        model: Some(values.model.clone()),
        year: Some(year),
        registration: Some(Some(values.registration.clone())),
        vin: Some(Some(values.vin.clone())),
        current_mileage: Some(mileage),
        engine_type: Some(Some(values.engine_type.clone())),
        notes: Some(Some(values.notes.clone())),
    })
}

pub(super) fn validate_vehicle_values(
    values: &VehicleFormValues,
) -> std::result::Result<(CustomerId, Option<i32>, Option<u64>), ValidationErrors> {
    let mut errors = Vec::new();
    required(
        &mut errors,
        "customer_id",
        &values.customer_id,
        "Choose an active customer.",
    );
    required(&mut errors, "make", &values.make, "Enter the vehicle make.");
    required(
        &mut errors,
        "model",
        &values.model,
        "Enter the vehicle model.",
    );
    for (field, value, maximum) in [
        ("make", values.make.as_str(), MAKE_MAX_CHARS),
        ("model", values.model.as_str(), MODEL_MAX_CHARS),
        (
            "registration",
            values.registration.as_str(),
            REGISTRATION_MAX_CHARS,
        ),
        ("vin", values.vin.as_str(), VIN_DISPLAY_MAX_CHARS),
        (
            "engine_type",
            values.engine_type.as_str(),
            ENGINE_TYPE_MAX_CHARS,
        ),
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
    let customer_id = CustomerId::parse(values.customer_id.clone()).map_err(|_| ());
    if customer_id.is_err() && !values.customer_id.trim().is_empty() {
        push_error(
            &mut errors,
            "customer_id",
            ValidationCode::InvalidFormat,
            "Choose an active customer.",
        );
    }
    let year = if values.year.trim().is_empty() {
        Ok(None)
    } else {
        values
            .year
            .trim()
            .parse::<i32>()
            .ok()
            .filter(|year| (EARLIEST_VEHICLE_YEAR..=Utc::now().year() + 1).contains(year))
            .map(Some)
            .ok_or(())
    };
    if year.is_err() {
        push_error(
            &mut errors,
            "year",
            ValidationCode::OutOfRange,
            "Enter a year from 1886 through next year.",
        );
    }
    let mileage = parse_optional_u64(&values.current_mileage);
    if mileage.is_err() {
        push_error(
            &mut errors,
            "current_mileage",
            ValidationCode::OutOfRange,
            "Enter a non-negative whole-number mileage.",
        );
    }
    if !values.vin.trim().is_empty() && NormalizedVin::parse(&values.vin).is_err() {
        push_error(
            &mut errors,
            "vin",
            ValidationCode::InvalidFormat,
            "Enter a 17-character VIN without I, O, or Q.",
        );
    }
    if !values.registration.trim().is_empty()
        && NormalizedRegistration::parse(&values.registration).is_err()
    {
        push_error(
            &mut errors,
            "registration",
            ValidationCode::InvalidFormat,
            "Enter a valid registration.",
        );
    }
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    Ok((
        customer_id.expect("validated customer id"),
        year.expect("validated year"),
        mileage.expect("validated mileage"),
    ))
}

pub(super) fn attachment_update_command(
    values: &AttachmentFormValues,
    attachment: &AttachmentMetadata,
) -> std::result::Result<WriteAttachmentMetadata, ValidationErrors> {
    let mut errors = Vec::new();
    required(
        &mut errors,
        "display_name",
        &values.display_name,
        "Enter a display name.",
    );
    if values.display_name.trim().chars().count() > DISPLAY_NAME_MAX_CHARS {
        push_error(
            &mut errors,
            "display_name",
            ValidationCode::TooLong,
            "Use 255 characters or fewer.",
        );
    }
    if values.caption.trim().chars().count() > CAPTION_MAX_CHARS {
        push_error(
            &mut errors,
            "caption",
            ValidationCode::TooLong,
            "Use 1,000 characters or fewer.",
        );
    }
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    Ok(WriteAttachmentMetadata {
        display_name: values.display_name.clone(),
        media_type: attachment.media_type.as_str().to_owned(),
        byte_size: Some(attachment.byte_size),
        caption: Some(values.caption.clone()),
    })
}

pub(super) fn parse_optional_u64(value: &str) -> std::result::Result<Option<u64>, ()> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        value.trim().parse::<u64>().map(Some).map_err(|_| ())
    }
}

pub(super) fn required(errors: &mut Vec<ValidationError>, field: &str, value: &str, message: &str) {
    if value.trim().is_empty() {
        push_error(errors, field, ValidationCode::Required, message);
    }
}

pub(super) fn push_error(
    errors: &mut Vec<ValidationError>,
    field: &str,
    code: ValidationCode,
    message: &str,
) {
    errors.push(
        ValidationError::new(field, code, message)
            .expect("vehicle browser validation metadata is static and valid"),
    );
}

pub(super) fn parse_cursor(value: &str) -> std::result::Result<Option<OpaqueCursor>, String> {
    if value.is_empty() {
        Ok(None)
    } else {
        OpaqueCursor::parse(value.to_owned())
            .map(Some)
            .map_err(|_| "This page link is no longer valid. Start from the first page.".to_owned())
    }
}

pub(super) fn optional_parse<T, E>(
    value: &str,
    parser: impl FnOnce(String) -> std::result::Result<T, E>,
) -> std::result::Result<Option<T>, E> {
    if value.trim().is_empty() {
        Ok(None)
    } else {
        parser(value.to_owned()).map(Some)
    }
}

pub(super) fn vehicle_list_href(filters: &VehicleFilterValues) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in [
        ("q", filters.q.as_str()),
        ("customer", filters.customer.as_str()),
        ("registration", filters.registration.as_str()),
        ("vin", filters.vin.as_str()),
        ("make", filters.make.as_str()),
        ("model", filters.model.as_str()),
        ("archived", filters.archived.as_str()),
        ("cursor", filters.cursor.as_str()),
    ] {
        if !value.is_empty() {
            serializer.append_pair(key, value);
        }
    }
    format!("/vehicles?{}", serializer.finish())
}

pub(super) fn history_href(id: &VehicleId, filters: &HistoryFilterValues) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in [
        ("status", filters.status.as_str()),
        ("from", filters.from.as_str()),
        ("to", filters.to.as_str()),
        ("cursor", filters.cursor.as_str()),
    ] {
        if !value.is_empty() {
            serializer.append_pair(key, value);
        }
    }
    format!("/vehicles/{}/history?{}", id.as_str(), serializer.finish())
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
        "Vehicle information is temporarily unavailable. Try again shortly.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vehicle_cursor_links_preserve_all_filters() {
        let href = vehicle_list_href(&VehicleFilterValues {
            q: "Golf & GTE".to_owned(),
            registration: "1-abc-234".to_owned(),
            archived: "archived".to_owned(),
            cursor: "opaque_cursor".to_owned(),
            ..VehicleFilterValues::default()
        });
        assert_eq!(
            href,
            "/vehicles?q=Golf+%26+GTE&registration=1-abc-234&archived=archived&cursor=opaque_cursor"
        );
    }

    #[test]
    fn vehicle_form_rejects_negative_mileage_and_invalid_vin() {
        let values = VehicleFormValues {
            customer_id: "customer-1".to_owned(),
            make: "Volkswagen".to_owned(),
            model: "Golf".to_owned(),
            vin: "WVWZZZ1JZXW00000I".to_owned(),
            current_mileage: "-1".to_owned(),
            ..VehicleFormValues::default()
        };
        let errors = validate_vehicle_values(&values).expect_err("values should be invalid");
        let fields = errors
            .as_slice()
            .iter()
            .map(|error| error.field().as_str())
            .collect::<Vec<_>>();
        assert!(fields.contains(&"vin"));
        assert!(fields.contains(&"current_mileage"));
    }

    #[test]
    fn service_history_links_keep_status_and_date_cursor_binding() {
        let href = history_href(
            &VehicleId::parse("golf").expect("valid id"),
            &HistoryFilterValues {
                status: "cancelled".to_owned(),
                from: "2026-01-01".to_owned(),
                to: "2026-07-20".to_owned(),
                cursor: "opaque_cursor".to_owned(),
            },
        );
        assert_eq!(
            href,
            "/vehicles/golf/history?status=cancelled&from=2026-01-01&to=2026-07-20&cursor=opaque_cursor"
        );
    }
}
