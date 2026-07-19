//! Database-independent authentication domain values.

use std::fmt;

use chrono::{DateTime, Utc};
use thiserror::Error;

/// Maximum number of Unicode scalar values accepted for a user-facing display name.
pub const DISPLAY_NAME_MAX_CHARS: usize = 120;
/// Maximum number of Unicode scalar values stored in a non-authoritative user-agent summary.
pub const USER_AGENT_SUMMARY_MAX_CHARS: usize = 256;
/// Minimum password length measured in Unicode scalar values.
pub const PASSWORD_MIN_SCALARS: usize = 12;
/// Maximum password size measured in UTF-8 bytes to bound Argon2 work.
pub const PASSWORD_MAX_BYTES: usize = 1_024;

/// Stable application user record identifier.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct UserId(String);

impl UserId {
    /// Validate a SurrealDB `user` record identifier.
    ///
    /// # Errors
    ///
    /// Rejects identifiers outside the `user` table or containing no key.
    pub fn parse(value: impl Into<String>) -> Result<Self, AuthModelError> {
        let value = value.into();
        if !value
            .strip_prefix("user:")
            .is_some_and(|key| !key.trim().is_empty())
        {
            return Err(AuthModelError::InvalidUserId);
        }
        Ok(Self(value))
    }

    /// String form suitable for JWT `pid` and repository parameters.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Normalized email lookup key.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NormalizedEmail(String);

impl NormalizedEmail {
    /// Trim and ASCII-lowercase an email address used for equality lookup.
    ///
    /// # Errors
    ///
    /// Rejects empty or structurally invalid addresses.
    pub fn parse(value: &str) -> Result<Self, AuthModelError> {
        let normalized = value.trim().to_ascii_lowercase();
        let valid = normalized.len() <= 254
            && !normalized.contains(char::is_whitespace)
            && normalized
                .split_once('@')
                .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'));
        if !valid {
            return Err(AuthModelError::InvalidEmail);
        }
        Ok(Self(normalized))
    }

    /// Normalized string value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Presentation-safe application user.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct User {
    pub id: UserId,
    pub email: String,
    pub display_name: String,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Internal credential projection. Debug output deliberately hides the hash.
#[derive(Clone)]
pub struct UserCredentials {
    pub user: User,
    pub(crate) password_hash: String,
}

impl UserCredentials {
    /// Construct the internal credential projection.
    #[must_use]
    pub fn new(user: User, password_hash: String) -> Self {
        Self {
            user,
            password_hash,
        }
    }

    pub(crate) fn password_hash(&self) -> &str {
        &self.password_hash
    }
}

impl fmt::Debug for UserCredentials {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserCredentials")
            .field("user", &self.user)
            .field("password_hash", &"[REDACTED]")
            .finish()
    }
}

/// Validated new-user persistence input.
#[derive(Clone)]
pub struct NewUserRecord {
    pub email: String,
    pub email_normalized: NormalizedEmail,
    pub display_name: String,
    pub(crate) password_hash: String,
}

impl NewUserRecord {
    /// Construct a validated persistence input from an already-hashed password.
    ///
    /// # Errors
    ///
    /// Rejects an address that does not match its normalized lookup value, an invalid display name,
    /// or a value that is not shaped like an Argon2id PHC string.
    pub fn new(
        email: String,
        email_normalized: NormalizedEmail,
        display_name: String,
        password_hash: String,
    ) -> Result<Self, AuthModelError> {
        let email = email.trim().to_owned();
        if NormalizedEmail::parse(&email)? != email_normalized {
            return Err(AuthModelError::InvalidEmail);
        }
        let display_name = validate_display_name(&display_name)?;
        if !password_hash.starts_with("$argon2id$") {
            return Err(AuthModelError::InvalidPasswordHash);
        }
        Ok(Self {
            email,
            email_normalized,
            display_name,
            password_hash,
        })
    }
}

impl fmt::Debug for NewUserRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NewUserRecord")
            .field("email", &self.email)
            .field("email_normalized", &self.email_normalized)
            .field("display_name", &self.display_name)
            .field("password_hash", &"[REDACTED]")
            .finish()
    }
}

/// SHA-256 digest of a raw JWT identifier.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SessionDigest(String);

impl SessionDigest {
    /// Construct from a lowercase 64-character SHA-256 hex digest.
    ///
    /// # Errors
    ///
    /// Rejects malformed digests.
    pub fn parse(value: impl Into<String>) -> Result<Self, AuthModelError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(AuthModelError::InvalidSessionDigest);
        }
        Ok(Self(value))
    }

    /// Digest string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Keyed HMAC-SHA-256 digest used as one half of a login-throttle key.
///
/// Keeping this as a validated type prevents repository callers from accidentally persisting a
/// submitted email address or raw network address.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ThrottleDigest(String);

impl ThrottleDigest {
    /// Parse an unpadded base64url-encoded 256-bit keyed digest.
    ///
    /// # Errors
    ///
    /// Rejects values that do not have the exact HMAC-SHA-256 encoded shape.
    pub fn parse(value: impl Into<String>) -> Result<Self, AuthModelError> {
        let value = value.into();
        if value.len() != 43
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(AuthModelError::InvalidThrottleDigest);
        }
        Ok(Self(value))
    }

    /// Digest string suitable for a bound persistence parameter.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Revocable server-side session registry value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthSession {
    pub user_id: UserId,
    pub jti_digest: SessionDigest,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub created_ip_digest: Option<ThrottleDigest>,
    pub user_agent_summary: Option<String>,
}

/// New server-side session input.
#[derive(Clone, Debug)]
pub struct NewAuthSession {
    pub user_id: UserId,
    pub jti_digest: SessionDigest,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// Optional keyed digest of trusted client-network data; never a raw IP address.
    pub created_ip_digest: Option<ThrottleDigest>,
    /// Optional sanitized audit summary. It is never authoritative for access decisions.
    pub user_agent_summary: Option<String>,
}

/// Presentation-safe authenticated request identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticatedUser {
    pub id: UserId,
    pub email: String,
    pub display_name: String,
    pub session_expires_at: DateTime<Utc>,
}

/// Domain validation error.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AuthModelError {
    #[error("invalid user identifier")]
    InvalidUserId,
    #[error("invalid email address")]
    InvalidEmail,
    #[error("invalid session digest")]
    InvalidSessionDigest,
    #[error("invalid login-throttle digest")]
    InvalidThrottleDigest,
    #[error("display name must contain 1 to 120 characters")]
    InvalidDisplayName,
    #[error("password does not meet the authentication policy")]
    InvalidPassword,
    #[error("password hash is not an Argon2id PHC string")]
    InvalidPasswordHash,
}

/// Validate and trim a display name.
///
/// # Errors
///
/// Rejects empty names and values longer than [`DISPLAY_NAME_MAX_CHARS`].
pub fn validate_display_name(value: &str) -> Result<String, AuthModelError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > DISPLAY_NAME_MAX_CHARS {
        return Err(AuthModelError::InvalidDisplayName);
    }
    Ok(value.to_owned())
}

/// Validate a password without changing its bytes.
///
/// # Errors
///
/// Enforces the approved length bounds and rejects control characters or the normalized email.
pub fn validate_password(
    password: &str,
    normalized_email: &NormalizedEmail,
) -> Result<(), AuthModelError> {
    let scalar_count = password.chars().count();
    let valid = scalar_count >= PASSWORD_MIN_SCALARS
        && password.len() <= PASSWORD_MAX_BYTES
        && !password.chars().any(char::is_control)
        && password != normalized_email.as_str();
    if !valid {
        return Err(AuthModelError::InvalidPassword);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_normalization_is_trimmed_and_ascii_lowercase() {
        let email =
            NormalizedEmail::parse("  Filippo@Example.COM ").expect("email should normalize");
        assert_eq!(email.as_str(), "filippo@example.com");
    }

    #[test]
    fn password_authentication_boundaries_are_enforced_without_rewriting() {
        let email = NormalizedEmail::parse("filippo@example.com").expect("valid email");
        assert!(validate_password(" workshop pass ", &email).is_ok());
        assert_eq!(
            validate_password("short", &email),
            Err(AuthModelError::InvalidPassword)
        );
        assert_eq!(
            validate_password("filippo@example.com", &email),
            Err(AuthModelError::InvalidPassword)
        );
        assert!(validate_password("prefix-filippo@example.com-suffix", &email).is_ok());
        assert!(validate_password(&"x".repeat(PASSWORD_MAX_BYTES), &email).is_ok());
        assert_eq!(
            validate_password(&"x".repeat(PASSWORD_MAX_BYTES + 1), &email),
            Err(AuthModelError::InvalidPassword)
        );
        assert!(validate_password(&"é".repeat(512), &email).is_ok());
        assert_eq!(
            validate_password(&"é".repeat(513), &email),
            Err(AuthModelError::InvalidPassword)
        );
        assert_eq!(
            validate_password("valid length\n", &email),
            Err(AuthModelError::InvalidPassword)
        );
    }

    #[test]
    fn session_digest_requires_lowercase_sha256_hex() {
        assert!(SessionDigest::parse("a".repeat(64)).is_ok());
        assert_eq!(
            SessionDigest::parse("A".repeat(64)),
            Err(AuthModelError::InvalidSessionDigest)
        );
    }
}
