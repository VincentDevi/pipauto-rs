//! Authenticated customer JSON routes.

use axum::{extract::DefaultBodyLimit, Json};
use loco_rs::{
    controller::{extractor::shared_store::SharedStore, Routes},
    prelude::{get, patch, post, Path, Query},
};
use serde::{Deserialize, Serialize};

use crate::{
    api::{ids::CustomerIdDto, DataEnvelope, PaginationEnvelope, TimestampDto},
    auth::{csrf::AuthenticatedCsrfJson, extractors::CurrentUser},
    domain::{CustomerId, PageRequest, ValidationCode, ValidationError, ValidationErrors},
    errors::AppError,
    models::customer::{Address, Customer},
    repositories::customer::{ArchiveFilter, CustomerFilter},
    services::{
        customer::{CreateCustomer, CustomerAddressInput, CustomerService, UpdateCustomer},
        vehicle::VehicleService,
    },
    settings::BusinessSettings,
};

const BODY_LIMIT: usize = 24 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CustomerQuery {
    limit: Option<u16>,
    cursor: Option<String>,
    q: Option<String>,
    archived: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddressRequest {
    line_1: String,
    line_2: Option<String>,
    postal_code: String,
    city: String,
    country_code: String,
}

impl From<AddressRequest> for CustomerAddressInput {
    fn from(v: AddressRequest) -> Self {
        Self {
            line_1: v.line_1,
            line_2: v.line_2,
            postal_code: v.postal_code,
            city: v.city,
            country_code: v.country_code,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateCustomerRequest {
    display_name: String,
    email: Option<String>,
    phone: Option<String>,
    address: Option<AddressRequest>,
    notes: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateCustomerRequest {
    display_name: Option<String>,
    #[serde(default, deserialize_with = "present_option")]
    email: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_option")]
    phone: Option<Option<String>>,
    #[serde(default, deserialize_with = "present_option")]
    address: Option<Option<AddressRequest>>,
    #[serde(default, deserialize_with = "present_option")]
    notes: Option<Option<String>>,
}

pub(crate) fn present_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Serialize)]
struct AddressDto {
    line_1: String,
    line_2: Option<String>,
    postal_code: String,
    city: String,
    country_code: String,
}
impl From<Address> for AddressDto {
    fn from(v: Address) -> Self {
        Self {
            line_1: v.line_1,
            line_2: v.line_2,
            postal_code: v.postal_code,
            city: v.city,
            country_code: v.country_code,
        }
    }
}

#[derive(Serialize)]
struct CustomerDto {
    id: CustomerIdDto,
    display_name: String,
    email: Option<String>,
    phone: Option<String>,
    address: Option<AddressDto>,
    notes: Option<String>,
    created_at: TimestampDto,
    updated_at: TimestampDto,
    archived_at: Option<TimestampDto>,
}
impl From<Customer> for CustomerDto {
    fn from(v: Customer) -> Self {
        Self {
            id: CustomerIdDto::from(&v.id),
            display_name: v.display_name,
            email: v.email,
            phone: v.phone,
            address: v.address.map(Into::into),
            notes: v.notes,
            created_at: v.created_at.into(),
            updated_at: v.updated_at.into(),
            archived_at: v.archived_at.map(Into::into),
        }
    }
}

async fn list(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<CustomerService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    Query(q): Query<CustomerQuery>,
) -> Result<Json<PaginationEnvelope<CustomerDto>>, AppError> {
    let pagination = crate::api::PaginationQuery {
        limit: q.limit,
        cursor: q.cursor,
    }
    .resolve(&settings)
    .map_err(AppError::Validation)?;
    let filter = CustomerFilter {
        query: q.q,
        archive: parse_archive(q.archived)?,
    };
    Ok(Json(
        service
            .list(PageRequest {
                filter,
                limit: pagination.limit,
                after: pagination.after,
            })
            .await?
            .into(),
    ))
}

async fn create(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<CustomerService>,
    AuthenticatedCsrfJson(r): AuthenticatedCsrfJson<CreateCustomerRequest>,
) -> Result<Json<DataEnvelope<CustomerDto>>, AppError> {
    let value = service
        .create(CreateCustomer {
            display_name: r.display_name,
            email: r.email,
            phone: r.phone,
            address: r.address.map(Into::into),
            notes: r.notes,
        })
        .await?;
    Ok(Json(DataEnvelope::new(value.into())))
}

async fn show(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<CustomerService>,
    Path(id): Path<String>,
) -> Result<Json<DataEnvelope<CustomerDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service.get(&parse_id(id)?).await?.into(),
    )))
}

async fn update(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<CustomerService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(r): AuthenticatedCsrfJson<UpdateCustomerRequest>,
) -> Result<Json<DataEnvelope<CustomerDto>>, AppError> {
    let value = service
        .update(
            &parse_id(id)?,
            UpdateCustomer {
                display_name: r.display_name,
                email: r.email,
                phone: r.phone,
                address: r.address.map(|v| v.map(Into::into)),
                notes: r.notes,
            },
        )
        .await?;
    Ok(Json(DataEnvelope::new(value.into())))
}

async fn archive(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<CustomerService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(()): AuthenticatedCsrfJson<()>,
) -> Result<Json<DataEnvelope<CustomerDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service.archive(&parse_id(id)?).await?.into(),
    )))
}
async fn restore(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<CustomerService>,
    Path(id): Path<String>,
    AuthenticatedCsrfJson(()): AuthenticatedCsrfJson<()>,
) -> Result<Json<DataEnvelope<CustomerDto>>, AppError> {
    Ok(Json(DataEnvelope::new(
        service.restore(&parse_id(id)?).await?.into(),
    )))
}

async fn vehicles(
    CurrentUser(_): CurrentUser,
    SharedStore(service): SharedStore<VehicleService>,
    SharedStore(settings): SharedStore<BusinessSettings>,
    Path(id): Path<String>,
    Query(query): Query<super::vehicles::VehicleQuery>,
) -> Result<Json<PaginationEnvelope<super::vehicles::VehicleDto>>, AppError> {
    if query.has_customer_filter() {
        return Err(invalid(
            "customer_id",
            "Do not combine customer_id with a customer-scoped vehicle route.",
        ));
    }
    let (filter, limit, after) = super::vehicles::resolve_query(query, &settings)?;
    Ok(Json(
        service
            .list_by_customer(
                &parse_id(id)?,
                PageRequest {
                    filter,
                    limit,
                    after,
                },
            )
            .await?
            .into(),
    ))
}

fn parse_id(v: String) -> Result<CustomerId, AppError> {
    CustomerId::parse(v).map_err(|_| invalid("id", "Use a valid customer identifier."))
}
fn parse_archive(v: Option<String>) -> Result<ArchiveFilter, AppError> {
    match v.as_deref().unwrap_or("active") {
        "active" => Ok(ArchiveFilter::Active),
        "archived" => Ok(ArchiveFilter::Archived),
        "all" => Ok(ArchiveFilter::All),
        _ => Err(invalid("archived", "Use active, archived, or all.")),
    }
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
        .add("/customers", get(list))
        .add(
            "/customers",
            post(create).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add("/customers/{id}", get(show))
        .add(
            "/customers/{id}",
            patch(update).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .add(
            "/customers/{id}/archive",
            post(archive).layer(DefaultBodyLimit::max(64)),
        )
        .add(
            "/customers/{id}/restore",
            post(restore).layer(DefaultBodyLimit::max(64)),
        )
        .add("/customers/{id}/vehicles", get(vehicles))
}

#[cfg(test)]
mod tests {
    #[test]
    fn customers_api_rejects_unknown_archive_values() {
        assert!(super::parse_archive(Some("deleted".into())).is_err());
    }
}
