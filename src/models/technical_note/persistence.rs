//! Private SurrealDB technical-note persistence.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use surrealdb::{
    engine::any::Any,
    types::{RecordId, SurrealValue},
    Surreal,
};

use crate::{
    domain::{CursorSortValue, CursorTuple, InterventionId, PageLimit, TechnicalNoteId, VehicleId},
    models::{
        customer::repository::{ArchiveFilter, RepositoryPage},
        persistence_error::PersistenceError as RepositoryError,
        technical_note::{
            repository::{TechnicalNoteFilter, TechnicalNoteRepository},
            NewTechnicalNote, TechnicalNote, TechnicalNoteContext,
        },
    },
};

use crate::database::surreal_support as support;

const PROJECTION: &str = "id, title, body, tags, vehicle, source_intervention, make, make_normalized, model, model_normalized, engine, engine_normalized, created_at, updated_at, archived_at";
const RELEVANCE: &str = "(IF $query IS NONE THEN 0 ELSE (IF title @0@ $query THEN 2 ELSE 0 END) + (IF body @1@ $query THEN 1 ELSE 0 END) END)";

#[derive(Clone)]
pub struct SurrealTechnicalNoteRepository {
    client: Surreal<Any>,
}

impl SurrealTechnicalNoteRepository {
    #[must_use]
    pub fn new(client: Surreal<Any>) -> Self {
        Self { client }
    }

    async fn set_archive(
        &self,
        id: &TechnicalNoteId,
        archived: bool,
    ) -> Result<TechnicalNote, RepositoryError> {
        let predicate = if archived {
            "archived_at IS NONE"
        } else {
            "archived_at IS NOT NONE"
        };
        let value = if archived { "time::now()" } else { "NONE" };
        let query = format!(
            "UPDATE ONLY $record SET archived_at = {value} WHERE {predicate} RETURN AFTER;"
        );
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind(("record", support::record_id("technical_note", id.as_str())?))
                .await,
        )?;
        let changed: Option<DbTechnicalNote> = support::take(&mut response, 0)?;
        match changed {
            Some(row) => row.try_into(),
            None => self.find_by_id(id).await?.ok_or(RepositoryError::NotFound),
        }
    }
}

#[derive(Deserialize, SurrealValue)]
struct DbTechnicalNote {
    id: RecordId,
    title: String,
    body: String,
    tags: Vec<String>,
    vehicle: Option<RecordId>,
    source_intervention: Option<RecordId>,
    make: Option<String>,
    make_normalized: Option<String>,
    model: Option<String>,
    model_normalized: Option<String>,
    engine: Option<String>,
    engine_normalized: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    archived_at: Option<DateTime<Utc>>,
    #[serde(default)]
    relevance: Option<i64>,
}

impl TryFrom<DbTechnicalNote> for TechnicalNote {
    type Error = RepositoryError;

    fn try_from(value: DbTechnicalNote) -> Result<Self, Self::Error> {
        Ok(Self {
            id: TechnicalNoteId::parse(support::record_key(&value.id, "technical_note")?)
                .map_err(|_| RepositoryError::CorruptData)?,
            title: value.title,
            body: value.body,
            tags: value.tags,
            vehicle_id: value
                .vehicle
                .map(|id| {
                    VehicleId::parse(support::record_key(&id, "vehicle")?)
                        .map_err(|_| RepositoryError::CorruptData)
                })
                .transpose()?,
            source_intervention_id: value
                .source_intervention
                .map(|id| {
                    InterventionId::parse(support::record_key(&id, "intervention")?)
                        .map_err(|_| RepositoryError::CorruptData)
                })
                .transpose()?,
            make: context(value.make, value.make_normalized)?,
            model: context(value.model, value.model_normalized)?,
            engine: context(value.engine, value.engine_normalized)?,
            created_at: value.created_at,
            updated_at: value.updated_at,
            archived_at: value.archived_at,
        })
    }
}

fn context(
    display: Option<String>,
    normalized: Option<String>,
) -> Result<Option<TechnicalNoteContext>, RepositoryError> {
    match (display, normalized) {
        (None, None) => Ok(None),
        (Some(display), Some(normalized)) => Ok(Some(TechnicalNoteContext {
            display,
            normalized,
        })),
        _ => Err(RepositoryError::CorruptData),
    }
}

#[async_trait]
impl TechnicalNoteRepository for SurrealTechnicalNoteRepository {
    async fn create(&self, value: &NewTechnicalNote) -> Result<TechnicalNote, RepositoryError> {
        let mut response = write_query(
            &self.client,
            "CREATE technical_note SET title = $title, body = $body, tags = $tags, vehicle = $vehicle, source_intervention = $source_intervention, make = $make, make_normalized = $make_normalized, model = $model, model_normalized = $model_normalized, engine = $engine, engine_normalized = $engine_normalized, archived_at = NONE RETURN AFTER;",
            None,
            value,
        ).await?;
        let row: Option<DbTechnicalNote> = support::take(&mut response, 0)?;
        row.ok_or(RepositoryError::CorruptData)?.try_into()
    }

    async fn find_by_id(
        &self,
        id: &TechnicalNoteId,
    ) -> Result<Option<TechnicalNote>, RepositoryError> {
        let query = format!("SELECT {PROJECTION} FROM ONLY $record;");
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind(("record", support::record_id("technical_note", id.as_str())?))
                .await,
        )?;
        let row: Option<DbTechnicalNote> = support::take(&mut response, 0)?;
        row.map(TryInto::try_into).transpose()
    }

    async fn update(
        &self,
        id: &TechnicalNoteId,
        value: &NewTechnicalNote,
    ) -> Result<TechnicalNote, RepositoryError> {
        let mut response = write_query(
            &self.client,
            "UPDATE ONLY $record SET title = $title, body = $body, tags = $tags, vehicle = $vehicle, source_intervention = $source_intervention, make = $make, make_normalized = $make_normalized, model = $model, model_normalized = $model_normalized, engine = $engine, engine_normalized = $engine_normalized RETURN AFTER;",
            Some(support::record_id("technical_note", id.as_str())?),
            value,
        ).await?;
        let row: Option<DbTechnicalNote> = support::take(&mut response, 0)?;
        row.ok_or(RepositoryError::NotFound)?.try_into()
    }

    async fn list(
        &self,
        filter: &TechnicalNoteFilter,
        limit: PageLimit,
        after: Option<&CursorTuple>,
    ) -> Result<RepositoryPage<TechnicalNote>, RepositoryError> {
        let (after_relevance, after_created_at, after_id) = after
            .map(search_cursor_values)
            .transpose()?
            .map_or((None, None, None), |(relevance, created_at, id)| {
                (Some(relevance), Some(created_at), Some(id))
            });
        let query = format!(
            "SELECT {PROJECTION}, {RELEVANCE} AS relevance FROM technical_note WHERE ($archive = 'all' OR ($archive = 'active' AND archived_at IS NONE) OR ($archive = 'archived' AND archived_at IS NOT NONE)) AND ($query IS NONE OR title @0@ $query OR body @1@ $query) AND array::all($tags, |$tag| tags CONTAINS $tag) AND ($make IS NONE OR make_normalized = $make) AND ($model IS NONE OR model_normalized = $model) AND ($engine IS NONE OR engine_normalized = $engine) AND ($after_relevance IS NONE OR {RELEVANCE} < $after_relevance OR ({RELEVANCE} = $after_relevance AND created_at < $after_created_at) OR ({RELEVANCE} = $after_relevance AND created_at = $after_created_at AND id < $after_id)) ORDER BY relevance DESC, created_at DESC, id DESC LIMIT $fetch_limit;"
        );
        let mut response = support::checked_response(
            self.client
                .query(query)
                .bind(("archive", archive_value(filter.archive).to_owned()))
                .bind(("query", filter.query.clone()))
                .bind(("tags", filter.tags.clone()))
                .bind(("make", filter.make.clone()))
                .bind(("model", filter.model.clone()))
                .bind(("engine", filter.engine.clone()))
                .bind(("after_relevance", after_relevance))
                .bind(("after_created_at", after_created_at))
                .bind(("after_id", after_id))
                .bind(("fetch_limit", i64::from(limit.value()) + 1))
                .await,
        )?;
        let mut rows: Vec<DbTechnicalNote> = support::take(&mut response, 0)?;
        let has_more = rows.len() > usize::from(limit.value());
        if has_more {
            rows.pop();
        }
        let next = if has_more {
            rows.last().map(search_cursor).transpose()?
        } else {
            None
        };
        let items = rows
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(RepositoryPage { items, next })
    }

    async fn archive(&self, id: &TechnicalNoteId) -> Result<TechnicalNote, RepositoryError> {
        self.set_archive(id, true).await
    }

    async fn restore(&self, id: &TechnicalNoteId) -> Result<TechnicalNote, RepositoryError> {
        self.set_archive(id, false).await
    }
}

fn archive_value(filter: ArchiveFilter) -> &'static str {
    match filter {
        ArchiveFilter::Active => "active",
        ArchiveFilter::Archived => "archived",
        ArchiveFilter::All => "all",
    }
}

fn search_cursor(row: &DbTechnicalNote) -> Result<CursorTuple, RepositoryError> {
    CursorTuple::new(
        vec![
            CursorSortValue::Integer(row.relevance.ok_or(RepositoryError::CorruptData)?),
            CursorSortValue::Timestamp(row.created_at),
        ],
        support::record_key(&row.id, "technical_note")?,
    )
    .map_err(|_| RepositoryError::CorruptData)
}

fn search_cursor_values(
    tuple: &CursorTuple,
) -> Result<(i64, DateTime<Utc>, RecordId), RepositoryError> {
    let [CursorSortValue::Integer(relevance), CursorSortValue::Timestamp(created_at)] =
        tuple.sort_values()
    else {
        return Err(RepositoryError::CorruptData);
    };
    Ok((
        *relevance,
        *created_at,
        support::record_id("technical_note", tuple.entity_key())?,
    ))
}

async fn write_query(
    client: &Surreal<Any>,
    query: &str,
    record: Option<RecordId>,
    value: &NewTechnicalNote,
) -> Result<surrealdb::IndexedResults, RepositoryError> {
    support::checked_response(
        client
            .query(query)
            .bind(("record", record))
            .bind(("title", value.title.clone()))
            .bind(("body", value.body.clone()))
            .bind(("tags", value.tags.clone()))
            .bind((
                "vehicle",
                value
                    .vehicle_id
                    .as_ref()
                    .map(|id| support::record_id("vehicle", id.as_str()))
                    .transpose()?,
            ))
            .bind((
                "source_intervention",
                value
                    .source_intervention_id
                    .as_ref()
                    .map(|id| support::record_id("intervention", id.as_str()))
                    .transpose()?,
            ))
            .bind(("make", value.make.as_ref().map(|v| v.display.clone())))
            .bind((
                "make_normalized",
                value.make.as_ref().map(|v| v.normalized.clone()),
            ))
            .bind(("model", value.model.as_ref().map(|v| v.display.clone())))
            .bind((
                "model_normalized",
                value.model.as_ref().map(|v| v.normalized.clone()),
            ))
            .bind(("engine", value.engine.as_ref().map(|v| v.display.clone())))
            .bind((
                "engine_normalized",
                value.engine.as_ref().map(|v| v.normalized.clone()),
            ))
            .await,
    )
}
