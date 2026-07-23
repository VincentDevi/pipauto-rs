use super::*;
pub(super) const FORM_FIELDS: &[&str] = &[
    "title",
    "body",
    "tags",
    "make",
    "model",
    "engine",
    "vehicle_id",
    "source_intervention_id",
];
// Preserve the former global 64 KiB ceiling now that multipart raises the global middleware.
pub(super) const FORM_BODY_LIMIT: usize = 64 * 1_024;
#[derive(Debug, Default, Deserialize)]
pub(super) struct NewNoteQuery {
    #[serde(default)]
    pub(super) vehicle: String,
    #[serde(default)]
    pub(super) source_intervention: String,
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn render_form(
    context: &BrowserRequestContext,
    engine: &TeraView,
    vehicles: &VehicleService,
    interventions: &InterventionService,
    editing: bool,
    action: String,
    cancel_href: String,
    form: FormState<KnowledgeFormValues>,
    conflict: Option<String>,
    status: StatusCode,
) -> Result<Response> {
    let selected_vehicle = form.values.vehicle_id.clone();
    let selected_source = form.values.source_intervention_id.clone();
    let (mut vehicle_options, mut source_options) = match options(vehicles, interventions).await {
        Ok(value) => value,
        Err(error) => return Ok(workflow_response(context, error, "knowledge context")),
    };
    if !selected_vehicle.is_empty()
        && !vehicle_options
            .iter()
            .any(|vehicle| vehicle.id.as_str() == selected_vehicle)
    {
        if let Ok(id) = VehicleId::parse(selected_vehicle) {
            if let Ok(vehicle) = vehicles.get(&id).await {
                vehicle_options.push(vehicle);
            }
        }
    }
    if !selected_source.is_empty()
        && !source_options
            .iter()
            .any(|(source, _)| source.id.as_str() == selected_source)
    {
        if let Ok(id) = InterventionId::parse(selected_source) {
            if let Ok(source) = interventions.get(&id).await {
                if let Ok(vehicle) = vehicles.get(&source.vehicle_id).await {
                    source_options.push((source, vehicle));
                }
            }
        }
    }
    let view = KnowledgeFormPage::new(
        context.layout(),
        editing,
        action,
        cancel_href,
        form.with_known_fields(FORM_FIELDS),
        vehicle_options,
        source_options,
        conflict,
    );
    Ok(responses::render(
        context.response_preference,
        status,
        view.render_page(engine)?,
        view.render_form(engine)?,
    ))
}

pub(super) async fn options(
    vehicles: &VehicleService,
    interventions: &InterventionService,
) -> std::result::Result<(Vec<Vehicle>, Vec<(Intervention, Vehicle)>), WorkflowError> {
    let vehicle_options = vehicles
        .list(PageRequest {
            filter: VehicleFilter {
                archive: ArchiveFilter::All,
                ..VehicleFilter::default()
            },
            limit: maximum_limit(),
            after: None,
        })
        .await?
        .items;
    let summaries = interventions
        .list(PageRequest {
            filter: InterventionFilter::default(),
            limit: maximum_limit(),
            after: None,
        })
        .await?
        .items;
    let mut source_options = Vec::with_capacity(summaries.len());
    for summary in summaries {
        let intervention = interventions.get(&summary.intervention.id).await?;
        let vehicle = vehicles.get(&intervention.vehicle_id).await?;
        source_options.push((intervention, vehicle));
    }
    Ok((vehicle_options, source_options))
}

pub(super) async fn apply_resolution(
    values: &mut KnowledgeFormValues,
    interventions: &InterventionService,
) -> std::result::Result<(), String> {
    if values.source_intervention_id.is_empty() {
        return Ok(());
    }
    if values.resolution == "remove_source" {
        values.source_intervention_id.clear();
        values.resolution.clear();
        return Ok(());
    }
    if values.vehicle_id.is_empty() || values.resolution == "source_vehicle" {
        if values.resolution != "source_vehicle" {
            return Err(source_conflict());
        }
        let id = InterventionId::parse(values.source_intervention_id.clone())
            .map_err(|_| source_conflict())?;
        let source = interventions
            .get(&id)
            .await
            .map_err(|_| source_conflict())?;
        values.vehicle_id = source.vehicle_id.as_str().to_owned();
        values.resolution.clear();
    }
    Ok(())
}

pub(super) fn command(
    values: &KnowledgeFormValues,
) -> std::result::Result<NewTechnicalNote, ValidationErrors> {
    let tags = parse_tags(&values.tags)?;
    let mut errors = Vec::new();
    required(&mut errors, "title", &values.title, TITLE_MAX_CHARS);
    required(&mut errors, "body", &values.body, BODY_MAX_CHARS);
    optional(&mut errors, "make", &values.make, MAKE_MAX_CHARS);
    optional(&mut errors, "model", &values.model, MODEL_MAX_CHARS);
    optional(&mut errors, "engine", &values.engine, ENGINE_MAX_CHARS);
    let vehicle_id = parse_optional_id(
        &values.vehicle_id,
        "vehicle_id",
        "Choose a valid related vehicle.",
        VehicleId::parse,
        &mut errors,
    );
    let source_id = parse_optional_id(
        &values.source_intervention_id,
        "source_intervention_id",
        "Choose a valid source intervention.",
        InterventionId::parse,
        &mut errors,
    );
    if let Some(errors) = ValidationErrors::from_vec(errors) {
        return Err(errors);
    }
    validate_write(
        values.title.clone(),
        values.body.clone(),
        tags,
        vehicle_id,
        source_id,
        optional_text(&values.make),
        optional_text(&values.model),
        optional_text(&values.engine),
    )
    .map_err(|error| match error {
        WorkflowError::Validation(errors) => errors,
        _ => validation_errors("title", "Check the technical-note values."),
    })
}

pub(super) fn parse_tags(value: &str) -> std::result::Result<Vec<String>, ValidationErrors> {
    let raw = value
        .lines()
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    let mut tags = Vec::with_capacity(raw.len());
    for tag in raw {
        if tag.chars().count() > TAG_MAX_CHARS {
            return Err(validation_errors(
                "tags",
                "Each tag must be 64 characters or fewer.",
            ));
        }
        let normalized = normalize_search_text(tag);
        if !tags.contains(&normalized) {
            tags.push(normalized);
        }
    }
    if tags.len() > TAG_MAX_COUNT {
        return Err(validation_errors("tags", "Use no more than 20 tags."));
    }
    Ok(tags)
}

pub(super) fn required(
    errors: &mut Vec<ValidationError>,
    field: &str,
    value: &str,
    maximum: usize,
) {
    if value.trim().is_empty() {
        errors.push(error(field, ValidationCode::Required, "Enter a value."));
    } else if value.trim().chars().count() > maximum {
        errors.push(error(
            field,
            ValidationCode::TooLong,
            format!("Use {maximum} characters or fewer."),
        ));
    }
}

pub(super) fn optional(
    errors: &mut Vec<ValidationError>,
    field: &str,
    value: &str,
    maximum: usize,
) {
    if value.trim().chars().count() > maximum {
        errors.push(error(
            field,
            ValidationCode::TooLong,
            format!("Use {maximum} characters or fewer."),
        ));
    }
}

pub(super) fn parse_optional_id<T, E>(
    value: &str,
    field: &str,
    message: &str,
    parser: impl FnOnce(String) -> std::result::Result<T, E>,
    errors: &mut Vec<ValidationError>,
) -> Option<T> {
    if value.is_empty() {
        return None;
    }
    match parser(value.to_owned()) {
        Ok(value) => Some(value),
        Err(_) => {
            errors.push(error(field, ValidationCode::InvalidFormat, message));
            None
        }
    }
}

pub(super) fn error(
    field: &str,
    code: ValidationCode,
    message: impl Into<String>,
) -> ValidationError {
    ValidationError::new(field, code, message).expect("static field path is valid")
}

pub(super) fn validation_errors(field: &str, message: &str) -> ValidationErrors {
    ValidationErrors::one(error(field, ValidationCode::InvalidFormat, message))
}

pub(super) fn parse_filter(
    values: &KnowledgeFilterValues,
) -> std::result::Result<TechnicalNoteFilter, String> {
    let tags =
        parse_tags(&values.tags).map_err(|errors| errors.as_slice()[0].message().to_owned())?;
    let archive = match values.archived.as_str() {
        "" | "active" => ArchiveFilter::Active,
        "archived" => ArchiveFilter::Archived,
        "all" => ArchiveFilter::All,
        _ => return Err("Choose Active, Archived, or All notes.".to_owned()),
    };
    Ok(TechnicalNoteFilter {
        query: normalized_optional(&values.q),
        tags,
        make: normalized_optional(&values.make),
        model: normalized_optional(&values.model),
        engine: normalized_optional(&values.engine),
        archive,
    })
}

pub(super) fn normalized_optional(value: &str) -> Option<String> {
    let value = normalize_search_text(value);
    (!value.is_empty()).then_some(value)
}

pub(super) fn parse_cursor(value: &str) -> std::result::Result<Option<OpaqueCursor>, String> {
    if value.is_empty() {
        Ok(None)
    } else {
        OpaqueCursor::parse(value.to_owned())
            .map(Some)
            .map_err(|_| "Use a page link returned by this knowledge search.".to_owned())
    }
}

pub(super) fn list_href(values: &KnowledgeFilterValues) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in [
        ("q", values.q.as_str()),
        ("tags", values.tags.as_str()),
        ("make", values.make.as_str()),
        ("model", values.model.as_str()),
        ("engine", values.engine.as_str()),
        ("archived", values.archived.as_str()),
        ("cursor", values.cursor.as_str()),
    ] {
        if !value.is_empty() {
            serializer.append_pair(key, value);
        }
    }
    format!("/knowledge?{}", serializer.finish())
}

pub(super) fn prefill_vehicle(values: &mut KnowledgeFormValues, vehicle: &Vehicle) {
    values.vehicle_id = vehicle.id.as_str().to_owned();
    values.make.clone_from(&vehicle.make);
    values.model.clone_from(&vehicle.model);
    values.engine = vehicle.engine_type.clone().unwrap_or_default();
}

pub(super) fn source_conflict() -> String {
    "The selected source intervention and vehicle are inconsistent. Use the source vehicle, remove or change the source, or Reload latest."
        .to_owned()
}

pub(super) fn empty_page() -> Page<crate::models::technical_note::TechnicalNote> {
    Page {
        items: Vec::new(),
        next_cursor: None,
    }
}

pub(super) fn maximum_limit() -> PageLimit {
    PageLimit::new(200).expect("maximum page limit is valid")
}

#[allow(clippy::result_large_err)]
pub(super) fn note_id(
    raw_id: String,
    context: &BrowserRequestContext,
) -> Result<TechnicalNoteId, Response> {
    TechnicalNoteId::parse(raw_id)
        .map_err(|_| responses::not_found(context.response_preference, "technical note"))
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
        "Technical knowledge is temporarily unavailable. Try again shortly.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knowledge_cursor_link_preserves_every_filter() {
        let href = list_href(&KnowledgeFilterValues {
            q: "water pump".to_owned(),
            tags: "cooling\nVolkswagen".to_owned(),
            make: "Volkswagen".to_owned(),
            model: "Golf".to_owned(),
            engine: "1.4 TSI".to_owned(),
            archived: "archived".to_owned(),
            cursor: "opaque_cursor".to_owned(),
        });
        assert!(href.contains("q=water+pump"));
        assert!(href.contains("tags=cooling%0AVolkswagen"));
        assert!(href.contains("cursor=opaque_cursor"));
    }

    #[test]
    fn browser_tags_preserve_normalized_unique_order_and_limits() {
        assert_eq!(
            parse_tags(" Cooling \nVW\nvw\n Brakes ").expect("valid tags"),
            vec!["cooling", "vw", "brakes"]
        );
        let too_many = (0..=TAG_MAX_COUNT)
            .map(|index| format!("tag-{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(parse_tags(&too_many).is_err());
    }
}
