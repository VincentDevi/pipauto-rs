//! Shared SurrealDB boundary mechanics.

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue, ToSql};

use crate::{
    domain::{CursorSortValue, CursorTuple},
    repositories::RepositoryError,
};

/// Build a record ID only after validating its database-independent key.
pub fn record_id(table: &'static str, key: &str) -> Result<RecordId, RepositoryError> {
    let valid = !table.is_empty()
        && table
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        && !key.is_empty()
        && key.len() <= 256
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if !valid {
        return Err(RepositoryError::CorruptData);
    }
    Ok(RecordId::new(table, key.to_owned()))
}

/// Extract a key from a record ID while enforcing the expected table.
pub fn record_key(record: &RecordId, table: &'static str) -> Result<String, RepositoryError> {
    let serialized = record.to_sql();
    let (actual_table, key) = serialized
        .split_once(':')
        .ok_or(RepositoryError::CorruptData)?;
    if actual_table != table || key.is_empty() {
        return Err(RepositoryError::CorruptData);
    }
    Ok(key.to_owned())
}

/// Classify an execution/query failure without exposing its message outside the adapter.
#[must_use]
pub fn classify_query_error(error: &surrealdb::Error) -> RepositoryError {
    let message = error.to_string().to_ascii_lowercase();
    if [
        "unique",
        "already contains",
        "immutable",
        "mileage",
        "draft",
        "state transition",
        "currency cannot conflict",
    ]
    .iter()
    .any(|needle| message.contains(needle))
    {
        RepositoryError::Conflict
    } else if [
        "connection",
        "network",
        "socket",
        "timeout",
        "unavailable",
        "closed",
    ]
    .iter()
    .any(|needle| message.contains(needle))
    {
        RepositoryError::Unavailable
    } else {
        RepositoryError::CorruptData
    }
}

/// Validate a multi-statement response before extracting rows.
pub fn checked_response(
    response: surrealdb::Result<surrealdb::IndexedResults>,
) -> Result<surrealdb::IndexedResults, RepositoryError> {
    response
        .map_err(|error| classify_query_error(&error))?
        .check()
        .map_err(|error| classify_query_error(&error))
}

/// Extract one typed statement result. Shape/deserialization failures indicate corrupt or
/// unexpected persistence data, never not-found.
pub fn take<T>(response: &mut surrealdb::IndexedResults, index: usize) -> Result<T, RepositoryError>
where
    T: SurrealValue,
    usize: surrealdb::opt::QueryResult<T>,
{
    response
        .take(index)
        .map_err(|_| RepositoryError::CorruptData)
}

/// Convert a Surreal record tuple into the persistence-neutral cursor tuple.
pub fn cursor_tuple(
    timestamp: DateTime<Utc>,
    record: &RecordId,
    table: &'static str,
) -> Result<CursorTuple, RepositoryError> {
    CursorTuple::new(
        vec![CursorSortValue::Timestamp(timestamp)],
        record_key(record, table)?,
    )
    .map_err(|_| RepositoryError::CorruptData)
}

/// Convert a decoded cursor tuple into bound Surreal query values.
pub fn surreal_cursor_tuple(
    tuple: &CursorTuple,
    table: &'static str,
) -> Result<(DateTime<Utc>, RecordId), RepositoryError> {
    let [CursorSortValue::Timestamp(timestamp)] = tuple.sort_values() else {
        return Err(RepositoryError::CorruptData);
    };
    Ok((*timestamp, record_id(table, tuple.entity_key())?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_ids_enforce_table_and_key_boundaries() {
        let record = record_id("vehicle", "vehicle_1").expect("valid record");
        assert_eq!(
            record_key(&record, "vehicle").expect("matching table"),
            "vehicle_1"
        );
        assert_eq!(
            record_key(&record, "customer"),
            Err(RepositoryError::CorruptData)
        );
        assert_eq!(
            record_id("bad-table", "one"),
            Err(RepositoryError::CorruptData)
        );
    }
}
