//! Persistence-neutral technical-note repository contract.

use async_trait::async_trait;

use crate::{
    domain::{CollectionFilter, CursorTuple, PageLimit, TechnicalNoteId},
    models::technical_note::{NewTechnicalNote, TechnicalNote},
};

use super::{customer::ArchiveFilter, customer::RepositoryPage, RepositoryError};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TechnicalNoteFilter {
    pub query: Option<String>,
    pub tags: Vec<String>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub engine: Option<String>,
    pub archive: ArchiveFilter,
}

impl CollectionFilter for TechnicalNoteFilter {
    fn fingerprint_bytes(&self) -> Vec<u8> {
        format!(
            "technical_notes:v1:{}:{}:{}:{}:{:?}:{}",
            self.query.as_deref().unwrap_or(""),
            self.tags.join("\u{1f}"),
            self.make.as_deref().unwrap_or(""),
            self.model.as_deref().unwrap_or(""),
            self.archive,
            self.engine.as_deref().unwrap_or("")
        )
        .into_bytes()
    }
}

#[async_trait]
pub trait TechnicalNoteRepository: Send + Sync {
    async fn create(&self, value: &NewTechnicalNote) -> Result<TechnicalNote, RepositoryError>;
    async fn find_by_id(
        &self,
        id: &TechnicalNoteId,
    ) -> Result<Option<TechnicalNote>, RepositoryError>;
    async fn update(
        &self,
        id: &TechnicalNoteId,
        value: &NewTechnicalNote,
    ) -> Result<TechnicalNote, RepositoryError>;
    async fn list(
        &self,
        filter: &TechnicalNoteFilter,
        limit: PageLimit,
        after: Option<&CursorTuple>,
    ) -> Result<RepositoryPage<TechnicalNote>, RepositoryError>;
    async fn archive(&self, id: &TechnicalNoteId) -> Result<TechnicalNote, RepositoryError>;
    async fn restore(&self, id: &TechnicalNoteId) -> Result<TechnicalNote, RepositoryError>;
}
