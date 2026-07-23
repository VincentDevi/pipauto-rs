use super::*;
#[derive(Clone, Debug, Default, Deserialize)]
pub(super) struct CustomerQuery {
    #[serde(default)]
    pub(super) q: String,
    pub(super) archived: Option<String>,
    pub(super) cursor: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(super) struct CustomerDetailQuery {
    pub(super) vehicle_cursor: Option<String>,
}

pub(super) const CUSTOMER_FORM_FIELDS: &[&str] = &[
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

pub(super) fn create_command(values: &CustomerFormValues) -> CreateCustomer {
    CreateCustomer {
        display_name: values.display_name.clone(),
        email: Some(values.email.clone()),
        phone: Some(values.phone.clone()),
        address: Some(address_input(values)),
        notes: Some(values.notes.clone()),
    }
}

pub(super) fn update_command(values: &CustomerFormValues) -> UpdateCustomer {
    UpdateCustomer {
        display_name: Some(values.display_name.clone()),
        email: Some(Some(values.email.clone())),
        phone: Some(Some(values.phone.clone())),
        address: Some(Some(address_input(values))),
        notes: Some(Some(values.notes.clone())),
    }
}

pub(super) fn address_input(values: &CustomerFormValues) -> CustomerAddressInput {
    CustomerAddressInput {
        line_1: values.address_line_1.clone(),
        line_2: Some(values.address_line_2.clone()),
        postal_code: values.postal_code.clone(),
        city: values.city.clone(),
        country_code: values.country_code.clone(),
    }
}

pub(super) fn validate_browser_form(values: &CustomerFormValues) -> Option<ValidationErrors> {
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
            .expect("customer form validation metadata is static and valid"),
    );
}

pub(super) fn parse_archive(
    value: Option<&str>,
) -> std::result::Result<(ArchiveFilter, &'static str), &'static str> {
    match value.unwrap_or("active") {
        "active" => Ok((ArchiveFilter::Active, "active")),
        "archived" => Ok((ArchiveFilter::Archived, "archived")),
        _ => Err("Choose Active or Archived customers."),
    }
}

pub(super) fn parse_cursor(
    value: Option<String>,
) -> std::result::Result<Option<OpaqueCursor>, &'static str> {
    value
        .map(OpaqueCursor::parse)
        .transpose()
        .map_err(|_| "This page link is no longer valid. Start from the customer list.")
}

pub(super) fn customer_list_href(query: &str, archive: &str, cursor: &str) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    if !query.is_empty() {
        serializer.append_pair("q", query);
    }
    serializer.append_pair("archived", archive);
    serializer.append_pair("cursor", cursor);
    format!("/customers?{}", serializer.finish())
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
        "Customer information is temporarily unavailable. Try again shortly.",
    )
}

/// Routes owned by customer browser workflows.
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
