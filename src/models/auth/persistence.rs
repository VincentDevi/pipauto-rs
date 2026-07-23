//! Private SurrealDB authentication persistence.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, TimeDelta, Utc};
use serde::Deserialize;
use surrealdb::{
    engine::any::Any,
    types::{RecordId, SurrealValue, ToSql},
    Surreal,
};

use crate::{
    database::surreal_support as support,
    models::auth::{
        repository::{
            AuthSessionRepository, LoginThrottleRepository, RepositoryError, RevokeOutcome,
            ThrottleState, UserRepository,
        },
        AuthSession, NewAuthSession, NewUserRecord, NormalizedEmail, SessionDigest, ThrottleDigest,
        User, UserCredentials, UserId, USER_AGENT_SUMMARY_MAX_CHARS,
    },
};

/// Shared SurrealDB implementation of all authentication repository contracts.
#[derive(Clone)]
pub struct SurrealAuthRepository {
    client: Surreal<Any>,
}

impl SurrealAuthRepository {
    /// Construct the adapter around the application-managed client.
    #[must_use]
    pub fn new(client: Surreal<Any>) -> Self {
        Self { client }
    }

    async fn throttle_row(
        &self,
        identifier_digest: &ThrottleDigest,
        network_digest: &ThrottleDigest,
    ) -> Result<Option<DbThrottle>, RepositoryError> {
        let mut response = support::checked_response(
            self.client
                .query(
                    "SELECT failed_attempts, window_started_at, blocked_until \
                     FROM login_throttle WHERE identifier_digest = $identifier \
                     AND network_digest = $network LIMIT 1;",
                )
                .bind(("identifier", identifier_digest.as_str().to_owned()))
                .bind(("network", network_digest.as_str().to_owned()))
                .await,
        )?;
        support::take(&mut response, 0)
    }
}

#[derive(Deserialize, SurrealValue)]
struct DbUser {
    id: RecordId,
    email: String,
    display_name: String,
    active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize, SurrealValue)]
struct DbUserCredentials {
    id: RecordId,
    email: String,
    display_name: String,
    password_hash: String,
    active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Deserialize, SurrealValue)]
struct DbAuthSession {
    user: RecordId,
    jti_digest: String,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    last_seen_at: Option<DateTime<Utc>>,
    created_ip_digest: Option<String>,
    user_agent_summary: Option<String>,
}

#[derive(Clone, Copy, Deserialize, SurrealValue)]
struct DbThrottle {
    failed_attempts: u32,
    window_started_at: DateTime<Utc>,
    blocked_until: Option<DateTime<Utc>>,
}

impl TryFrom<DbUser> for User {
    type Error = RepositoryError;

    fn try_from(value: DbUser) -> Result<Self, Self::Error> {
        Ok(Self {
            id: UserId::parse(value.id.to_sql()).map_err(|_| RepositoryError::CorruptData)?,
            email: value.email,
            display_name: value.display_name,
            active: value.active,
            created_at: value.created_at,
            updated_at: value.updated_at,
        })
    }
}

impl TryFrom<DbUserCredentials> for UserCredentials {
    type Error = RepositoryError;

    fn try_from(value: DbUserCredentials) -> Result<Self, Self::Error> {
        let user = User {
            id: UserId::parse(value.id.to_sql()).map_err(|_| RepositoryError::CorruptData)?,
            email: value.email,
            display_name: value.display_name,
            active: value.active,
            created_at: value.created_at,
            updated_at: value.updated_at,
        };
        Ok(UserCredentials::new(user, value.password_hash))
    }
}

impl TryFrom<DbAuthSession> for AuthSession {
    type Error = RepositoryError;

    fn try_from(value: DbAuthSession) -> Result<Self, Self::Error> {
        Ok(Self {
            user_id: UserId::parse(value.user.to_sql())
                .map_err(|_| RepositoryError::CorruptData)?,
            jti_digest: SessionDigest::parse(value.jti_digest)
                .map_err(|_| RepositoryError::CorruptData)?,
            issued_at: value.issued_at,
            expires_at: value.expires_at,
            revoked_at: value.revoked_at,
            last_seen_at: value.last_seen_at,
            created_ip_digest: value
                .created_ip_digest
                .map(ThrottleDigest::parse)
                .transpose()
                .map_err(|_| RepositoryError::CorruptData)?,
            user_agent_summary: value.user_agent_summary,
        })
    }
}

#[async_trait]
impl UserRepository for SurrealAuthRepository {
    async fn find_by_id(&self, id: &UserId) -> Result<Option<User>, RepositoryError> {
        let mut response = support::checked_response(
            self.client
                .query(
                    "SELECT id, email, display_name, active, created_at, updated_at \
                     FROM $record LIMIT 1;",
                )
                .bind(("record", user_record_id(id)?))
                .await,
        )?;
        let user: Option<DbUser> = support::take(&mut response, 0)?;
        user.map(TryInto::try_into).transpose()
    }

    async fn find_credentials_by_email(
        &self,
        normalized_email: &NormalizedEmail,
    ) -> Result<Option<UserCredentials>, RepositoryError> {
        let mut response = support::checked_response(
            self.client
                .query(
                    "SELECT id, email, display_name, password_hash, active, created_at, \
                     updated_at FROM user WHERE email_normalized = $email LIMIT 1;",
                )
                .bind(("email", normalized_email.as_str().to_owned()))
                .await,
        )?;
        let user: Option<DbUserCredentials> = support::take(&mut response, 0)?;
        user.map(TryInto::try_into).transpose()
    }

    async fn create(&self, new_user: NewUserRecord) -> Result<User, RepositoryError> {
        let mut response = self
            .client
            .query(
                "CREATE user SET email = $email, email_normalized = $normalized, \
                 display_name = $display_name, password_hash = $password_hash, active = true, \
                 created_at = time::now(), updated_at = time::now() RETURN AFTER;",
            )
            .bind(("email", new_user.email))
            .bind(("normalized", new_user.email_normalized.as_str().to_owned()))
            .bind(("display_name", new_user.display_name))
            .bind(("password_hash", new_user.password_hash))
            .await
            .map_err(classify_write_error)?
            .check()
            .map_err(classify_write_error)?;
        let user: Option<DbUser> = support::take(&mut response, 0)?;
        user.ok_or(RepositoryError::CorruptData)?.try_into()
    }
}

#[async_trait]
impl AuthSessionRepository for SurrealAuthRepository {
    async fn create(&self, session: NewAuthSession) -> Result<AuthSession, RepositoryError> {
        let user_agent_summary = session.user_agent_summary.map(|summary| {
            summary
                .chars()
                .filter(|character| !character.is_control())
                .take(USER_AGENT_SUMMARY_MAX_CHARS)
                .collect::<String>()
        });
        let mut response = self
            .client
            .query(
                "CREATE auth_session SET user = $user, jti_digest = $digest, \
                 issued_at = $issued_at, expires_at = $expires_at, revoked_at = NONE, \
                 last_seen_at = NONE, created_ip_digest = $created_ip_digest, \
                 user_agent_summary = $user_agent_summary RETURN AFTER;",
            )
            .bind(("user", user_record_id(&session.user_id)?))
            .bind(("digest", session.jti_digest.as_str().to_owned()))
            .bind(("issued_at", session.issued_at))
            .bind(("expires_at", session.expires_at))
            .bind((
                "created_ip_digest",
                session
                    .created_ip_digest
                    .map(|digest| digest.as_str().to_owned()),
            ))
            .bind(("user_agent_summary", user_agent_summary))
            .await
            .map_err(classify_write_error)?
            .check()
            .map_err(classify_write_error)?;
        let record: Option<DbAuthSession> = support::take(&mut response, 0)?;
        record.ok_or(RepositoryError::CorruptData)?.try_into()
    }

    async fn find_active(
        &self,
        jti_digest: &SessionDigest,
        now: DateTime<Utc>,
    ) -> Result<Option<AuthSession>, RepositoryError> {
        let mut response = support::checked_response(
            self.client
                .query(
                    "SELECT user, jti_digest, issued_at, expires_at, revoked_at, last_seen_at, \
                     created_ip_digest, user_agent_summary FROM auth_session WHERE \
                     jti_digest = $digest AND revoked_at IS NONE AND expires_at > $now LIMIT 1;",
                )
                .bind(("digest", jti_digest.as_str().to_owned()))
                .bind(("now", now))
                .await,
        )?;
        let session: Option<DbAuthSession> = support::take(&mut response, 0)?;
        session.map(TryInto::try_into).transpose()
    }

    async fn revoke(
        &self,
        jti_digest: &SessionDigest,
        now: DateTime<Utc>,
    ) -> Result<RevokeOutcome, RepositoryError> {
        let mut response = support::checked_response(
            self.client
                .query(
                    "UPDATE auth_session SET revoked_at = $now WHERE jti_digest = $digest \
                     AND revoked_at IS NONE RETURN BEFORE;",
                )
                .bind(("digest", jti_digest.as_str().to_owned()))
                .bind(("now", now))
                .await,
        )?;
        let changed: Vec<DbAuthSession> = support::take(&mut response, 0)?;
        Ok(if changed.is_empty() {
            RevokeOutcome::AlreadyInactive
        } else {
            RevokeOutcome::Revoked
        })
    }

    async fn revoke_all_for_user(
        &self,
        user_id: &UserId,
        now: DateTime<Utc>,
    ) -> Result<u64, RepositoryError> {
        changed_count(
            self.client
                .query(
                    "UPDATE auth_session SET revoked_at = $now WHERE user = $user \
                     AND revoked_at IS NONE RETURN BEFORE;",
                )
                .bind(("user", user_record_id(user_id)?))
                .bind(("now", now))
                .await,
        )
    }

    async fn delete_expired(&self, now: DateTime<Utc>) -> Result<u64, RepositoryError> {
        changed_count(
            self.client
                .query("DELETE auth_session WHERE expires_at < $now RETURN BEFORE;")
                .bind(("now", now))
                .await,
        )
    }
}

#[async_trait]
impl LoginThrottleRepository for SurrealAuthRepository {
    async fn state(
        &self,
        identifier_digest: &ThrottleDigest,
        network_digest: &ThrottleDigest,
        now: DateTime<Utc>,
    ) -> Result<ThrottleState, RepositoryError> {
        let row = self.throttle_row(identifier_digest, network_digest).await?;
        Ok(row
            .and_then(|row| row.blocked_until)
            .filter(|until| *until > now)
            .map_or(ThrottleState::Allowed, ThrottleState::BlockedUntil))
    }

    async fn record_failure(
        &self,
        identifier_digest: &ThrottleDigest,
        network_digest: &ThrottleDigest,
        now: DateTime<Utc>,
        window: Duration,
        maximum_attempts: u32,
        block_duration: Duration,
    ) -> Result<ThrottleState, RepositoryError> {
        let existing = self.throttle_row(identifier_digest, network_digest).await?;
        if let Some(until) = existing.and_then(|row| row.blocked_until) {
            if until > now {
                return Ok(ThrottleState::BlockedUntil(until));
            }
        }
        let window_delta = duration_delta(window)?;
        let reset = existing.is_none_or(|row| now - row.window_started_at >= window_delta);
        let failed_attempts = if reset {
            1
        } else {
            existing.map_or(1, |row| row.failed_attempts.saturating_add(1))
        };
        let window_started_at = if reset {
            now
        } else {
            existing.map_or(now, |row| row.window_started_at)
        };
        let block_delta = duration_delta(block_duration)?;
        let blocked_until = (failed_attempts >= maximum_attempts).then_some(now + block_delta);
        self.client
            .query(
                "UPSERT login_throttle SET identifier_digest = $identifier, \
                 network_digest = $network, failed_attempts = $attempts, \
                 window_started_at = $window_started, blocked_until = $blocked_until \
                 WHERE identifier_digest = $identifier \
                 AND network_digest = $network;",
            )
            .bind(("identifier", identifier_digest.as_str().to_owned()))
            .bind(("network", network_digest.as_str().to_owned()))
            .bind(("attempts", failed_attempts))
            .bind(("window_started", window_started_at))
            .bind(("blocked_until", blocked_until))
            .await
            .map_err(|error| support::classify_query_error(&error))?
            .check()
            .map_err(|error| support::classify_query_error(&error))?;
        Ok(blocked_until.map_or(ThrottleState::Allowed, ThrottleState::BlockedUntil))
    }

    async fn clear(
        &self,
        identifier_digest: &ThrottleDigest,
        network_digest: &ThrottleDigest,
    ) -> Result<(), RepositoryError> {
        self.client
            .query(
                "DELETE login_throttle WHERE identifier_digest = $identifier \
                 AND network_digest = $network;",
            )
            .bind(("identifier", identifier_digest.as_str().to_owned()))
            .bind(("network", network_digest.as_str().to_owned()))
            .await
            .map_err(|error| support::classify_query_error(&error))?
            .check()
            .map_err(|error| support::classify_query_error(&error))?;
        Ok(())
    }
}

fn user_record_id(id: &UserId) -> Result<RecordId, RepositoryError> {
    let (_, key) = id
        .as_str()
        .split_once(':')
        .ok_or(RepositoryError::CorruptData)?;
    support::record_id("user", key)
}

fn classify_write_error(error: surrealdb::Error) -> RepositoryError {
    support::classify_query_error(&error)
}

fn duration_delta(duration: Duration) -> Result<TimeDelta, RepositoryError> {
    TimeDelta::from_std(duration).map_err(|_| RepositoryError::Unavailable)
}

fn changed_count(
    response: surrealdb::Result<surrealdb::IndexedResults>,
) -> Result<u64, RepositoryError> {
    let mut response = support::checked_response(response)?;
    let changed: Vec<DbAuthSession> = support::take(&mut response, 0)?;
    u64::try_from(changed.len()).map_err(|_| RepositoryError::CorruptData)
}
