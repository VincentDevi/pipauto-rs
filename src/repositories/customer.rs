//! Persistence-neutral customer repository contract.

use async_trait::async_trait;

use crate::{
    domain::{CollectionFilter, CursorTuple, CustomerId, PageLimit},
    models::customer::{Customer, NewCustomer},
};

use super::RepositoryError;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ArchiveFilter {
    #[default]
    Active,
    Archived,
    All,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CustomerFilter {
    pub query: Option<String>,
    pub archive: ArchiveFilter,
}

impl CollectionFilter for CustomerFilter {
    fn fingerprint_bytes(&self) -> Vec<u8> {
        format!(
            "customers:v1:{:?}:{}",
            self.archive,
            self.query.as_deref().unwrap_or("")
        )
        .into_bytes()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryPage<T> {
    pub items: Vec<T>,
    pub next: Option<CursorTuple>,
}

#[async_trait]
pub trait CustomerRepository: Send + Sync {
    async fn create(&self, customer: &NewCustomer) -> Result<Customer, RepositoryError>;
    async fn find_by_id(&self, id: &CustomerId) -> Result<Option<Customer>, RepositoryError>;
    async fn update(
        &self,
        id: &CustomerId,
        customer: &NewCustomer,
    ) -> Result<Customer, RepositoryError>;
    async fn list(
        &self,
        filter: &CustomerFilter,
        limit: PageLimit,
        after: Option<&CursorTuple>,
    ) -> Result<RepositoryPage<Customer>, RepositoryError>;
    async fn archive(&self, id: &CustomerId) -> Result<Customer, RepositoryError>;
    async fn restore(&self, id: &CustomerId) -> Result<Customer, RepositoryError>;
}
