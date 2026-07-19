//! Validated, secret-redacting authentication settings.

use std::{env, fmt, time::Duration};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use loco_rs::environment::Environment;
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use url::Url;

const JWT_SECRET_ENV: &str = "PIPAUTO_JWT_SECRET";
const CSRF_SECRET_ENV: &str = "PIPAUTO_CSRF_SECRET";
const ORIGIN_ENV: &str = "PIPAUTO_CANONICAL_ORIGIN";
const SESSION_SECONDS_ENV: &str = "PIPAUTO_SESSION_LIFETIME_SECONDS";
const DEFAULT_SESSION_SECONDS: u64 = 12 * 60 * 60;
const LOGIN_WINDOW_SECONDS: u64 = 15 * 60;
const LOGIN_BLOCK_SECONDS: u64 = 15 * 60;
const LOGIN_CSRF_SECONDS: u64 = 10 * 60;
const MIN_SECRET_BYTES: usize = 32;
const EXAMPLE_SECRET: &str = "replace-with-generated-base64";

/// Validated authentication settings installed once during application startup.
#[derive(Clone)]
pub struct AuthSettings {
    jwt_secret: SecretString,
    csrf_secret: SecretString,
    canonical_origin: Url,
    session_lifetime: Duration,
    login_window: Duration,
    login_block_duration: Duration,
    login_csrf_lifetime: Duration,
    maximum_login_attempts: u32,
    session_cookie_name: String,
    login_csrf_cookie_name: String,
    secure_cookies: bool,
}

impl AuthSettings {
    /// Load settings from process environment and validate them for the Loco environment.
    ///
    /// Test processes receive isolated non-production key material so request tests do not need
    /// global environment mutation. Development and production always require explicit values.
    ///
    /// # Errors
    ///
    /// Returns a secret-free error naming only the invalid setting.
    pub fn from_environment(environment: &Environment) -> Result<Self, AuthSettingsError> {
        if environment == &Environment::Test {
            return Self::from_values(
                environment,
                STANDARD.encode([0x51; MIN_SECRET_BYTES]),
                STANDARD.encode([0xA7; MIN_SECRET_BYTES]),
                "http://localhost:5150".to_owned(),
                DEFAULT_SESSION_SECONDS,
            );
        }

        let jwt_secret = required_env(JWT_SECRET_ENV)?;
        let csrf_secret = required_env(CSRF_SECRET_ENV)?;
        let canonical_origin = required_env(ORIGIN_ENV)?;
        let session_seconds = optional_u64_env(SESSION_SECONDS_ENV, DEFAULT_SESSION_SECONDS)?;
        Self::from_values(
            environment,
            jwt_secret,
            csrf_secret,
            canonical_origin,
            session_seconds,
        )
    }

    pub(super) fn from_values(
        environment: &Environment,
        jwt_secret: String,
        csrf_secret: String,
        canonical_origin: String,
        session_seconds: u64,
    ) -> Result<Self, AuthSettingsError> {
        validate_secret(JWT_SECRET_ENV, &jwt_secret)?;
        validate_secret(CSRF_SECRET_ENV, &csrf_secret)?;
        if jwt_secret == csrf_secret {
            return Err(AuthSettingsError::SecretsMustDiffer);
        }
        if session_seconds != DEFAULT_SESSION_SECONDS {
            return Err(AuthSettingsError::SessionLifetimeMustBeTwelveHours);
        }

        let canonical_origin =
            Url::parse(&canonical_origin).map_err(|_| AuthSettingsError::InvalidCanonicalOrigin)?;
        validate_origin(&canonical_origin, environment)?;

        let secure_cookies = environment == &Environment::Production;
        let (session_cookie_name, login_csrf_cookie_name) = if secure_cookies {
            (
                "__Host-pipauto_session".to_owned(),
                "__Host-pipauto_login_csrf".to_owned(),
            )
        } else {
            (
                "pipauto_session".to_owned(),
                "pipauto_login_csrf".to_owned(),
            )
        };

        Ok(Self {
            jwt_secret: SecretString::from(jwt_secret),
            csrf_secret: SecretString::from(csrf_secret),
            canonical_origin,
            session_lifetime: Duration::from_secs(session_seconds),
            login_window: Duration::from_secs(LOGIN_WINDOW_SECONDS),
            login_block_duration: Duration::from_secs(LOGIN_BLOCK_SECONDS),
            login_csrf_lifetime: Duration::from_secs(LOGIN_CSRF_SECONDS),
            maximum_login_attempts: 5,
            session_cookie_name,
            login_csrf_cookie_name,
            secure_cookies,
        })
    }

    /// Canonical origin used for strict Origin and Referer checks.
    #[must_use]
    pub fn canonical_origin(&self) -> &Url {
        &self.canonical_origin
    }

    /// Fixed session lifetime.
    #[must_use]
    pub fn session_lifetime(&self) -> Duration {
        self.session_lifetime
    }

    /// Login attempt accounting window.
    #[must_use]
    pub fn login_window(&self) -> Duration {
        self.login_window
    }

    /// Temporary login block duration.
    #[must_use]
    pub fn login_block_duration(&self) -> Duration {
        self.login_block_duration
    }

    /// Pre-authentication CSRF state lifetime.
    #[must_use]
    pub fn login_csrf_lifetime(&self) -> Duration {
        self.login_csrf_lifetime
    }

    /// Maximum failed attempts in one window.
    #[must_use]
    pub fn maximum_login_attempts(&self) -> u32 {
        self.maximum_login_attempts
    }

    /// Environment-selected browser session cookie name.
    #[must_use]
    pub fn session_cookie_name(&self) -> &str {
        &self.session_cookie_name
    }

    /// Environment-selected pre-authentication CSRF cookie name.
    #[must_use]
    pub fn login_csrf_cookie_name(&self) -> &str {
        &self.login_csrf_cookie_name
    }

    /// Whether cookies must carry the Secure attribute.
    #[must_use]
    pub fn secure_cookies(&self) -> bool {
        self.secure_cookies
    }

    pub(crate) fn jwt_secret(&self) -> &str {
        self.jwt_secret.expose_secret()
    }

    pub(crate) fn csrf_secret(&self) -> &str {
        self.csrf_secret.expose_secret()
    }
}

impl fmt::Debug for AuthSettings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthSettings")
            .field("jwt_secret", &"[REDACTED]")
            .field("csrf_secret", &"[REDACTED]")
            .field("canonical_origin", &self.canonical_origin)
            .field("session_lifetime", &self.session_lifetime)
            .field("login_window", &self.login_window)
            .field("login_block_duration", &self.login_block_duration)
            .field("login_csrf_lifetime", &self.login_csrf_lifetime)
            .field("maximum_login_attempts", &self.maximum_login_attempts)
            .field("session_cookie_name", &self.session_cookie_name)
            .field("login_csrf_cookie_name", &self.login_csrf_cookie_name)
            .field("secure_cookies", &self.secure_cookies)
            .finish()
    }
}

/// Secret-free authentication configuration error.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum AuthSettingsError {
    /// A required setting is absent.
    #[error("required authentication setting {setting} is missing")]
    Missing { setting: &'static str },
    /// A numeric setting is malformed.
    #[error("authentication setting {setting} must be a positive integer")]
    InvalidNumber { setting: &'static str },
    /// A secret is not valid base64 or is too short after decoding.
    #[error("authentication setting {setting} must be base64 for at least 32 random bytes")]
    InvalidSecret { setting: &'static str },
    /// Committed example material was supplied as a real secret.
    #[error("authentication setting {setting} still contains the example value")]
    ExampleSecret { setting: &'static str },
    /// JWT and CSRF keys must have different material.
    #[error("JWT and CSRF secrets must differ")]
    SecretsMustDiffer,
    /// Browser sessions have one fixed, approved lifetime.
    #[error("session lifetime must be exactly 12 hours")]
    SessionLifetimeMustBeTwelveHours,
    /// The canonical origin is malformed or contains non-origin components.
    #[error("canonical origin must contain only scheme and authority")]
    InvalidCanonicalOrigin,
    /// Production must use an HTTPS origin.
    #[error("production canonical origin must use HTTPS")]
    InsecureProductionOrigin,
}

fn required_env(setting: &'static str) -> Result<String, AuthSettingsError> {
    env::var(setting)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or(AuthSettingsError::Missing { setting })
}

fn optional_u64_env(setting: &'static str, default: u64) -> Result<u64, AuthSettingsError> {
    match env::var(setting) {
        Ok(value) => value
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or(AuthSettingsError::InvalidNumber { setting }),
        Err(_) => Ok(default),
    }
}

fn validate_secret(setting: &'static str, value: &str) -> Result<(), AuthSettingsError> {
    if value == EXAMPLE_SECRET {
        return Err(AuthSettingsError::ExampleSecret { setting });
    }
    let decoded = STANDARD
        .decode(value)
        .map_err(|_| AuthSettingsError::InvalidSecret { setting })?;
    if decoded.len() < MIN_SECRET_BYTES {
        return Err(AuthSettingsError::InvalidSecret { setting });
    }
    Ok(())
}

fn validate_origin(origin: &Url, environment: &Environment) -> Result<(), AuthSettingsError> {
    let has_only_origin = origin.host_str().is_some()
        && origin.username().is_empty()
        && origin.password().is_none()
        && origin.path() == "/"
        && origin.query().is_none()
        && origin.fragment().is_none()
        && matches!(origin.scheme(), "http" | "https");
    if !has_only_origin {
        return Err(AuthSettingsError::InvalidCanonicalOrigin);
    }
    if environment == &Environment::Production && origin.scheme() != "https" {
        return Err(AuthSettingsError::InsecureProductionOrigin);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret(byte: u8) -> String {
        STANDARD.encode([byte; MIN_SECRET_BYTES])
    }

    #[test]
    fn auth_settings_accept_valid_development_and_redact_secrets() {
        let settings = AuthSettings::from_values(
            &Environment::Development,
            secret(1),
            secret(2),
            "http://localhost:5150".to_owned(),
            DEFAULT_SESSION_SECONDS,
        )
        .expect("settings should validate");

        let debug = format!("{settings:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains(&secret(1)));
        assert_eq!(settings.session_cookie_name(), "pipauto_session");
        assert!(!settings.secure_cookies());
    }

    #[test]
    fn auth_settings_require_https_and_host_cookies_in_production() {
        let settings = AuthSettings::from_values(
            &Environment::Production,
            secret(1),
            secret(2),
            "https://pipauto.example".to_owned(),
            DEFAULT_SESSION_SECONDS,
        )
        .expect("settings should validate");
        assert!(settings.secure_cookies());
        assert!(settings.session_cookie_name().starts_with("__Host-"));

        let error = AuthSettings::from_values(
            &Environment::Production,
            secret(1),
            secret(2),
            "http://pipauto.example".to_owned(),
            DEFAULT_SESSION_SECONDS,
        )
        .expect_err("HTTP production origin must fail");
        assert_eq!(error, AuthSettingsError::InsecureProductionOrigin);
    }

    #[test]
    fn auth_settings_reject_short_equal_and_non_origin_values() {
        assert_eq!(
            AuthSettings::from_values(
                &Environment::Development,
                STANDARD.encode([1; 8]),
                secret(2),
                "http://localhost:5150".to_owned(),
                DEFAULT_SESSION_SECONDS,
            )
            .expect_err("short key must fail"),
            AuthSettingsError::InvalidSecret {
                setting: JWT_SECRET_ENV
            }
        );
        assert_eq!(
            AuthSettings::from_values(
                &Environment::Development,
                secret(1),
                secret(1),
                "http://localhost:5150".to_owned(),
                DEFAULT_SESSION_SECONDS,
            )
            .expect_err("equal keys must fail"),
            AuthSettingsError::SecretsMustDiffer
        );
        assert_eq!(
            AuthSettings::from_values(
                &Environment::Development,
                secret(1),
                secret(2),
                "http://localhost:5150/path".to_owned(),
                DEFAULT_SESSION_SECONDS,
            )
            .expect_err("origin path must fail"),
            AuthSettingsError::InvalidCanonicalOrigin
        );
        assert_eq!(
            AuthSettings::from_values(
                &Environment::Development,
                secret(1),
                secret(2),
                "http://localhost:5150".to_owned(),
                DEFAULT_SESSION_SECONDS - 1,
            )
            .expect_err("non-standard session lifetime must fail"),
            AuthSettingsError::SessionLifetimeMustBeTwelveHours
        );
    }

    #[test]
    fn missing_required_authentication_settings_are_named_without_values() {
        const MISSING_TEST_ENV: &str = "PIPAUTO_AUTH_TEST_DELIBERATELY_MISSING";
        std::env::remove_var(MISSING_TEST_ENV);
        assert_eq!(
            required_env(MISSING_TEST_ENV),
            Err(AuthSettingsError::Missing {
                setting: MISSING_TEST_ENV
            })
        );
    }
}
