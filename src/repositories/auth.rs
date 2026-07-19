//! Persistence contracts for users, revocable sessions, and login throttling.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use thiserror::Error;

use crate::models::auth::{
    AuthSession, NewAuthSession, NewUserRecord, NormalizedEmail, SessionDigest, ThrottleDigest,
    User, UserCredentials, UserId,
};

/// Technology-independent persistence failure.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RepositoryError {
    #[error("record already exists")]
    Conflict,
    #[error("repository unavailable")]
    Unavailable,
}

/// Idempotent revocation result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RevokeOutcome {
    Revoked,
    AlreadyInactive,
}

/// Current temporary-throttle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThrottleState {
    Allowed,
    BlockedUntil(DateTime<Utc>),
}

/// Application user persistence.
#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_id(&self, id: &UserId) -> Result<Option<User>, RepositoryError>;
    async fn find_credentials_by_email(
        &self,
        normalized_email: &NormalizedEmail,
    ) -> Result<Option<UserCredentials>, RepositoryError>;
    async fn create(&self, new_user: NewUserRecord) -> Result<User, RepositoryError>;
}

/// Revocable browser-session persistence.
#[async_trait]
pub trait AuthSessionRepository: Send + Sync {
    async fn create(&self, session: NewAuthSession) -> Result<AuthSession, RepositoryError>;
    async fn find_active(
        &self,
        jti_digest: &SessionDigest,
        now: DateTime<Utc>,
    ) -> Result<Option<AuthSession>, RepositoryError>;
    async fn revoke(
        &self,
        jti_digest: &SessionDigest,
        now: DateTime<Utc>,
    ) -> Result<RevokeOutcome, RepositoryError>;
    async fn revoke_all_for_user(
        &self,
        user_id: &UserId,
        now: DateTime<Utc>,
    ) -> Result<u64, RepositoryError>;
    async fn delete_expired(&self, now: DateTime<Utc>) -> Result<u64, RepositoryError>;
}

/// Repository-backed, account-and-network-aware temporary login throttling.
#[async_trait]
pub trait LoginThrottleRepository: Send + Sync {
    async fn state(
        &self,
        identifier_digest: &ThrottleDigest,
        network_digest: &ThrottleDigest,
        now: DateTime<Utc>,
    ) -> Result<ThrottleState, RepositoryError>;
    #[allow(clippy::too_many_arguments)]
    async fn record_failure(
        &self,
        identifier_digest: &ThrottleDigest,
        network_digest: &ThrottleDigest,
        now: DateTime<Utc>,
        window: Duration,
        maximum_attempts: u32,
        block_duration: Duration,
    ) -> Result<ThrottleState, RepositoryError>;
    async fn clear(
        &self,
        identifier_digest: &ThrottleDigest,
        network_digest: &ThrottleDigest,
    ) -> Result<(), RepositoryError>;
}
