//! Explicit, idempotent SurrealDB schema application.

use surrealdb::{engine::any::Any, Surreal};
use thiserror::Error;

/// Strict authentication schema applied by the `apply_auth_schema` task.
pub const AUTH_SCHEMA: &str = r#"
DEFINE TABLE IF NOT EXISTS user SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS email ON user TYPE string
    ASSERT string::len($value) > 0 AND string::len($value) <= 254;
DEFINE FIELD IF NOT EXISTS email_normalized ON user TYPE string
    ASSERT string::len($value) > 0 AND string::len($value) <= 254;
DEFINE FIELD IF NOT EXISTS display_name ON user TYPE string
    ASSERT string::len($value) > 0 AND string::len($value) <= 120;
DEFINE FIELD IF NOT EXISTS password_hash ON user TYPE string
    ASSERT string::starts_with($value, '$argon2id$');
DEFINE FIELD IF NOT EXISTS active ON user TYPE bool DEFAULT true;
DEFINE FIELD IF NOT EXISTS created_at ON user TYPE datetime DEFAULT time::now() READONLY;
DEFINE FIELD IF NOT EXISTS updated_at ON user TYPE datetime DEFAULT ALWAYS time::now();
DEFINE INDEX IF NOT EXISTS user_email_normalized_unique ON user FIELDS email_normalized UNIQUE;

DEFINE TABLE IF NOT EXISTS auth_session SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS user ON auth_session TYPE record<user>;
DEFINE FIELD IF NOT EXISTS jti_digest ON auth_session TYPE string;
DEFINE FIELD IF NOT EXISTS issued_at ON auth_session TYPE datetime;
DEFINE FIELD IF NOT EXISTS expires_at ON auth_session TYPE datetime;
DEFINE FIELD IF NOT EXISTS revoked_at ON auth_session TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS last_seen_at ON auth_session TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS created_ip_digest ON auth_session TYPE option<string>
    ASSERT $value IS NONE OR string::len($value) = 43;
DEFINE FIELD IF NOT EXISTS user_agent_summary ON auth_session TYPE option<string>
    ASSERT $value IS NONE OR (
        string::len($value) <= 256 AND !$value.contains('\\n') AND !$value.contains('\\r')
    );
DEFINE INDEX IF NOT EXISTS auth_session_jti_unique ON auth_session FIELDS jti_digest UNIQUE;
DEFINE INDEX IF NOT EXISTS auth_session_user ON auth_session FIELDS user;
DEFINE INDEX IF NOT EXISTS auth_session_expires_at ON auth_session FIELDS expires_at;

DEFINE TABLE IF NOT EXISTS login_throttle SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS identifier_digest ON login_throttle TYPE string
    ASSERT string::len($value) = 43;
DEFINE FIELD IF NOT EXISTS network_digest ON login_throttle TYPE string
    ASSERT string::len($value) = 43;
DEFINE FIELD IF NOT EXISTS failed_attempts ON login_throttle TYPE int DEFAULT 0;
DEFINE FIELD IF NOT EXISTS window_started_at ON login_throttle TYPE datetime;
DEFINE FIELD IF NOT EXISTS blocked_until ON login_throttle TYPE option<datetime>;
DEFINE INDEX IF NOT EXISTS login_throttle_key_unique ON login_throttle
    FIELDS identifier_digest, network_digest UNIQUE;
DEFINE INDEX IF NOT EXISTS login_throttle_blocked_until ON login_throttle FIELDS blocked_until;
"#;

/// Apply all authentication definitions to the selected namespace and database.
///
/// # Errors
///
/// Returns a secret-free schema error when SurrealDB rejects a definition.
pub async fn apply_auth_schema(client: &Surreal<Any>) -> Result<(), SchemaError> {
    let response = client.query(AUTH_SCHEMA).await.map_err(|_| SchemaError)?;
    response.check().map(|_| ()).map_err(|_| SchemaError)
}

/// Opaque schema-application failure.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("authentication schema application failed")]
pub struct SchemaError;
