//! Password login, revocable session validation, logout, and user administration workflows.

use std::{fmt, sync::Arc, time::Duration};

use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::time::timeout;

use crate::{
    auth::{
        csrf::{CsrfService, SecretCsrfToken},
        settings::AuthSettings,
    },
    models::auth::{
        validate_display_name, validate_password, AuthenticatedUser, NewAuthSession, NewUserRecord,
        NormalizedEmail, SessionDigest, ThrottleDigest, User, UserId,
    },
    repositories::auth::{
        AuthSessionRepository, LoginThrottleRepository, RepositoryError, RevokeOutcome,
        ThrottleState, UserRepository,
    },
};

type HmacSha256 = Hmac<Sha256>;
const AUTH_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);

/// Time source seam for deterministic authentication tests.
pub trait Clock: Send + Sync {
    /// Current UTC time.
    fn now(&self) -> DateTime<Utc>;
}

/// Secure random source seam.
pub trait RandomSource: Send + Sync {
    /// Create 32 random bytes encoded for a JWT string claim.
    fn session_identifier(&self) -> Result<String, AuthError>;
}

/// Password operations supplied by Loco in production.
#[async_trait]
pub trait PasswordEngine: Send + Sync {
    /// Produce an Argon2id PHC string.
    async fn hash(&self, password: &str) -> Result<String, AuthError>;
    /// Verify one password against one PHC string.
    async fn verify(&self, password: &str, password_hash: &str) -> Result<bool, AuthError>;
}

/// Validated JWT claims required by Pipauto.
#[derive(Clone, Debug)]
pub struct ValidatedJwt {
    /// Stable user record identifier.
    pub pid: String,
    /// Raw random session identifier carried only inside the signed JWT.
    pub jti: String,
    /// Exact expiry signed by Loco.
    pub expires_at: DateTime<Utc>,
}

/// JWT issuance result. Debug output never contains the encoded token or raw identifier.
pub struct IssuedJwt {
    pub(crate) encoded: String,
    pub(crate) claims: ValidatedJwt,
}

impl fmt::Debug for IssuedJwt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IssuedJwt")
            .field("encoded", &"[REDACTED]")
            .field("pid", &self.claims.pid)
            .field("jti", &"[REDACTED]")
            .field("expires_at", &self.claims.expires_at)
            .finish()
    }
}

/// Loco JWT adapter seam.
pub trait JwtCodec: Send + Sync {
    /// Issue and immediately validate a signed token with the pre-registered exact expiry.
    fn issue(
        &self,
        pid: &UserId,
        jti: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<IssuedJwt, AuthError>;
    /// Validate signature, expiry, and required claim shapes.
    fn validate(&self, encoded: &str) -> Result<ValidatedJwt, AuthError>;
}

/// Credential submission. Debug output omits the password.
pub struct LoginCommand {
    /// Submitted email.
    pub email: String,
    /// Submitted password, preserved byte-for-byte.
    pub password: String,
    /// Trusted socket address or a stable unknown sentinel.
    pub client_network: String,
}

impl fmt::Debug for LoginCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoginCommand")
            .field("email", &self.email)
            .field("password", &"[REDACTED]")
            .field("client_network", &self.client_network)
            .finish()
    }
}

/// Successful login result containing secrets only through explicit accessors.
pub struct LoginSuccess {
    encoded_jwt: String,
    csrf_token: SecretCsrfToken,
    expires_at: DateTime<Utc>,
    /// Presentation-safe user state.
    pub user: AuthenticatedUser,
}

impl LoginSuccess {
    /// Explicitly expose the encoded token for the Set-Cookie response only.
    #[must_use]
    pub fn encoded_jwt(&self) -> &str {
        &self.encoded_jwt
    }

    /// Explicitly expose the session-bound token for same-origin unsafe browser requests.
    #[must_use]
    pub fn csrf_token(&self) -> &SecretCsrfToken {
        &self.csrf_token
    }

    /// Exact browser-session expiry used by the registry and presentation-safe user.
    #[must_use]
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
}

impl fmt::Debug for LoginSuccess {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LoginSuccess")
            .field("encoded_jwt", &"[REDACTED]")
            .field("csrf_token", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .field("user", &self.user)
            .finish()
    }
}

/// Internal authenticated session including the raw JWT binding for CSRF validation.
pub struct AuthenticatedSession {
    /// Safe current user.
    pub user: AuthenticatedUser,
    pub(crate) jti: String,
}

/// Shared authentication workflow service.
#[derive(Clone)]
pub struct AuthService {
    settings: AuthSettings,
    users: Arc<dyn UserRepository>,
    sessions: Arc<dyn AuthSessionRepository>,
    throttles: Arc<dyn LoginThrottleRepository>,
    passwords: Arc<dyn PasswordEngine>,
    jwt: Arc<dyn JwtCodec>,
    clock: Arc<dyn Clock>,
    random: Arc<dyn RandomSource>,
    csrf: CsrfService,
    dummy_password_hash: String,
}

impl AuthService {
    /// Compose the authentication service from its validated boundaries.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        settings: AuthSettings,
        users: Arc<dyn UserRepository>,
        sessions: Arc<dyn AuthSessionRepository>,
        throttles: Arc<dyn LoginThrottleRepository>,
        passwords: Arc<dyn PasswordEngine>,
        jwt: Arc<dyn JwtCodec>,
        clock: Arc<dyn Clock>,
        random: Arc<dyn RandomSource>,
        dummy_password_hash: String,
    ) -> Self {
        let csrf = CsrfService::new(settings.clone());
        Self {
            settings,
            users,
            sessions,
            throttles,
            passwords,
            jwt,
            clock,
            random,
            csrf,
            dummy_password_hash,
        }
    }

    /// Authenticate credentials, register an exact-expiry session, and return a browser token.
    ///
    /// # Errors
    ///
    /// Returns generic credential/input failures, a temporary throttle, or a distinct outage.
    pub async fn login(&self, command: LoginCommand) -> Result<LoginSuccess, LoginError> {
        timeout(AUTH_OPERATION_TIMEOUT, self.login_inner(command))
            .await
            .unwrap_or(Err(LoginError::Unavailable))
    }

    async fn login_inner(&self, command: LoginCommand) -> Result<LoginSuccess, LoginError> {
        let email = NormalizedEmail::parse(&command.email).map_err(|_| LoginError::InvalidInput)?;
        if command.password.len() > crate::models::auth::PASSWORD_MAX_BYTES {
            return Err(LoginError::InvalidInput);
        }
        let now = self.clock.now();
        let identifier_digest = self.keyed_digest("login-identifier", email.as_str())?;
        let network_digest = self.keyed_digest("login-network", &command.client_network)?;
        if let ThrottleState::BlockedUntil(until) = self
            .throttles
            .state(&identifier_digest, &network_digest, now)
            .await
            .map_err(|_| LoginError::Unavailable)?
        {
            return Err(LoginError::Throttled { until });
        }

        let credentials = verify_user_credentials(
            self.users.as_ref(),
            self.passwords.as_ref(),
            &self.dummy_password_hash,
            &email,
            &command.password,
        )
        .await;
        if matches!(credentials, Err(LoginError::InvalidCredentials)) {
            let state = self
                .throttles
                .record_failure(
                    &identifier_digest,
                    &network_digest,
                    now,
                    self.settings.login_window(),
                    self.settings.maximum_login_attempts(),
                    self.settings.login_block_duration(),
                )
                .await
                .map_err(|_| LoginError::Unavailable)?;
            return match state {
                ThrottleState::Allowed => Err(LoginError::InvalidCredentials),
                ThrottleState::BlockedUntil(until) => Err(LoginError::Throttled { until }),
            };
        }
        if let Err(error) = credentials {
            return Err(error);
        }

        self.throttles
            .clear(&identifier_digest, &network_digest)
            .await
            .map_err(|_| LoginError::Unavailable)?;
        let credentials = credentials?;
        let jti = self.random.session_identifier().map_err(LoginError::from)?;
        let digest = digest_jti(&jti).map_err(LoginError::from)?;
        let issued_at = now;
        let expires_at = issued_at
            + chrono::TimeDelta::from_std(self.settings.session_lifetime())
                .map_err(|_| LoginError::Unavailable)?;
        self.sessions
            .create(NewAuthSession {
                user_id: credentials.user.id.clone(),
                jti_digest: digest.clone(),
                issued_at,
                expires_at,
                created_ip_digest: None,
                user_agent_summary: None,
            })
            .await
            .map_err(|_| LoginError::Unavailable)?;

        let issued = match self.jwt.issue(&credentials.user.id, &jti, expires_at) {
            Ok(issued)
                if issued.claims.pid == credentials.user.id.as_str()
                    && issued.claims.jti == jti
                    && issued.claims.expires_at == expires_at =>
            {
                issued
            }
            Ok(_) | Err(_) => {
                self.revoke_failed_login(&digest).await;
                return Err(LoginError::Unavailable);
            }
        };
        let csrf_token = match self.csrf.issue_authenticated(&jti, expires_at) {
            Ok(token) => token,
            Err(_) => {
                self.revoke_failed_login(&digest).await;
                return Err(LoginError::Unavailable);
            }
        };
        Ok(LoginSuccess {
            encoded_jwt: issued.encoded,
            csrf_token,
            expires_at,
            user: authenticated_user(credentials.user, expires_at),
        })
    }

    /// Validate JWT, registry session, matching active user, and fixed expiry.
    pub async fn authenticate(&self, encoded_jwt: &str) -> Result<AuthenticatedUser, AuthError> {
        self.authenticate_session(encoded_jwt)
            .await
            .map(|value| value.user)
    }

    /// Authenticate while retaining the raw JWT identifier for CSRF binding.
    pub async fn authenticate_session(
        &self,
        encoded_jwt: &str,
    ) -> Result<AuthenticatedSession, AuthError> {
        timeout(
            AUTH_OPERATION_TIMEOUT,
            self.authenticate_session_inner(encoded_jwt),
        )
        .await
        .unwrap_or(Err(AuthError::Unavailable))
    }

    async fn authenticate_session_inner(
        &self,
        encoded_jwt: &str,
    ) -> Result<AuthenticatedSession, AuthError> {
        let claims = self.jwt.validate(encoded_jwt)?;
        let user_id = UserId::parse(claims.pid).map_err(|_| AuthError::Unauthenticated)?;
        let digest = digest_jti(&claims.jti)?;
        let session = self
            .sessions
            .find_active(&digest, self.clock.now())
            .await
            .map_err(map_repository_error)?
            .ok_or(AuthError::Unauthenticated)?;
        if session.user_id != user_id || session.expires_at != claims.expires_at {
            return Err(AuthError::Unauthenticated);
        }
        let user = self
            .users
            .find_by_id(&user_id)
            .await
            .map_err(map_repository_error)?
            .filter(|user| user.active)
            .ok_or(AuthError::Unauthenticated)?;
        Ok(AuthenticatedSession {
            user: authenticated_user(user, claims.expires_at),
            jti: claims.jti,
        })
    }

    /// Revoke a valid registry session; absent or invalid credentials are idempotent success.
    pub async fn logout(&self, encoded_jwt: Option<&str>) -> Result<RevokeOutcome, AuthError> {
        timeout(AUTH_OPERATION_TIMEOUT, self.logout_inner(encoded_jwt))
            .await
            .unwrap_or(Err(AuthError::Unavailable))
    }

    async fn logout_inner(&self, encoded_jwt: Option<&str>) -> Result<RevokeOutcome, AuthError> {
        let Some(encoded_jwt) = encoded_jwt else {
            return Ok(RevokeOutcome::AlreadyInactive);
        };
        let claims = match self.jwt.validate(encoded_jwt) {
            Ok(claims) => claims,
            Err(AuthError::Unauthenticated) => return Ok(RevokeOutcome::AlreadyInactive),
            Err(error) => return Err(error),
        };
        let digest = match digest_jti(&claims.jti) {
            Ok(digest) => digest,
            Err(AuthError::Unauthenticated) => return Ok(RevokeOutcome::AlreadyInactive),
            Err(error) => return Err(error),
        };
        self.sessions
            .revoke(&digest, self.clock.now())
            .await
            .map_err(map_repository_error)
    }

    /// Validate, hash, and persist a user without exposing a registration route.
    pub async fn create_user(
        &self,
        email: &str,
        display_name: &str,
        password: &str,
    ) -> Result<User, CreateUserError> {
        let normalized =
            NormalizedEmail::parse(email).map_err(|_| CreateUserError::InvalidInput)?;
        let display_name =
            validate_display_name(display_name).map_err(|_| CreateUserError::InvalidInput)?;
        validate_password(password, &normalized).map_err(|_| CreateUserError::InvalidInput)?;
        let password_hash = self
            .passwords
            .hash(password)
            .await
            .map_err(|_| CreateUserError::Unavailable)?;
        self.users
            .create(
                NewUserRecord::new(email.to_owned(), normalized, display_name, password_hash)
                    .map_err(|_| CreateUserError::InvalidInput)?,
            )
            .await
            .map_err(|error| match error {
                RepositoryError::Conflict => CreateUserError::Duplicate,
                RepositoryError::Unavailable => CreateUserError::Unavailable,
            })
    }

    /// Remove only expired registry sessions.
    pub async fn purge_expired_sessions(&self) -> Result<u64, AuthError> {
        self.sessions
            .delete_expired(self.clock.now())
            .await
            .map_err(map_repository_error)
    }

    fn keyed_digest(&self, domain: &str, value: &str) -> Result<ThrottleDigest, LoginError> {
        let mut mac = HmacSha256::new_from_slice(self.settings.csrf_secret().as_bytes())
            .map_err(|_| LoginError::Unavailable)?;
        mac.update(domain.as_bytes());
        mac.update(&[0]);
        mac.update(value.as_bytes());
        ThrottleDigest::parse(URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()))
            .map_err(|_| LoginError::Unavailable)
    }

    async fn revoke_failed_login(&self, digest: &SessionDigest) {
        let _result = self.sessions.revoke(digest, self.clock.now()).await;
    }
}

async fn verify_user_credentials(
    users: &dyn UserRepository,
    passwords: &dyn PasswordEngine,
    dummy_password_hash: &str,
    email: &NormalizedEmail,
    password: &str,
) -> Result<crate::models::auth::UserCredentials, LoginError> {
    let credentials = users
        .find_credentials_by_email(email)
        .await
        .map_err(|_| LoginError::Unavailable)?;
    let password_hash = credentials
        .as_ref()
        .map_or(dummy_password_hash, |value| value.password_hash());
    let verified = passwords
        .verify(password, password_hash)
        .await
        .map_err(|_| LoginError::Unavailable)?;
    credentials
        .filter(|value| verified && value.user.active)
        .ok_or(LoginError::InvalidCredentials)
}

/// Authentication-layer failure that distinguishes invalid identity from infrastructure outage.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AuthError {
    #[error("authentication required")]
    Unauthenticated,
    #[error("authentication unavailable")]
    Unavailable,
}

/// Public login failure categories.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum LoginError {
    #[error("invalid login input")]
    InvalidInput,
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("login temporarily throttled")]
    Throttled { until: DateTime<Utc> },
    #[error("authentication unavailable")]
    Unavailable,
}

impl From<AuthError> for LoginError {
    fn from(value: AuthError) -> Self {
        match value {
            AuthError::Unauthenticated | AuthError::Unavailable => Self::Unavailable,
        }
    }
}

/// First-user administration failure.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum CreateUserError {
    #[error("invalid user input")]
    InvalidInput,
    #[error("email already exists")]
    Duplicate,
    #[error("user creation unavailable")]
    Unavailable,
}

fn authenticated_user(user: User, expires_at: DateTime<Utc>) -> AuthenticatedUser {
    AuthenticatedUser {
        id: user.id,
        email: user.email,
        display_name: user.display_name,
        session_expires_at: expires_at,
    }
}

fn digest_jti(jti: &str) -> Result<SessionDigest, AuthError> {
    let decoded = URL_SAFE_NO_PAD
        .decode(jti)
        .map_err(|_| AuthError::Unauthenticated)?;
    if jti.len() != 43 || decoded.len() != 32 {
        return Err(AuthError::Unauthenticated);
    }
    let bytes = Sha256::digest(jti.as_bytes());
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").map_err(|_| AuthError::Unavailable)?;
    }
    SessionDigest::parse(encoded).map_err(|_| AuthError::Unauthenticated)
}

fn map_repository_error(_error: RepositoryError) -> AuthError {
    AuthError::Unavailable
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use loco_rs::environment::Environment;

    use super::*;
    use crate::models::auth::{AuthSession, UserCredentials};

    struct CredentialRepository {
        credentials: Option<UserCredentials>,
    }

    #[async_trait]
    impl UserRepository for CredentialRepository {
        async fn find_by_id(&self, _id: &UserId) -> Result<Option<User>, RepositoryError> {
            Ok(None)
        }

        async fn find_credentials_by_email(
            &self,
            _normalized_email: &NormalizedEmail,
        ) -> Result<Option<UserCredentials>, RepositoryError> {
            Ok(self.credentials.clone())
        }

        async fn create(&self, _new_user: NewUserRecord) -> Result<User, RepositoryError> {
            Err(RepositoryError::Unavailable)
        }
    }

    struct RecordingPasswords {
        expected_password: String,
        verified_hashes: Mutex<Vec<String>>,
    }

    struct FixedClock(DateTime<Utc>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    struct FixedRandom;

    impl RandomSource for FixedRandom {
        fn session_identifier(&self) -> Result<String, AuthError> {
            Ok(URL_SAFE_NO_PAD.encode([7_u8; 32]))
        }
    }

    struct FailingJwt {
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    impl JwtCodec for FailingJwt {
        fn issue(
            &self,
            _pid: &UserId,
            _jti: &str,
            _expires_at: DateTime<Utc>,
        ) -> Result<IssuedJwt, AuthError> {
            self.events
                .lock()
                .expect("event record should lock")
                .push("jwt:issue");
            Err(AuthError::Unavailable)
        }

        fn validate(&self, _encoded: &str) -> Result<ValidatedJwt, AuthError> {
            Err(AuthError::Unauthenticated)
        }
    }

    struct WorkflowRepository {
        credentials: UserCredentials,
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    #[async_trait]
    impl UserRepository for WorkflowRepository {
        async fn find_by_id(&self, _id: &UserId) -> Result<Option<User>, RepositoryError> {
            Ok(Some(self.credentials.user.clone()))
        }

        async fn find_credentials_by_email(
            &self,
            _normalized_email: &NormalizedEmail,
        ) -> Result<Option<UserCredentials>, RepositoryError> {
            Ok(Some(self.credentials.clone()))
        }

        async fn create(&self, _new_user: NewUserRecord) -> Result<User, RepositoryError> {
            Err(RepositoryError::Unavailable)
        }
    }

    #[async_trait]
    impl AuthSessionRepository for WorkflowRepository {
        async fn create(&self, session: NewAuthSession) -> Result<AuthSession, RepositoryError> {
            self.events
                .lock()
                .expect("event record should lock")
                .push("session:create");
            Ok(AuthSession {
                user_id: session.user_id,
                jti_digest: session.jti_digest,
                issued_at: session.issued_at,
                expires_at: session.expires_at,
                revoked_at: None,
                last_seen_at: None,
                created_ip_digest: session.created_ip_digest,
                user_agent_summary: session.user_agent_summary,
            })
        }

        async fn find_active(
            &self,
            _jti_digest: &SessionDigest,
            _now: DateTime<Utc>,
        ) -> Result<Option<AuthSession>, RepositoryError> {
            Ok(None)
        }

        async fn revoke(
            &self,
            _jti_digest: &SessionDigest,
            _now: DateTime<Utc>,
        ) -> Result<RevokeOutcome, RepositoryError> {
            self.events
                .lock()
                .expect("event record should lock")
                .push("session:revoke");
            Ok(RevokeOutcome::Revoked)
        }

        async fn revoke_all_for_user(
            &self,
            _user_id: &UserId,
            _now: DateTime<Utc>,
        ) -> Result<u64, RepositoryError> {
            Ok(0)
        }

        async fn delete_expired(&self, _now: DateTime<Utc>) -> Result<u64, RepositoryError> {
            Ok(0)
        }
    }

    #[async_trait]
    impl LoginThrottleRepository for WorkflowRepository {
        async fn state(
            &self,
            _identifier_digest: &ThrottleDigest,
            _network_digest: &ThrottleDigest,
            _now: DateTime<Utc>,
        ) -> Result<ThrottleState, RepositoryError> {
            Ok(ThrottleState::Allowed)
        }

        async fn record_failure(
            &self,
            _identifier_digest: &ThrottleDigest,
            _network_digest: &ThrottleDigest,
            _now: DateTime<Utc>,
            _window: Duration,
            _maximum_attempts: u32,
            _block_duration: Duration,
        ) -> Result<ThrottleState, RepositoryError> {
            Ok(ThrottleState::Allowed)
        }

        async fn clear(
            &self,
            _identifier_digest: &ThrottleDigest,
            _network_digest: &ThrottleDigest,
        ) -> Result<(), RepositoryError> {
            Ok(())
        }
    }

    #[async_trait]
    impl PasswordEngine for RecordingPasswords {
        async fn hash(&self, _password: &str) -> Result<String, AuthError> {
            Err(AuthError::Unavailable)
        }

        async fn verify(&self, password: &str, password_hash: &str) -> Result<bool, AuthError> {
            self.verified_hashes
                .lock()
                .expect("verification record should lock")
                .push(password_hash.to_owned());
            Ok(password == self.expected_password && password_hash == "$argon2id$known")
        }
    }

    fn credentials(active: bool) -> UserCredentials {
        let now = Utc::now();
        UserCredentials::new(
            User {
                id: UserId::parse("user:test").expect("test id should parse"),
                email: "filippo@example.com".to_owned(),
                display_name: "Filippo".to_owned(),
                active,
                created_at: now,
                updated_at: now,
            },
            "$argon2id$known".to_owned(),
        )
    }

    #[tokio::test]
    async fn password_authentication_unknown_user_runs_one_dummy_verification() {
        let repository = CredentialRepository { credentials: None };
        let passwords = RecordingPasswords {
            expected_password: "correct workshop password".to_owned(),
            verified_hashes: Mutex::new(Vec::new()),
        };
        let email = NormalizedEmail::parse("filippo@example.com").expect("email should parse");

        let result = verify_user_credentials(
            &repository,
            &passwords,
            "$argon2id$dummy",
            &email,
            "wrong workshop password",
        )
        .await;

        assert!(matches!(result, Err(LoginError::InvalidCredentials)));
        assert_eq!(
            *passwords
                .verified_hashes
                .lock()
                .expect("verification record should lock"),
            vec!["$argon2id$dummy"]
        );
    }

    #[tokio::test]
    async fn password_authentication_wrong_and_inactive_users_share_public_failure() {
        let email = NormalizedEmail::parse("filippo@example.com").expect("email should parse");
        for (active, password) in [
            (true, "wrong workshop password"),
            (false, "correct workshop password"),
        ] {
            let repository = CredentialRepository {
                credentials: Some(credentials(active)),
            };
            let passwords = RecordingPasswords {
                expected_password: "correct workshop password".to_owned(),
                verified_hashes: Mutex::new(Vec::new()),
            };
            assert!(matches!(
                verify_user_credentials(
                    &repository,
                    &passwords,
                    "$argon2id$dummy",
                    &email,
                    password,
                )
                .await,
                Err(LoginError::InvalidCredentials)
            ));
        }
    }

    #[tokio::test]
    async fn auth_service_login_persists_before_jwt_and_revokes_if_issuance_fails() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let repository = Arc::new(WorkflowRepository {
            credentials: credentials(true),
            events: events.clone(),
        });
        let now =
            DateTime::from_timestamp(1_800_000_000, 0).expect("fixture timestamp should be valid");
        let settings = AuthSettings::from_environment(&Environment::Test)
            .expect("test settings should be valid");
        let service = AuthService::new(
            settings,
            repository.clone(),
            repository.clone(),
            repository,
            Arc::new(RecordingPasswords {
                expected_password: "correct workshop password".to_owned(),
                verified_hashes: Mutex::new(Vec::new()),
            }),
            Arc::new(FailingJwt {
                events: events.clone(),
            }),
            Arc::new(FixedClock(now)),
            Arc::new(FixedRandom),
            "$argon2id$dummy".to_owned(),
        );

        assert!(matches!(
            service
                .login(LoginCommand {
                    email: "filippo@example.com".to_owned(),
                    password: "correct workshop password".to_owned(),
                    client_network: "socket:127.0.0.1".to_owned(),
                })
                .await,
            Err(LoginError::Unavailable)
        ));
        assert_eq!(
            *events.lock().expect("event record should lock"),
            ["session:create", "jwt:issue", "session:revoke"]
        );
    }
}
