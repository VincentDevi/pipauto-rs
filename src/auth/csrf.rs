//! HMAC-signed pre-authentication and session-bound CSRF tokens.

use std::fmt;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;

use super::settings::AuthSettings;

type HmacSha256 = Hmac<Sha256>;

/// Signed CSRF token service sharing the validated application settings.
#[derive(Clone)]
pub struct CsrfService {
    settings: AuthSettings,
}

impl CsrfService {
    /// Construct a CSRF service.
    #[must_use]
    pub fn new(settings: AuthSettings) -> Self {
        Self { settings }
    }

    /// Create a random nonce cookie value and a login-action token bound to it.
    ///
    /// # Errors
    ///
    /// Returns a safe error if operating-system randomness is unavailable.
    pub fn issue_login(&self, now: DateTime<Utc>) -> Result<LoginCsrfState, CsrfError> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| CsrfError::Unavailable)?;
        let nonce = URL_SAFE_NO_PAD.encode(bytes);
        let binding = self.binding("login-nonce", &nonce)?;
        let expires_at = now
            + chrono::TimeDelta::from_std(self.settings.login_csrf_lifetime())
                .map_err(|_| CsrfError::Unavailable)?;
        let token = self.sign(TokenPayload {
            version: 1,
            action: "login".to_owned(),
            binding,
            origin: self
                .settings
                .canonical_origin()
                .origin()
                .ascii_serialization(),
            expires_at: expires_at.timestamp(),
        })?;
        Ok(LoginCsrfState {
            nonce,
            token,
            expires_at,
        })
    }

    /// Create a token bound to one authenticated session's raw JWT identifier.
    ///
    /// # Errors
    ///
    /// Returns a safe error if signing fails.
    pub fn issue_authenticated(
        &self,
        jti: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<SecretCsrfToken, CsrfError> {
        let payload = TokenPayload {
            version: 1,
            action: "unsafe".to_owned(),
            binding: self.binding("session", jti)?,
            origin: self
                .settings
                .canonical_origin()
                .origin()
                .ascii_serialization(),
            expires_at: expires_at.timestamp(),
        };
        self.sign(payload).map(SecretCsrfToken)
    }

    /// Validate a login token against its HttpOnly nonce cookie and request origin.
    pub fn validate_login(
        &self,
        token: &str,
        nonce: &str,
        request_origin: &str,
        now: DateTime<Utc>,
    ) -> Result<(), CsrfError> {
        self.validate(
            token,
            "login",
            &self.binding("login-nonce", nonce)?,
            request_origin,
            now,
        )
    }

    /// Validate an authenticated unsafe-request token.
    pub fn validate_authenticated(
        &self,
        token: &str,
        jti: &str,
        request_origin: &str,
        now: DateTime<Utc>,
    ) -> Result<(), CsrfError> {
        self.validate(
            token,
            "unsafe",
            &self.binding("session", jti)?,
            request_origin,
            now,
        )
    }

    fn validate(
        &self,
        token: &str,
        action: &str,
        binding: &str,
        request_origin: &str,
        now: DateTime<Utc>,
    ) -> Result<(), CsrfError> {
        if request_origin
            != self
                .settings
                .canonical_origin()
                .origin()
                .ascii_serialization()
        {
            return Err(CsrfError::Invalid);
        }
        let (payload, signature) = token.split_once('.').ok_or(CsrfError::Invalid)?;
        let signature = URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|_| CsrfError::Invalid)?;
        let mut mac = HmacSha256::new_from_slice(self.settings.csrf_secret().as_bytes())
            .map_err(|_| CsrfError::Unavailable)?;
        mac.update(payload.as_bytes());
        mac.verify_slice(&signature)
            .map_err(|_| CsrfError::Invalid)?;
        let payload = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| CsrfError::Invalid)?;
        let payload: TokenPayload =
            serde_json::from_slice(&payload).map_err(|_| CsrfError::Invalid)?;
        let valid = payload.version == 1
            && payload.action == action
            && payload.binding == binding
            && payload.origin == request_origin
            && payload.expires_at >= now.timestamp();
        if valid {
            Ok(())
        } else {
            Err(CsrfError::Invalid)
        }
    }

    fn sign(&self, payload: TokenPayload) -> Result<String, CsrfError> {
        let payload = serde_json::to_vec(&payload).map_err(|_| CsrfError::Unavailable)?;
        let payload = URL_SAFE_NO_PAD.encode(payload);
        let mut mac = HmacSha256::new_from_slice(self.settings.csrf_secret().as_bytes())
            .map_err(|_| CsrfError::Unavailable)?;
        mac.update(payload.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        Ok(format!("{payload}.{signature}"))
    }

    fn binding(&self, domain: &str, value: &str) -> Result<String, CsrfError> {
        let mut mac = HmacSha256::new_from_slice(self.settings.csrf_secret().as_bytes())
            .map_err(|_| CsrfError::Unavailable)?;
        mac.update(domain.as_bytes());
        mac.update(&[0]);
        mac.update(value.as_bytes());
        Ok(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
    }
}

#[derive(Serialize, Deserialize)]
struct TokenPayload {
    version: u8,
    action: String,
    binding: String,
    origin: String,
    expires_at: i64,
}

/// Fresh pre-authentication state. Debug output hides both secret values.
pub struct LoginCsrfState {
    pub(crate) nonce: String,
    pub(crate) token: String,
    pub(crate) expires_at: DateTime<Utc>,
}

impl fmt::Debug for LoginCsrfState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoginCsrfState")
            .field("nonce", &"[REDACTED]")
            .field("token", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// HTML-renderable token whose debug output is redacted.
#[derive(Clone)]
pub struct SecretCsrfToken(String);

impl SecretCsrfToken {
    /// Explicitly expose the token for a hidden field or same-origin HTMX header.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretCsrfToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretCsrfToken([REDACTED])")
    }
}

/// Safe CSRF operation failure.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CsrfError {
    #[error("invalid CSRF state")]
    Invalid,
    #[error("CSRF service unavailable")]
    Unavailable,
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;
    use loco_rs::environment::Environment;

    use super::*;

    fn service() -> CsrfService {
        CsrfService::new(
            AuthSettings::from_environment(&Environment::Test)
                .expect("test settings should be valid"),
        )
    }

    #[test]
    fn login_csrf_is_nonce_origin_action_and_expiry_bound() {
        let service = service();
        let now = Utc::now();
        let state = service.issue_login(now).expect("token should issue");
        assert_eq!(
            service.validate_login(&state.token, &state.nonce, "http://localhost:5150", now),
            Ok(())
        );
        assert_eq!(
            service.validate_login(&state.token, "wrong", "http://localhost:5150", now),
            Err(CsrfError::Invalid)
        );
        assert_eq!(
            service.validate_login(&state.token, &state.nonce, "https://attacker.example", now),
            Err(CsrfError::Invalid)
        );
        assert_eq!(
            service.validate_login(
                &state.token,
                &state.nonce,
                "http://localhost:5150",
                state.expires_at + TimeDelta::seconds(1),
            ),
            Err(CsrfError::Invalid)
        );
    }

    #[test]
    fn authenticated_csrf_rejects_tampering_and_wrong_session() {
        let service = service();
        let now = Utc::now();
        let token = service
            .issue_authenticated("session-one", now + TimeDelta::minutes(5))
            .expect("token should issue");
        assert_eq!(
            service.validate_authenticated(
                token.expose(),
                "session-one",
                "http://localhost:5150",
                now,
            ),
            Ok(())
        );
        assert_eq!(
            service.validate_authenticated(
                token.expose(),
                "session-two",
                "http://localhost:5150",
                now,
            ),
            Err(CsrfError::Invalid)
        );
        let mut tampered = token.expose().to_owned();
        tampered.push('x');
        assert_eq!(
            service.validate_authenticated(&tampered, "session-one", "http://localhost:5150", now,),
            Err(CsrfError::Invalid)
        );
    }
}
