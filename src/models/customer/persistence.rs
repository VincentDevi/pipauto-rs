//! Private SurrealDB customer persistence.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use surrealdb::{
    engine::any::Any,
    types::{RecordId, SurrealValue},
    Surreal,
};

use crate::{
    database::surreal_support as support,
    domain::{CursorTuple, CustomerId, PageLimit},
    models::{
        customer::{Address, Customer, NewCustomer},
        persistence_error::PersistenceError as RepositoryError,
    },
};

use super::repository::{ArchiveFilter, CustomerFilter, CustomerRepository, RepositoryPage};

const PROJECTION: &str =
    "id, display_name, email, phone, address, notes, created_at, updated_at, archived_at";

#[derive(Clone)]
#[doc(hidden)]
pub struct SurrealCustomerRepository {
    client: Surreal<Any>,
}

impl SurrealCustomerRepository {
    #[must_use]
    pub fn new(client: Surreal<Any>) -> Self {
        Self { client }
    }

    async fn set_archive(
        &self,
        id: &CustomerId,
        archived: bool,
    ) -> Result<Customer, RepositoryError> {
        let record = support::record_id("customer", id.as_str())?;
        let predicate = if archived {
            "archived_at IS NONE"
        } else {
            "archived_at IS NOT NONE"
        };
        let value = if archived { "time::now()" } else { "NONE" };
        let query = format!(
            "UPDATE ONLY $record SET archived_at = {value} WHERE {predicate} RETURN AFTER;"
        );
        let mut response =
            support::checked_response(self.client.query(query).bind(("record", record)).await)?;
        let changed: Option<DbCustomer> = support::take(&mut response, 0)?;
        match changed {
            Some(row) => row.try_into(),
            None => self.find_by_id(id).await?.ok_or(RepositoryError::NotFound),
        }
    }
}

#[derive(Clone, Deserialize, SurrealValue)]
struct DbAddress {
    line_1: String,
    line_2: Option<String>,
    postal_code: String,
    city: String,
    country_code: String,
}

impl From<&Address> for DbAddress {
    fn from(value: &Address) -> Self {
        Self {
            line_1: value.line_1.clone(),
            line_2: value.line_2.clone(),
            postal_code: value.postal_code.clone(),
            city: value.city.clone(),
            country_code: value.country_code.clone(),
        }
    }
}

impl From<DbAddress> for Address {
    fn from(value: DbAddress) -> Self {
        Self {
            line_1: value.line_1,
            line_2: value.line_2,
            postal_code: value.postal_code,
            city: value.city,
            country_code: value.country_code,
        }
    }
}

#[derive(Deserialize, SurrealValue)]
struct DbCustomer {
    id: RecordId,
    display_name: String,
    email: Option<String>,
    phone: Option<String>,
    address: Option<DbAddress>,
    notes: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    archived_at: Option<DateTime<Utc>>,
}

impl TryFrom<DbCustomer> for Customer {
    type Error = RepositoryError;

    fn try_from(value: DbCustomer) -> Result<Self, Self::Error> {
        Ok(Self {
            id: CustomerId::parse(support::record_key(&value.id, "customer")?)
                .map_err(|_| RepositoryError::CorruptData)?,
            display_name: value.display_name,
            email: value.email,
            phone: value.phone,
            address: value.address.map(Into::into),
            notes: value.notes,
            created_at: value.created_at,
            updated_at: value.updated_at,
            archived_at: value.archived_at,
        })
    }
}

fn archive_value(filter: ArchiveFilter) -> &'static str {
    match filter {
        ArchiveFilter::Active => "active",
        ArchiveFilter::Archived => "archived",
        ArchiveFilter::All => "all",
    }
}

#[async_trait]
impl CustomerRepository for SurrealCustomerRepository {
    async fn create(&self, customer: &NewCustomer) -> Result<Customer, RepositoryError> {
        let address = customer.address.as_ref().map(DbAddress::from);
        let mut response = support::checked_response(
            self.client
                .query(
                    "CREATE customer SET display_name = $display_name, \
                     display_name_normalized = $display_name_normalized, email = $email, \
                     email_normalized = $email_normalized, phone = $phone, \
                     phone_normalized = $phone_normalized, address = $address, notes = $notes, \
                     archived_at = NONE RETURN AFTER;",
                )
                .bind(("display_name", customer.display_name.clone()))
                .bind((
                    "display_name_normalized",
                    customer.display_name_normalized.clone(),
                ))
                .bind(("email", customer.email.clone()))
                .bind(("email_normalized", customer.email_normalized.clone()))
                .bind(("phone", customer.phone.clone()))
                .bind(("phone_normalized", customer.phone_normalized.clone()))
                .bind(("address", address))
                .bind(("notes", customer.notes.clone()))
                .await,
        )?;
        let row: Option<DbCustomer> = support::take(&mut response, 0)?;
        row.ok_or(RepositoryError::CorruptData)?.try_into()
    }

    async fn find_by_id(&self, id: &CustomerId) -> Result<Option<Customer>, RepositoryError> {
        let query = format!("SELECT {PROJECTION} FROM ONLY $record;");
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind(("record", support::record_id("customer", id.as_str())?))
                .await,
        )?;
        let row: Option<DbCustomer> = support::take(&mut response, 0)?;
        row.map(TryInto::try_into).transpose()
    }

    async fn update(
        &self,
        id: &CustomerId,
        customer: &NewCustomer,
    ) -> Result<Customer, RepositoryError> {
        let address = customer.address.as_ref().map(DbAddress::from);
        let mut response = support::checked_response(
            self.client
                .query(
                    "UPDATE ONLY $record SET display_name = $display_name, \
                     display_name_normalized = $display_name_normalized, email = $email, \
                     email_normalized = $email_normalized, phone = $phone, \
                     phone_normalized = $phone_normalized, address = $address, notes = $notes \
                     RETURN AFTER;",
                )
                .bind(("record", support::record_id("customer", id.as_str())?))
                .bind(("display_name", customer.display_name.clone()))
                .bind((
                    "display_name_normalized",
                    customer.display_name_normalized.clone(),
                ))
                .bind(("email", customer.email.clone()))
                .bind(("email_normalized", customer.email_normalized.clone()))
                .bind(("phone", customer.phone.clone()))
                .bind(("phone_normalized", customer.phone_normalized.clone()))
                .bind(("address", address))
                .bind(("notes", customer.notes.clone()))
                .await,
        )?;
        let row: Option<DbCustomer> = support::take(&mut response, 0)?;
        row.ok_or(RepositoryError::NotFound)?.try_into()
    }

    async fn list(
        &self,
        filter: &CustomerFilter,
        limit: PageLimit,
        after: Option<&CursorTuple>,
    ) -> Result<RepositoryPage<Customer>, RepositoryError> {
        let (after_time, after_id) = after
            .map(|cursor| support::surreal_cursor_tuple(cursor, "customer"))
            .transpose()?
            .map_or((None, None), |(time, id)| (Some(time), Some(id)));
        let query = format!(
            "SELECT {PROJECTION} FROM customer WHERE \
             ($archive = 'all' OR ($archive = 'active' AND archived_at IS NONE) OR \
             ($archive = 'archived' AND archived_at IS NOT NONE)) AND \
             ($query IS NONE OR string::contains(display_name_normalized, $query) OR \
             string::contains(email_normalized ?? '', $query) OR \
             string::contains(phone_normalized ?? '', $query)) AND \
             ($after_time IS NONE OR created_at < $after_time OR \
             (created_at = $after_time AND id < $after_id)) \
             ORDER BY created_at DESC, id DESC LIMIT $fetch_limit;"
        );
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind(("archive", archive_value(filter.archive).to_owned()))
                .bind(("query", filter.query.clone()))
                .bind(("after_time", after_time))
                .bind(("after_id", after_id))
                .bind(("fetch_limit", i64::from(limit.value()) + 1))
                .await,
        )?;
        let mut rows: Vec<DbCustomer> = support::take(&mut response, 0)?;
        let has_more = rows.len() > usize::from(limit.value());
        if has_more {
            rows.pop();
        }
        let next = if has_more {
            rows.last()
                .map(|row| support::cursor_tuple(row.created_at, &row.id, "customer"))
                .transpose()?
        } else {
            None
        };
        let items = rows
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RepositoryPage { items, next })
    }

    async fn archive(&self, id: &CustomerId) -> Result<Customer, RepositoryError> {
        self.set_archive(id, true).await
    }

    async fn restore(&self, id: &CustomerId) -> Result<Customer, RepositoryError> {
        self.set_archive(id, false).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customer_repository_uses_explicit_projection_and_bound_filters() {
        assert!(!PROJECTION.contains('*'));
        assert_eq!(archive_value(ArchiveFilter::Active), "active");
    }
}
