use std::{collections::VecDeque, io};

use chrono::{TimeDelta, Utc};
use loco_rs::task::Vars;
use loco_rs::testing::request::boot_test;
use pipauto::{
    app::App,
    database::schema::apply_auth_schema,
    models::auth::{NewAuthSession, NewUserRecord, NormalizedEmail, SessionDigest, ThrottleDigest},
    repositories::{
        auth::{
            AuthSessionRepository, LoginThrottleRepository, RepositoryError, RevokeOutcome,
            ThrottleState, UserRepository,
        },
        surreal::auth::SurrealAuthRepository,
    },
    services::auth::{AuthError, AuthService, LoginCommand},
    tasks::auth::{CreateUser, PasswordReader},
};
use surrealdb::engine::any;

async fn repository() -> SurrealAuthRepository {
    let client = any::connect("memory")
        .await
        .expect("fresh in-memory database should connect");
    client
        .use_ns("pipauto_repository_tests")
        .use_db("authentication")
        .await
        .expect("test namespace and database should select");
    apply_auth_schema(&client)
        .await
        .expect("authentication schema should apply to a fresh database");
    apply_auth_schema(&client)
        .await
        .expect("authentication schema should apply idempotently");
    SurrealAuthRepository::new(client)
}

#[tokio::test]
async fn user_repository_enforces_normalized_email_uniqueness_and_safe_projection() {
    let repository = repository().await;
    let normalized = NormalizedEmail::parse("filippo@example.com").expect("email should be valid");
    let input = || {
        NewUserRecord::new(
            "Filippo@Example.com".to_owned(),
            normalized.clone(),
            "Filippo".to_owned(),
            "$argon2id$fixture-redacted".to_owned(),
        )
        .expect("new user record should be valid")
    };
    let created = UserRepository::create(&repository, input())
        .await
        .expect("user should create");
    assert_eq!(created.email, "Filippo@Example.com");
    assert_eq!(
        UserRepository::create(&repository, input()).await,
        Err(RepositoryError::Conflict)
    );
    let projected = repository
        .find_by_id(&created.id)
        .await
        .expect("lookup should work")
        .expect("user should exist");
    assert_eq!(projected, created);
    assert!(!format!("{projected:?}").contains("argon2id"));
    let credentials = repository
        .find_credentials_by_email(&normalized)
        .await
        .expect("credential lookup should work")
        .expect("credentials should exist");
    let debug = format!("{credentials:?}");
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("fixture-redacted"));
}

#[tokio::test]
async fn auth_session_repository_excludes_expired_and_revoked_rows() {
    let repository = repository().await;
    let user = UserRepository::create(
        &repository,
        NewUserRecord::new(
            "filippo@example.com".to_owned(),
            NormalizedEmail::parse("filippo@example.com").expect("email should be valid"),
            "Filippo".to_owned(),
            "$argon2id$session-fixture".to_owned(),
        )
        .expect("new user record should be valid"),
    )
    .await
    .expect("user should create");
    let now = Utc::now();
    let digest = SessionDigest::parse("a".repeat(64)).expect("digest should be valid");
    let created_ip_digest =
        ThrottleDigest::parse("z".repeat(43)).expect("network digest should be valid");
    let created_session = AuthSessionRepository::create(
        &repository,
        NewAuthSession {
            user_id: user.id.clone(),
            jti_digest: digest.clone(),
            issued_at: now,
            expires_at: now + TimeDelta::minutes(5),
            created_ip_digest: Some(created_ip_digest.clone()),
            user_agent_summary: Some(format!("Workshop\n{}", "B".repeat(300))),
        },
    )
    .await
    .expect("session should create");
    assert_eq!(created_session.created_ip_digest, Some(created_ip_digest));
    let summary = created_session
        .user_agent_summary
        .expect("sanitized summary should persist");
    assert!(!summary.chars().any(char::is_control));
    assert_eq!(summary.chars().count(), 256);
    assert_eq!(
        AuthSessionRepository::create(
            &repository,
            NewAuthSession {
                user_id: user.id.clone(),
                jti_digest: digest.clone(),
                issued_at: now,
                expires_at: now + TimeDelta::minutes(5),
                created_ip_digest: None,
                user_agent_summary: None,
            },
        )
        .await,
        Err(RepositoryError::Conflict)
    );
    assert!(repository
        .find_active(&digest, now)
        .await
        .expect("lookup should work")
        .is_some());
    assert_eq!(
        repository
            .revoke(&digest, now)
            .await
            .expect("revoke should work"),
        RevokeOutcome::Revoked
    );
    assert!(repository
        .find_active(&digest, now)
        .await
        .expect("lookup should work")
        .is_none());
    assert_eq!(
        repository
            .revoke(&digest, now)
            .await
            .expect("revoke should work"),
        RevokeOutcome::AlreadyInactive
    );

    let expired_digest = SessionDigest::parse("b".repeat(64)).expect("digest should be valid");
    AuthSessionRepository::create(
        &repository,
        NewAuthSession {
            user_id: user.id,
            jti_digest: expired_digest.clone(),
            issued_at: now - TimeDelta::minutes(10),
            expires_at: now - TimeDelta::seconds(1),
            created_ip_digest: None,
            user_agent_summary: Some("Workshop\nBrowser\u{0000}".to_owned()),
        },
    )
    .await
    .expect("expired session should persist for cleanup");
    assert!(repository
        .find_active(&expired_digest, now)
        .await
        .expect("expired lookup should work")
        .is_none());
    assert_eq!(
        repository
            .delete_expired(now)
            .await
            .expect("cleanup should work"),
        1
    );
}

#[tokio::test]
async fn auth_session_repository_concurrent_revocation_is_idempotent() {
    let repository = repository().await;
    let user = UserRepository::create(
        &repository,
        NewUserRecord::new(
            "logout@example.com".to_owned(),
            NormalizedEmail::parse("logout@example.com").expect("email should be valid"),
            "Logout Test".to_owned(),
            "$argon2id$concurrent-fixture".to_owned(),
        )
        .expect("new user record should be valid"),
    )
    .await
    .expect("user should create");
    let now = Utc::now();
    let digest = SessionDigest::parse("c".repeat(64)).expect("digest should be valid");
    AuthSessionRepository::create(
        &repository,
        NewAuthSession {
            user_id: user.id,
            jti_digest: digest.clone(),
            issued_at: now,
            expires_at: now + TimeDelta::minutes(5),
            created_ip_digest: None,
            user_agent_summary: None,
        },
    )
    .await
    .expect("session should create");

    let first_repository = repository.clone();
    let first_digest = digest.clone();
    let second_repository = repository.clone();
    let (first, second) = tokio::join!(
        first_repository.revoke(&first_digest, now),
        second_repository.revoke(&digest, now)
    );
    let mut outcomes = [
        first.expect("first revocation should complete"),
        second.expect("second revocation should complete"),
    ];
    outcomes.sort_by_key(|outcome| match outcome {
        RevokeOutcome::Revoked => 0,
        RevokeOutcome::AlreadyInactive => 1,
    });
    assert_eq!(
        outcomes,
        [RevokeOutcome::Revoked, RevokeOutcome::AlreadyInactive]
    );
}

#[tokio::test]
async fn login_throttle_repository_blocks_at_limit_and_success_clear_resets_state() {
    let repository = repository().await;
    let now = Utc::now();
    let identifier_digest =
        ThrottleDigest::parse("i".repeat(43)).expect("identifier digest should be valid");
    let network_digest =
        ThrottleDigest::parse("n".repeat(43)).expect("network digest should be valid");
    for attempt in 1..=5 {
        let state = repository
            .record_failure(
                &identifier_digest,
                &network_digest,
                now,
                std::time::Duration::from_secs(900),
                5,
                std::time::Duration::from_secs(900),
            )
            .await
            .expect("failure should record");
        if attempt < 5 {
            assert_eq!(state, ThrottleState::Allowed);
        } else {
            assert!(matches!(state, ThrottleState::BlockedUntil(_)));
        }
    }
    repository
        .clear(&identifier_digest, &network_digest)
        .await
        .expect("state should clear");
    assert_eq!(
        repository
            .state(&identifier_digest, &network_digest, now)
            .await
            .expect("state should load"),
        ThrottleState::Allowed
    );
}

#[tokio::test]
async fn authentication_registry_round_trip_revokes_a_copied_jwt() {
    let boot = boot_test::<App>()
        .await
        .expect("test application should boot");
    let service = boot
        .app_context
        .shared_store
        .get::<AuthService>()
        .expect("authentication service should be installed");
    service
        .create_user("filippo@example.com", "Filippo", "workshop password")
        .await
        .expect("user should be created");

    let login = service
        .login(LoginCommand {
            email: " FILIPPO@example.com ".to_owned(),
            password: "workshop password".to_owned(),
            client_network: "socket:127.0.0.1".to_owned(),
        })
        .await
        .expect("valid credentials should authenticate");
    let token = login.encoded_jwt().to_owned();
    let current = service
        .authenticate(&token)
        .await
        .expect("registered token should authenticate");
    assert_eq!(current.display_name, "Filippo");

    assert_eq!(
        service
            .logout(Some(&token))
            .await
            .expect("logout should work"),
        RevokeOutcome::Revoked
    );
    assert_eq!(
        service.authenticate(&token).await,
        Err(AuthError::Unauthenticated)
    );
    assert_eq!(
        service
            .logout(Some(&token))
            .await
            .expect("logout is idempotent"),
        RevokeOutcome::AlreadyInactive
    );
}

struct StubPasswordReader {
    entries: VecDeque<io::Result<String>>,
}

impl StubPasswordReader {
    fn matching(password: &str) -> Self {
        Self {
            entries: VecDeque::from([Ok(password.to_owned()), Ok(password.to_owned())]),
        }
    }

    fn mismatched(password: &str, confirmation: &str) -> Self {
        Self {
            entries: VecDeque::from([Ok(password.to_owned()), Ok(confirmation.to_owned())]),
        }
    }
}

impl PasswordReader for StubPasswordReader {
    fn read_password(&mut self, _prompt: &str) -> io::Result<String> {
        self.entries.pop_front().unwrap_or_else(|| {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "no test password remains",
            ))
        })
    }
}

fn create_user_vars(email: &str) -> Vars {
    Vars::from_cli_args(vec![
        ("email".to_owned(), email.to_owned()),
        ("display_name".to_owned(), " Filippo ".to_owned()),
    ])
}

#[tokio::test]
async fn create_user_task_succeeds_against_memory_and_duplicate_is_non_destructive() {
    let boot = boot_test::<App>()
        .await
        .expect("test application should boot");
    let vars = create_user_vars(" FILIPPO@example.com ");
    let task = CreateUser;
    task.run_with_reader(
        &boot.app_context,
        &vars,
        &mut StubPasswordReader::matching("original workshop password"),
    )
    .await
    .expect("first task execution should create a user");

    let duplicate = task
        .run_with_reader(
            &boot.app_context,
            &vars,
            &mut StubPasswordReader::matching("replacement workshop password"),
        )
        .await;
    assert!(duplicate.is_err());

    let service = boot
        .app_context
        .shared_store
        .get::<AuthService>()
        .expect("authentication service should be installed");
    assert!(service
        .login(LoginCommand {
            email: "filippo@example.com".to_owned(),
            password: "original workshop password".to_owned(),
            client_network: "test:create-user-task".to_owned(),
        })
        .await
        .is_ok());
}

#[tokio::test]
async fn create_user_task_password_mismatch_creates_no_record() {
    let boot = boot_test::<App>()
        .await
        .expect("test application should boot");
    let vars = create_user_vars("mismatch@example.com");
    let task = CreateUser;
    assert!(task
        .run_with_reader(
            &boot.app_context,
            &vars,
            &mut StubPasswordReader::mismatched(
                "original workshop password",
                "different workshop password",
            ),
        )
        .await
        .is_err());
    task.run_with_reader(
        &boot.app_context,
        &vars,
        &mut StubPasswordReader::matching("original workshop password"),
    )
    .await
    .expect("a corrected rerun should create the previously absent user");
}

#[tokio::test]
async fn create_user_task_requires_terminal_reader_success() {
    let boot = boot_test::<App>()
        .await
        .expect("test application should boot");
    let vars = create_user_vars("terminal@example.com");
    let mut reader = StubPasswordReader {
        entries: VecDeque::from([Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "no controlling terminal",
        ))]),
    };

    assert!(CreateUser
        .run_with_reader(&boot.app_context, &vars, &mut reader)
        .await
        .is_err());
}
