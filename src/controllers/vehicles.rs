//! Authenticated vehicle JSON routes.

use axum::{extract::DefaultBodyLimit, Json};
use loco_rs::{
    controller::{extractor::shared_store::SharedStore, Routes},
    prelude::{get, patch, post, Path, Query},
};
use serde::{Deserialize, Serialize};

use crate::{
    api::{
        ids::{CustomerIdDto, VehicleIdDto},
        DataEnvelope, PaginationEnvelope, TimestampDto,
    },
    auth::{csrf::AuthenticatedCsrfJson, extractors::CurrentUser},
    domain::{
        normalize_search_text, CustomerId, NormalizedRegistration, NormalizedVin, OpaqueCursor,
        PageLimit, PageRequest, ValidationCode, ValidationError, ValidationErrors, VehicleId,
    },
    errors::AppError,
    models::vehicle::Vehicle,
    repositories::{customer::ArchiveFilter, vehicle::VehicleFilter},
    services::vehicle::{CreateVehicle, UpdateVehicle, VehicleService},
    settings::BusinessSettings,
};

const BODY_LIMIT: usize = 24 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VehicleQuery {
    limit: Option<u16>,
    cursor: Option<String>,
    q: Option<String>,
    archived: Option<String>,
    customer_id: Option<String>,
    registration: Option<String>,
    vin: Option<String>,
    make: Option<String>,
    model: Option<String>,
}

impl VehicleQuery {
    pub(crate) const fn has_customer_filter(&self) -> bool {
        self.customer_id.is_some()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateVehicleRequest {
    customer_id: String,
    make: String,
    model: String,
    year: Option<i32>,
    registration: Option<String>,
    vin: Option<String>,
    current_mileage: Option<u64>,
    engine_type: Option<String>,
    notes: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateVehicleRequest {
    customer_id: Option<String>,
    make: Option<String>,
    model: Option<String>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    year: Option<Option<i32>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    registration: Option<Option<String>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    vin: Option<Option<String>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    current_mileage: Option<Option<u64>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    engine_type: Option<Option<String>>,
    #[serde(default, deserialize_with = "super::customers::present_option")]
    notes: Option<Option<String>>,
}

#[derive(Serialize)]
pub(crate) struct VehicleDto {
    id: VehicleIdDto,
    customer_id: CustomerIdDto,
    make: String,
    model: String,
    year: Option<i32>,
    registration: Option<String>,
    vin: Option<String>,
    current_mileage: Option<u64>,
    engine_type: Option<String>,
    notes: Option<String>,
    created_at: TimestampDto,
    updated_at: TimestampDto,
    archived_at: Option<TimestampDto>,
}
impl From<Vehicle> for VehicleDto {
    fn from(v: Vehicle) -> Self {
        Self {
            id: VehicleIdDto::from(&v.id),
            customer_id: CustomerIdDto::from(&v.customer_id),
            make: v.make,
            model: v.model,
            year: v.year,
            registration: v.registration,
            vin: v.vin,
            current_mileage: v.current_mileage,
            engine_type: v.engine_type,
            notes: v.notes,
            created_at: v.created_at.into(),
            updated_at: v.updated_at.into(),
            archived_at: v.archived_at.map(Into::into),
        }
    }
}

pub(crate) fn resolve_query(
    q: VehicleQuery,
    settings: &BusinessSettings,
) -> Result<(VehicleFilter, PageLimit, Option<OpaqueCursor>), AppError> {
    let pagination = crate::api::PaginationQuery {
        limit: q.limit,
        cursor: q.cursor,
    }
    .resolve(settings)
    .map_err(AppError::Validation)?;
    let archive = match q.archived.as_deref().unwrap_or("active") {
        "active" => ArchiveFilter::Active,
        "archived" => ArchiveFilter::Archived,
        "all" => ArchiveFilter::All,
        _ => return Err(invalid("archived", "Use active, archived, or all.")),
    };
    let customer_id = q
        .customer_id
        .map(CustomerId::parse)
        .transpose()
        .map_err(|_| invalid("customer_id", "Use a valid customer identifier."))?;
    let registration = q
        .registration
        .as_deref()
        .map(NormalizedRegistration::parse)
        .transpose()
        .map_err(|_| invalid("registration", "Enter a valid registration."))?;
    let vin = q
        .vin
        .as_deref()
        .map(NormalizedVin::parse)
        .transpose()
        .map_err(|_| invalid("vin", "Enter a valid VIN."))?;
    Ok((
        VehicleFilter {
            query: q.q,
            archive,
            customer_id,
            registration,
            vin,
            make: q.make.map(|v| normalize_search_text(&v)),
            model: q.model.map(|v| normalize_search_text(&v)),
        },
        pagination.limit,
        pagination.after,
    ))
}

async fn list(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<VehicleService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    Query(q): Query<VehicleQuery>,
) -> Result<Json<PaginationEnvelope<VehicleDto>>, AppError> {
    let (filter, limit, after) = resolve_query(q, &settings)?;
    Ok(Json(
        service
            .list(PageRequest {
                filter,
                limit,
                after,
            })
            .await?
            .into(),
    ))
}
async fn create(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<VehicleService>,
    AuthenticatedCsrfJson(r): AuthenticatedCsrfJson<CreateVehicleRequest>,
) -> Result<Json<DataEnvelope<VehicleDto>>, AppError> {
    let customer_id = CustomerId::parse(r.customer_id)
        .map_err(|_| invalid("customer_id", "Use a valid customer identifier."))?;
    let value = service
        .create(CreateVehicle {
            customer_id,
            make: r.make,
            model: r.model,
            year: r.year,
            registration: r.registration,
            vin: r.vin,
            current_mileage: r.current_mileage,
            engine_type: r.engine_type,
            notes: r.notes,
        })
        .await?;
    Ok(Json(DataEnvelope::new(value.into())))
}
async fn show(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<VehicleService>,
    Path(id): Path<String>,
) -> Result<Json<DataEnvelope<VehicleDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service.get(&parse_id(id)?).await?.into(),
    )))
}
async fn update(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<VehicleService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(r): AuthenticatedCsrfJson<UpdateVehicleRequest>,
) -> Result<Json<DataEnvelope<VehicleDto>>, AppError> {
    let customer_id = r
        .customer_id
        .map(CustomerId::parse)
        .transpose()
        .map_err(|_| invalid("customer_id", "Use a valid customer identifier."))?;
    let value = service
        .update(
            &parse_id(id)?,
            UpdateVehicle {
                customer_id,
                make: r.make,
                model: r.model,
                year: r.year,
                registration: r.registration,
                vin: r.vin,
                current_mileage: r.current_mileage,
                engine_type: r.engine_type,
                notes: r.notes,
            },
        )
        .await?;
    Ok(Json(DataEnvelope::new(value.into())))
}
async fn archive(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<VehicleService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(()): AuthenticatedCsrfJson<()>,
) -> Result<Json<DataEnvelope<VehicleDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service.archive(&parse_id(id)?).await?.into(),
    )))
}
async fn restore(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<VehicleService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(()): AuthenticatedCsrfJson<()>,
) -> Result<Json<DataEnvelope<VehicleDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service.restore(&parse_id(id)?).await?.into(),
    )))
}
fn parse_id(v: String) -> Result<VehicleId, AppError> {
    VehicleId::parse(v).map_err(|_| invalid("id", "Use a valid vehicle identifier."))
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
        .add("/vehicles", get(list))
        .add(
            "/vehicles",
            post(create).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add("/vehicles/{id}", get(show))
        .add(
            "/vehicles/{id}",
            patch(update).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add(
            "/vehicles/{id}/archive",
            post(archive).layer(DefaultBodyLimit::max(64)),
        )
        .add(
            "/vehicles/{id}/restore",
            post(restore).layer(DefaultBodyLimit::max(64)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vehicles_api_normalizes_exact_filters() {
        assert_eq!(
            NormalizedRegistration::parse(" 1-abc-234 ")
                .expect("valid")
                .as_str(),
            "1ABC234"
        );
    }
}
