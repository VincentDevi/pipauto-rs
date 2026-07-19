//! Shared archive and chronology values.

use chrono::{DateTime, Utc};
use thiserror::Error;

/// Creation and last-update timestamps with preserved chronology.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntityTimestamps {
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl EntityTimestamps {
    /// Construct timestamps whose update cannot predate creation.
    pub fn new(
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, TimestampError> {
        if updated_at < created_at {
            return Err(TimestampError::UpdatedBeforeCreated);
        }
        Ok(Self {
            created_at,
            updated_at,
        })
    }

    #[must_use]
    pub const fn created_at(self) -> DateTime<Utc> {
        self.created_at
    }

    #[must_use]
    pub const fn updated_at(self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Advance the last-update timestamp without permitting history to move backward.
    pub fn touch(&mut self, at: DateTime<Utc>) -> Result<(), TimestampError> {
        if at < self.updated_at {
            return Err(TimestampError::UpdateMovedBackward);
        }
        self.updated_at = at;
        Ok(())
    }
}

/// Explicit soft-archive state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArchiveState {
    Active,
    Archived { archived_at: DateTime<Utc> },
}

impl ArchiveState {
    #[must_use]
    pub const fn is_archived(self) -> bool {
        matches!(self, Self::Archived { .. })
    }

    /// Archive an active record at or after its creation.
    pub fn archive(
        self,
        archived_at: DateTime<Utc>,
        timestamps: &mut EntityTimestamps,
    ) -> Result<Self, TimestampError> {
        if self.is_archived() {
            return Err(TimestampError::AlreadyArchived);
        }
        if archived_at < timestamps.created_at() {
            return Err(TimestampError::ArchivedBeforeCreated);
        }
        timestamps.touch(archived_at)?;
        Ok(Self::Archived { archived_at })
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum TimestampError {
    #[error("updated timestamp cannot predate creation")]
    UpdatedBeforeCreated,
    #[error("updated timestamp cannot move backward")]
    UpdateMovedBackward,
    #[error("archive timestamp cannot predate creation")]
    ArchivedBeforeCreated,
    #[error("record is already archived")]
    AlreadyArchived,
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;

    use super::*;

    #[test]
    fn domain_archive_state_preserves_timestamp_chronology() {
        let created = Utc::now();
        let earlier = created - TimeDelta::seconds(1);
        assert_eq!(
            EntityTimestamps::new(created, earlier),
            Err(TimestampError::UpdatedBeforeCreated)
        );

        let mut timestamps = EntityTimestamps::new(created, created).expect("valid chronology");
        assert_eq!(
            ArchiveState::Active.archive(earlier, &mut timestamps),
            Err(TimestampError::ArchivedBeforeCreated)
        );
        let later = created + TimeDelta::seconds(1);
        let archived = ArchiveState::Active
            .archive(later, &mut timestamps)
            .expect("valid archive");
        assert!(archived.is_archived());
        assert_eq!(timestamps.updated_at(), later);
    }
}
