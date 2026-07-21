//! Authenticated intervention, line-item, transition, and service-history JSON routes.

use axum::{extract::DefaultBodyLimit, http::StatusCode, Json};
use loco_rs::{
    controller::{extractor::shared_store::SharedStore, Routes},
    prelude::{delete, get, patch, post, Path, Query},
};
use serde::{Deserialize, Serialize};

use crate::{
    api::{
        ids::{InterventionIdDto, InterventionLineIdDto, VehicleIdDto},
        DataEnvelope, MoneyDto, PaginationEnvelope, QuantityDto, TimestampDto,
    },
    auth::{csrf::AuthenticatedCsrfJson, extractors::CurrentUser},
    domain::{
        CurrencyCode, InterventionId, InterventionLineId, PageRequest, Quantity, ValidationCode,
        ValidationError, ValidationErrors, VehicleId,
    },
    errors::AppError,
    models::{
        intervention::{
            Intervention, InterventionStatus, InterventionTotals, ServiceHistorySummary,
        },
        intervention_line::{InterventionLine, InterventionLineCategory},
    },
    repositories::intervention::{InterventionFilter, LineMutationResult},
    services::intervention::{
        CreateIntervention, InterventionService, UpdateIntervention, WriteLine,
    },
    settings::BusinessSettings,
};

const BODY_LIMIT: usize = 64 * 1024;
type OptionalUtcBounds = (
    Option<chrono::DateTime<chrono::Utc>>,
    Option<chrono::DateTime<chrono::Utc>>,
);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InterventionQuery {
    limit: Option<u16>,
    cursor: Option<String>,
    vehicle_id: Option<String>,
    status: Option<String>,
    service_date_from: Option<chrono::NaiveDate>,
    service_date_to: Option<chrono::NaiveDate>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateInterventionRequest {
    vehicle_id: String,
    service_date: String,
    estimated_duration_minutes: u16,
    mileage: Option<u64>,
    customer_reported_problem: Option<String>,
    diagnostics: Option<String>,
    performed_work: Option<String>,
    recommendations: Option<String>,
    notes: Option<String>,
    currency: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateInterventionRequest {
    service_date: Option<String>,
    estimated_duration_minutes: Option<u16>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    mileage: Option<Option<u64>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    customer_reported_problem: Option<Option<String>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    diagnostics: Option<Option<String>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    performed_work: Option<Option<String>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    recommendations: Option<Option<String>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    notes: Option<Option<String>>,
    currency: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteLineRequest {
    category: String,
    description: String,
    quantity: String,
    unit_label: String,
    unit_price_minor: i64,
    unit_cost_minor: Option<i64>,
    position: u32,
}

#[derive(Serialize)]
struct InterventionDto {
    id: InterventionIdDto,
    vehicle_id: VehicleIdDto,
    service_date: String,
    estimated_duration_minutes: u16,
    customer_snapshot: CustomerSnapshotDto,
    vehicle_snapshot: VehicleSnapshotDto,
    status: &'static str,
    mileage: Option<u64>,
    customer_reported_problem: Option<String>,
    diagnostics: Option<String>,
    performed_work: Option<String>,
    recommendations: Option<String>,
    notes: Option<String>,
    currency: String,
    created_at: TimestampDto,
    updated_at: TimestampDto,
    completed_at: Option<TimestampDto>,
    cancelled_at: Option<TimestampDto>,
    links: InterventionLinksDto,
}

#[derive(Serialize)]
struct CustomerSnapshotDto {
    id: String,
    display_name: String,
}

#[derive(Serialize)]
struct VehicleSnapshotDto {
    registration: Option<String>,
    make: String,
    model: String,
}

#[derive(Serialize)]
struct InterventionLinksDto {
    detail: String,
    lines: String,
}

impl From<Intervention> for InterventionDto {
    fn from(value: Intervention) -> Self {
        let key = value.id.as_str();
        Self {
            id: InterventionIdDto::from(&value.id),
            vehicle_id: VehicleIdDto::from(&value.vehicle_id),
            service_date: value
                .service_date
                .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
            estimated_duration_minutes: value.estimated_duration.minutes(),
            customer_snapshot: CustomerSnapshotDto {
                id: value.identity_snapshot.customer_id.as_str().to_owned(),
                display_name: value.identity_snapshot.customer_name.clone(),
            },
            vehicle_snapshot: VehicleSnapshotDto {
                registration: value.identity_snapshot.vehicle_registration.clone(),
                make: value.identity_snapshot.vehicle_make.clone(),
                model: value.identity_snapshot.vehicle_model.clone(),
            },
            status: status_value(value.status),
            mileage: value.mileage,
            customer_reported_problem: value.customer_reported_problem,
            diagnostics: value.diagnostics,
            performed_work: value.performed_work,
            recommendations: value.recommendations,
            notes: value.notes,
            currency: value.currency.as_str().to_owned(),
            created_at: value.created_at.into(),
            updated_at: value.updated_at.into(),
            completed_at: value.completed_at.map(Into::into),
            cancelled_at: value.cancelled_at.map(Into::into),
            links: InterventionLinksDto {
                detail: format!("/api/v1/interventions/{key}"),
                lines: format!("/api/v1/interventions/{key}/lines"),
            },
        }
    }
}

#[derive(Serialize)]
struct HistoryDto {
    intervention: InterventionDto,
    totals: TotalsDto,
}

impl From<ServiceHistorySummary> for HistoryDto {
    fn from(value: ServiceHistorySummary) -> Self {
        Self {
            intervention: value.intervention.into(),
            totals: value.totals.into(),
        }
    }
}

#[derive(Serialize)]
struct TotalsDto {
    price: MoneyDto,
    cost: MoneyDto,
}

impl From<InterventionTotals> for TotalsDto {
    fn from(value: InterventionTotals) -> Self {
        Self {
            price: value.price.into(),
            cost: value.cost.into(),
        }
    }
}

#[derive(Serialize)]
struct InterventionLineDto {
    id: InterventionLineIdDto,
    intervention_id: InterventionIdDto,
    category: &'static str,
    description: String,
    quantity: QuantityDto,
    unit_label: String,
    unit_price: MoneyDto,
    unit_cost: Option<MoneyDto>,
    total_price: MoneyDto,
    total_cost: Option<MoneyDto>,
    position: u32,
    created_at: TimestampDto,
    updated_at: TimestampDto,
}

impl From<InterventionLine> for InterventionLineDto {
    fn from(value: InterventionLine) -> Self {
        Self {
            id: InterventionLineIdDto::from(&value.id),
            intervention_id: InterventionIdDto::from(&value.intervention_id),
            category: category_value(value.category),
            description: value.description,
            quantity: value.quantity.into(),
            unit_label: value.unit_label,
            unit_price: value.unit_price.into(),
            unit_cost: value.unit_cost.map(Into::into),
            total_price: value.total_price.into(),
            total_cost: value.total_cost.map(Into::into),
            position: value.position,
            created_at: value.created_at.into(),
            updated_at: value.updated_at.into(),
        }
    }
}

#[derive(Serialize)]
struct LineMutationDto {
    line: Option<InterventionLineDto>,
    totals: TotalsDto,
}

impl From<LineMutationResult> for LineMutationDto {
    fn from(value: LineMutationResult) -> Self {
        Self {
            line: value.line.map(Into::into),
            totals: value.totals.into(),
        }
    }
}

async fn list(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InterventionService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    Query(query): Query<InterventionQuery>,
) -> Result<Json<PaginationEnvelope<HistoryDto>>, AppError> {
    let request = resolve_query(query, &settings)?;
    Ok(Json(service.list(request).await?.into()))
}

async fn create(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InterventionService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<CreateInterventionRequest>,
) -> Result<(StatusCode, Json<DataEnvelope<InterventionDto>>), AppError> {
    let command = create_command(request, &settings)?;
    let value = service.create(command).await?;
    Ok((StatusCode::CREATED, Json(DataEnvelope::new(value.into()))))
}

async fn show(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InterventionService>,
    Path(id): Path<String>,
) -> Result<Json<DataEnvelope<InterventionDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service.get(&parse_intervention_id(id)?).await?.into(),
    )))
}

async fn update(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InterventionService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<UpdateInterventionRequest>,
) -> Result<Json<DataEnvelope<InterventionDto>>, AppError> {
    let command = UpdateIntervention {
        service_date: request
            .service_date
            .map(|value| resolve_local_service_date(&value, &settings))
            .transpose()?,
        estimated_duration_minutes: request.estimated_duration_minutes,
        mileage: request.mileage,
        customer_reported_problem: request.customer_reported_problem,
        diagnostics: request.diagnostics,
        performed_work: request.performed_work,
        recommendations: request.recommendations,
        notes: request.notes,
        currency: request.currency.map(parse_currency).transpose()?,
    };
    Ok(Json(DataEnvelope::new(
        service
            .update(&parse_intervention_id(id)?, command)
            .await?
            .into(),
    )))
}

async fn complete(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InterventionService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(()): AuthenticatedCsrfJson<()>,
) -> Result<Json<DataEnvelope<InterventionDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service.complete(&parse_intervention_id(id)?).await?.into(),
    )))
}

async fn cancel(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InterventionService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(()): AuthenticatedCsrfJson<()>,
) -> Result<Json<DataEnvelope<InterventionDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service.cancel(&parse_intervention_id(id)?).await?.into(),
    )))
}

async fn history(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InterventionService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    Path(vehicle_id): Path<String>,
    Query(mut query): Query<InterventionQuery>,
) -> Result<Json<PaginationEnvelope<HistoryDto>>, AppError> {
    let vehicle_id = parse_vehicle_id(vehicle_id)?;
    query.vehicle_id = Some(vehicle_id.as_str().to_owned());
    let request = resolve_query(query, &settings)?;
    Ok(Json(
        service.service_history(&vehicle_id, request).await?.into(),
    ))
}

async fn lines(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InterventionService>,
    Path(id): Path<String>,
) -> Result<Json<DataEnvelope<Vec<InterventionLineDto>>>, AppError> {
    let values = service.list_lines(&parse_intervention_id(id)?).await?;
    Ok(Json(DataEnvelope::new(
        values.into_iter().map(Into::into).collect(),
    )))
}

async fn create_line(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InterventionService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<WriteLineRequest>,
) -> Result<(StatusCode, Json<DataEnvelope<LineMutationDto>>), AppError> {
    let value = service
        .create_line(&parse_intervention_id(id)?, line_command(request)?)
        .await?;
    Ok((StatusCode::CREATED, Json(DataEnvelope::new(value.into()))))
}

async fn update_line(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InterventionService>,
    Path((id, line_id)): Path<(String, String)>,
    AuthenticatedCsrfJson(request): AuthenticatedCsrfJson<WriteLineRequest>,
) -> Result<Json<DataEnvelope<LineMutationDto>>, AppError> {
    let value = service
        .update_line(
            &parse_intervention_id(id)?,
            parse_line_id(line_id)?,
            line_command(request)?,
        )
        .await?;
    Ok(Json(DataEnvelope::new(value.into())))
}

async fn delete_line(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<InterventionService>,
    Path((id, line_id)): Path<(String, String)>,
    AuthenticatedCsrfJson(()): AuthenticatedCsrfJson<()>,
) -> Result<Json<DataEnvelope<LineMutationDto>>, AppError> {
    let value = service
        .delete_line(&parse_intervention_id(id)?, parse_line_id(line_id)?)
        .await?;
    Ok(Json(DataEnvelope::new(value.into())))
}

fn resolve_query(
    query: InterventionQuery,
    settings: &BusinessSettings,
) -> Result<PageRequest<InterventionFilter>, AppError> {
    let pagination = crate::api::PaginationQuery {
        limit: query.limit,
        cursor: query.cursor,
    }
    .resolve(settings)
    .map_err(AppError::Validation)?;
    let vehicle_id = query
        .vehicle_id
        .map(VehicleId::parse)
        .transpose()
        .map_err(|_| invalid("vehicle_id", "Use a valid vehicle identifier."))?;
    let status = query.status.map(|value| parse_status(&value)).transpose()?;
    if query
        .service_date_from
        .zip(query.service_date_to)
        .is_some_and(|(from, to)| from > to)
    {
        return Err(invalid(
            "service_date_to",
            "The end date must not precede the start date.",
        ));
    }
    let (service_date_from, service_date_until) =
        resolve_date_bounds(query.service_date_from, query.service_date_to, settings)?;
    Ok(PageRequest {
        filter: InterventionFilter {
            vehicle_id,
            status,
            service_date_from,
            service_date_until,
        },
        limit: pagination.limit,
        after: pagination.after,
    })
}

fn create_command(
    request: CreateInterventionRequest,
    settings: &BusinessSettings,
) -> Result<CreateIntervention, AppError> {
    Ok(CreateIntervention {
        vehicle_id: parse_vehicle_id(request.vehicle_id)?,
        service_date: resolve_local_service_date(&request.service_date, settings)?,
        estimated_duration_minutes: request.estimated_duration_minutes,
        mileage: request.mileage,
        customer_reported_problem: request.customer_reported_problem,
        diagnostics: request.diagnostics,
        performed_work: request.performed_work,
        recommendations: request.recommendations,
        notes: request.notes,
        currency: request
            .currency
            .map_or_else(|| parse_currency("EUR".into()), parse_currency)?,
    })
}

fn resolve_local_service_date(
    value: &str,
    settings: &BusinessSettings,
) -> Result<chrono::DateTime<chrono::Utc>, AppError> {
    crate::domain::WorkshopTime::system(settings.workshop_timezone())
        .local_to_utc(value)
        .map_err(|error| invalid("service_date", &error.to_string()))
}

fn resolve_date_bounds(
    from: Option<chrono::NaiveDate>,
    to: Option<chrono::NaiveDate>,
    settings: &BusinessSettings,
) -> Result<OptionalUtcBounds, AppError> {
    use chrono::Days;

    let workshop_time = crate::domain::WorkshopTime::system(settings.workshop_timezone());
    let from = from
        .map(|date| workshop_time.local_to_utc(&format!("{date}T00:00")))
        .transpose()
        .map_err(|error| invalid("service_date_from", &error.to_string()))?;
    let until = to
        .map(|date| {
            date.checked_add_days(Days::new(1))
                .ok_or(crate::domain::WorkshopTimeError::CalendarBoundaryOutOfRange)
                .and_then(|date| workshop_time.local_to_utc(&format!("{date}T00:00")))
        })
        .transpose()
        .map_err(|error| invalid("service_date_to", &error.to_string()))?;
    Ok((from, until))
}

fn line_command(request: WriteLineRequest) -> Result<WriteLine, AppError> {
    Ok(WriteLine {
        category: parse_category(&request.category)?,
        description: request.description,
        quantity: Quantity::parse(&request.quantity).map_err(|_| {
            invalid(
                "quantity",
                "Enter a positive quantity with up to three decimals.",
            )
        })?,
        unit_label: request.unit_label,
        unit_price_minor: request.unit_price_minor,
        unit_cost_minor: request.unit_cost_minor,
        position: request.position,
    })
}

fn parse_status(value: &str) -> Result<InterventionStatus, AppError> {
    match value {
        "draft" => Ok(InterventionStatus::Draft),
        "completed" => Ok(InterventionStatus::Completed),
        "cancelled" => Ok(InterventionStatus::Cancelled),
        _ => Err(invalid("status", "Use draft, completed, or cancelled.")),
    }
}

fn parse_category(value: &str) -> Result<InterventionLineCategory, AppError> {
    match value {
        "labour" => Ok(InterventionLineCategory::Labour),
        "part" => Ok(InterventionLineCategory::Part),
        "material" => Ok(InterventionLineCategory::Material),
        "other" => Ok(InterventionLineCategory::Other),
        _ => Err(invalid("category", "Use labour, part, material, or other.")),
    }
}

fn status_value(value: InterventionStatus) -> &'static str {
    match value {
        InterventionStatus::Draft => "draft",
        InterventionStatus::Completed => "completed",
        InterventionStatus::Cancelled => "cancelled",
    }
}

fn category_value(value: InterventionLineCategory) -> &'static str {
    match value {
        InterventionLineCategory::Labour => "labour",
        InterventionLineCategory::Part => "part",
        InterventionLineCategory::Material => "material",
        InterventionLineCategory::Other => "other",
    }
}

fn parse_currency(value: String) -> Result<CurrencyCode, AppError> {
    CurrencyCode::parse(&value)
        .map_err(|_| invalid("currency", "Use an assigned uppercase currency code."))
}

fn parse_intervention_id(value: String) -> Result<InterventionId, AppError> {
    InterventionId::parse(value).map_err(|_| invalid("id", "Use a valid intervention identifier."))
}

fn parse_line_id(value: String) -> Result<InterventionLineId, AppError> {
    InterventionLineId::parse(value)
        .map_err(|_| invalid("line_id", "Use a valid intervention-line identifier."))
}

fn parse_vehicle_id(value: String) -> Result<VehicleId, AppError> {
    VehicleId::parse(value).map_err(|_| invalid("vehicle_id", "Use a valid vehicle identifier."))
}

fn invalid(field: &str, message: &str) -> AppError {
    AppError::Validation(ValidationErrors::one(
        ValidationError::new(field, ValidationCode::InvalidFormat, message)
            .expect("static validation metadata is valid"),
    ))
}

#[must_use]
pub fn routes() -> Routes {
    Routes::new()
        .add("/interventions", get(list))
        .add(
            "/interventions",
            post(create).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add("/interventions/{id}", get(show))
        .add(
            "/interventions/{id}",
            patch(update).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add(
            "/interventions/{id}/complete",
            post(complete).layer(DefaultBodyLimit::max(64)),
        )
        .add(
            "/interventions/{id}/cancel",
            post(cancel).layer(DefaultBodyLimit::max(64)),
        )
        .add("/vehicles/{id}/service-history", get(history))
        .add("/interventions/{id}/lines", get(lines))
        .add(
            "/interventions/{id}/lines",
            post(create_line).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add(
            "/interventions/{id}/lines/{line_id}",
            patch(update_line).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add(
            "/interventions/{id}/lines/{line_id}",
            delete(delete_line).layer(DefaultBodyLimit::max(64)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interventions_api_keeps_cancelled_as_an_explicit_status_filter() {
        assert_eq!(
            parse_status("cancelled").expect("supported"),
            InterventionStatus::Cancelled
        );
    }
}
