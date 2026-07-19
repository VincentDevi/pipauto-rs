//! Loco cryptography adapters and production time/randomness sources.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use loco_rs::auth::jwt::JWT;
use serde_json::{Map, Value};

use crate::{
    models::auth::UserId,
    services::auth::{
        AuthError, Clock, IssuedJwt, JwtCodec, PasswordEngine, RandomSource, ValidatedJwt,
    },
};

/// System UTC clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(Utc::now().timestamp(), 0)
            .expect("current UTC timestamp should be representable")
    }
}

/// Operating-system CSPRNG.
pub struct OsRandom;

impl RandomSource for OsRandom {
    fn session_identifier(&self) -> Result<String, AuthError> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| AuthError::Unavailable)?;
        Ok(URL_SAFE_NO_PAD.encode(bytes))
    }
}

/// Loco Argon2id password engine, offloaded from the async executor.
pub struct LocoPasswordEngine;

#[async_trait]
impl PasswordEngine for LocoPasswordEngine {
    async fn hash(&self, password: &str) -> Result<String, AuthError> {
        let password = password.to_owned();
        tokio::task::spawn_blocking(move || loco_rs::hash::hash_password(&password))
            .await
            .map_err(|_| AuthError::Unavailable)?
            .map_err(|_| AuthError::Unavailable)
    }

    async fn verify(&self, password: &str, password_hash: &str) -> Result<bool, AuthError> {
        let password = password.to_owned();
        let password_hash = password_hash.to_owned();
        tokio::task::spawn_blocking(move || {
            loco_rs::hash::verify_password(&password, &password_hash)
        })
        .await
        .map_err(|_| AuthError::Unavailable)
    }
}

/// Loco 0.16.4 JWT adapter. Debug output never exposes its signing key.
pub struct LocoJwtCodec {
    jwt: JWT,
}

impl LocoJwtCodec {
    /// Construct using the already-validated base64 signing key.
    #[must_use]
    pub fn new(secret: &str) -> Self {
        Self {
            jwt: JWT::new(secret),
        }
    }

    fn validated_claims(&self, encoded: &str) -> Result<ValidatedJwt, AuthError> {
        let token = self
            .jwt
            .validate(encoded)
            .map_err(|_| AuthError::Unauthenticated)?;
        let serialized = serde_json::to_value(&token.claims).map_err(|_| AuthError::Unavailable)?;
        let exp = serialized
            .get("exp")
            .and_then(Value::as_u64)
            .ok_or(AuthError::Unauthenticated)?;
        let jti = token
            .claims
            .claims
            .get("jti")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or(AuthError::Unauthenticated)?
            .to_owned();
        let expires_at = DateTime::from_timestamp(
            i64::try_from(exp).map_err(|_| AuthError::Unauthenticated)?,
            0,
        )
        .ok_or(AuthError::Unauthenticated)?;
        Ok(ValidatedJwt {
            pid: token.claims.pid,
            jti,
            expires_at,
        })
    }
}

impl JwtCodec for LocoJwtCodec {
    fn issue(
        &self,
        pid: &UserId,
        jti: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<IssuedJwt, AuthError> {
        let mut claims = Map::new();
        claims.insert("jti".to_owned(), Value::String(jti.to_owned()));
        for _attempt in 0..3 {
            let now = Utc::now().timestamp();
            let lifetime = expires_at
                .timestamp()
                .checked_sub(now)
                .and_then(|seconds| u64::try_from(seconds).ok())
                .filter(|seconds| *seconds > 0)
                .ok_or(AuthError::Unavailable)?;
            let encoded = self
                .jwt
                .generate_token(lifetime, pid.as_str().to_owned(), claims.clone())
                .map_err(|_| AuthError::Unavailable)?;
            let validated = self.validated_claims(&encoded)?;
            if validated.expires_at == expires_at {
                return Ok(IssuedJwt {
                    encoded,
                    claims: validated,
                });
            }
        }
        Err(AuthError::Unavailable)
    }

    fn validate(&self, encoded: &str) -> Result<ValidatedJwt, AuthError> {
        self.validated_claims(encoded)
    }
}

impl std::fmt::Debug for LocoJwtCodec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("LocoJwtCodec([REDACTED])")
    }
}

/// Create production adapter objects without exposing concrete types to services.
#[must_use]
pub fn adapters(secret: &str) -> (Arc<dyn Clock>, Arc<dyn RandomSource>, Arc<dyn JwtCodec>) {
    (
        Arc::new(SystemClock),
        Arc::new(OsRandom),
        Arc::new(LocoJwtCodec::new(secret)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn password_authentication_loco_argon2id_hash_round_trip() {
        let engine = LocoPasswordEngine;
        let hash = engine
            .hash("correct horse workshop staple")
            .await
            .expect("password should hash");

        assert!(hash.starts_with("$argon2id$"));
        assert!(engine
            .verify("correct horse workshop staple", &hash)
            .await
            .expect("valid hash should verify"));
        assert!(!engine
            .verify("wrong horse workshop staple", &hash)
            .await
            .expect("wrong password should not verify"));
    }
}
