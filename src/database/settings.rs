//! Typed and validated `SurrealDB` configuration.

use std::{fmt, time::Duration};

use loco_rs::config::Config;
use serde::{de::Error as _, Deserialize, Deserializer};
use thiserror::Error;

const SETTINGS_KEY: &str = "surrealdb";

/// The database engine selected for this application process.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseEngine {
    /// Connect to a standalone `SurrealDB` server over WebSockets.
    Websocket,
    /// Run an isolated embedded database in memory.
    Memory,
}

/// Validated settings needed to initialize the application database.
#[derive(Clone)]
pub struct DatabaseSettings {
    endpoint: String,
    username: String,
    password: String,
    namespace: String,
    database: String,
    connection_timeout: Duration,
    engine: DatabaseEngine,
}

impl DatabaseSettings {
    /// Deserialize and validate the `settings.surrealdb` configuration section.
    ///
    /// # Errors
    ///
    /// Returns an error when the section is absent, malformed, or contains an invalid setting.
    pub fn from_config(config: &Config) -> Result<Self, DatabaseSettingsError> {
        let settings = config
            .settings
            .as_ref()
            .and_then(|settings| settings.get(SETTINGS_KEY))
            .cloned()
            .ok_or(DatabaseSettingsError::MissingSection)?;

        serde_json::from_value(settings).map_err(|error| DatabaseSettingsError::InvalidFormat {
            message: error.to_string(),
        })
    }

    /// Server endpoint, or `memory` for the in-memory engine.
    #[must_use]
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Username used for remote root authentication.
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Password used for remote root authentication.
    #[must_use]
    pub(crate) fn password(&self) -> &str {
        &self.password
    }

    /// Selected namespace.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Selected database name.
    #[must_use]
    pub fn database(&self) -> &str {
        &self.database
    }

    /// Maximum duration allowed for each startup database operation.
    #[must_use]
    pub fn connection_timeout(&self) -> Duration {
        self.connection_timeout
    }

    /// Selected database engine.
    #[must_use]
    pub fn engine(&self) -> DatabaseEngine {
        self.engine
    }
}

impl<'de> Deserialize<'de> for DatabaseSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let raw = serde_json::from_value::<RawDatabaseSettings>(value)
            .map_err(|error| D::Error::custom(safe_deserialization_error(&error)))?;
        Self::try_from(raw).map_err(D::Error::custom)
    }
}

impl fmt::Debug for DatabaseSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DatabaseSettings")
            .field("endpoint", &self.endpoint)
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("namespace", &self.namespace)
            .field("database", &self.database)
            .field("connection_timeout", &self.connection_timeout)
            .field("engine", &self.engine)
            .finish()
    }
}

#[derive(Deserialize)]
struct RawDatabaseSettings {
    endpoint: String,
    username: String,
    password: String,
    namespace: String,
    database: String,
    connection_timeout_ms: u64,
    engine: DatabaseEngine,
}

impl TryFrom<RawDatabaseSettings> for DatabaseSettings {
    type Error = DatabaseSettingsError;

    fn try_from(raw: RawDatabaseSettings) -> Result<Self, Self::Error> {
        validate_not_empty("surrealdb.endpoint", &raw.endpoint)?;
        validate_not_empty("surrealdb.username", &raw.username)?;
        validate_not_empty("surrealdb.password", &raw.password)?;
        validate_not_empty("surrealdb.namespace", &raw.namespace)?;
        validate_not_empty("surrealdb.database", &raw.database)?;

        if raw.connection_timeout_ms == 0 {
            return Err(DatabaseSettingsError::ZeroTimeout);
        }

        let endpoint_supported = match raw.engine {
            DatabaseEngine::Websocket => websocket_endpoint_is_supported(&raw.endpoint),
            DatabaseEngine::Memory => matches!(raw.endpoint.as_str(), "memory" | "mem://"),
        };
        if !endpoint_supported {
            return Err(DatabaseSettingsError::UnsupportedEndpoint);
        }

        Ok(Self {
            endpoint: raw.endpoint,
            username: raw.username,
            password: raw.password,
            namespace: raw.namespace,
            database: raw.database,
            connection_timeout: Duration::from_millis(raw.connection_timeout_ms),
            engine: raw.engine,
        })
    }
}

fn validate_not_empty(setting: &'static str, value: &str) -> Result<(), DatabaseSettingsError> {
    if value.trim().is_empty() {
        return Err(DatabaseSettingsError::Empty { setting });
    }
    Ok(())
}

fn websocket_endpoint_is_supported(endpoint: &str) -> bool {
    ["ws://", "wss://"].iter().any(|scheme| {
        endpoint
            .strip_prefix(scheme)
            .is_some_and(|authority| !authority.is_empty() && !authority.starts_with('/'))
    })
}

fn safe_deserialization_error(error: &serde_json::Error) -> String {
    let error = error.to_string();
    error
        .strip_prefix("missing field `")
        .and_then(|rest| rest.split_once('`'))
        .map_or_else(
            || "one or more settings have an invalid type or value".to_owned(),
            |(field, _)| format!("missing required setting `surrealdb.{field}`"),
        )
}

/// A safe configuration error which never contains a configured value.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum DatabaseSettingsError {
    /// The application has no `settings.surrealdb` object.
    #[error("missing required setting section `settings.surrealdb`")]
    MissingSection,
    /// A required string is empty.
    #[error("setting `{setting}` must not be empty")]
    Empty { setting: &'static str },
    /// The endpoint cannot be used with the selected engine.
    #[error("setting `surrealdb.endpoint` is unsupported for the selected engine")]
    UnsupportedEndpoint,
    /// A zero timeout would disable startup protection.
    #[error("setting `surrealdb.connection_timeout_ms` must be greater than zero")]
    ZeroTimeout,
    /// The settings object could not be deserialized.
    #[error("invalid `settings.surrealdb` configuration: {message}")]
    InvalidFormat { message: String },
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{DatabaseEngine, DatabaseSettings, DatabaseSettingsError};

    fn valid_settings() -> serde_json::Value {
        json!({
            "endpoint": "ws://localhost:8000",
            "username": "root",
            "password": "top-secret",
            "namespace": "pipauto",
            "database": "pipauto_test",
            "connection_timeout_ms": 5_000,
            "engine": "websocket"
        })
    }

    #[test]
    fn accepts_valid_websocket_settings() {
        let settings: DatabaseSettings =
            serde_json::from_value(valid_settings()).expect("settings should be valid");

        assert_eq!(settings.engine(), DatabaseEngine::Websocket);
        assert_eq!(settings.connection_timeout().as_millis(), 5_000);
    }

    #[test]
    fn accepts_valid_memory_settings() {
        let mut value = valid_settings();
        value["endpoint"] = json!("memory");
        value["engine"] = json!("memory");

        let settings: DatabaseSettings =
            serde_json::from_value(value).expect("memory settings should be valid");

        assert_eq!(settings.engine(), DatabaseEngine::Memory);
    }

    #[test]
    fn rejects_each_empty_required_value() {
        for setting in ["endpoint", "username", "password", "namespace", "database"] {
            let mut value = valid_settings();
            value[setting] = json!("  ");

            let error = serde_json::from_value::<DatabaseSettings>(value)
                .expect_err("empty setting should be rejected");

            assert!(error.to_string().contains(setting));
            assert!(!error.to_string().contains("top-secret"));
        }
    }

    #[test]
    fn rejects_unsupported_endpoint_for_each_engine() {
        for (endpoint, engine) in [
            ("http://localhost:8000", "websocket"),
            ("ws://db", "memory"),
        ] {
            let mut value = valid_settings();
            value["endpoint"] = json!(endpoint);
            value["engine"] = json!(engine);

            let error = serde_json::from_value::<DatabaseSettings>(value)
                .expect_err("unsupported endpoint should be rejected");

            assert!(error.to_string().contains("surrealdb.endpoint"));
        }
    }

    #[test]
    fn rejects_zero_timeout() {
        let mut value = valid_settings();
        value["connection_timeout_ms"] = json!(0);

        let error = serde_json::from_value::<DatabaseSettings>(value)
            .expect_err("zero timeout should be rejected");

        assert!(error.to_string().contains("connection_timeout_ms"));
    }

    #[test]
    fn rejects_incomplete_configuration_without_revealing_password() {
        let mut value = valid_settings();
        value.as_object_mut().expect("object").remove("namespace");

        let error = serde_json::from_value::<DatabaseSettings>(value)
            .expect_err("incomplete settings should be rejected");

        assert!(error.to_string().contains("namespace"));
        assert!(!error.to_string().contains("top-secret"));
    }

    #[test]
    fn debug_output_redacts_password() {
        let settings: DatabaseSettings =
            serde_json::from_value(valid_settings()).expect("settings should be valid");
        let output = format!("{settings:?}");

        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("top-secret"));
    }

    #[test]
    fn malformed_password_value_is_never_echoed() {
        let mut value = valid_settings();
        value["password"] = json!(987_654_321);

        let error = serde_json::from_value::<DatabaseSettings>(value)
            .expect_err("non-string password should be rejected");

        assert!(!error.to_string().contains("987654321"));
        assert!(error.to_string().contains("invalid type or value"));
    }

    #[test]
    fn settings_errors_do_not_contain_configured_values() {
        let error = DatabaseSettingsError::UnsupportedEndpoint;
        assert_eq!(
            error.to_string(),
            "setting `surrealdb.endpoint` is unsupported for the selected engine"
        );
    }
}
