use chrono::{TimeDelta, Utc};
use pipauto::{
    models::auth::{NewAuthSession, NewUserRecord, NormalizedEmail, SessionDigest, ThrottleDigest},
    testing::persistence::{
        auth::{
            AuthSessionRepository, LoginThrottleRepository, RepositoryError, RevokeOutcome,
            ThrottleState, UserRepository,
        },
        surreal::auth::SurrealAuthRepository,
    },
};
use surrealdb::engine::any;

use crate::support::apply_authentication_schema;

async fn repository() -> SurrealAuthRepository {
    let client = any::connect("memory")
        .await
        .expect("fresh in-memory database should connect");
    client
        .use_ns("pipauto_repository_tests")
        .use_db("authentication")
        .await
        .expect("test database should select");
    apply_authentication_schema(&client).await;
    apply_authentication_schema(&client).await;
    SurrealAuthRepository::new(client)
}

fn new_user(email: &str) -> NewUserRecord {
    NewUserRecord::new(
        email.to_owned(),
        NormalizedEmail::parse(email).expect("email should be valid"),
        "Filippo".to_owned(),
        "$argon2id$repository-fixture".to_owned(),
    )
    .expect("new user should be valid")
}

#[tokio::test]
async fn user_repository_enforces_unique_email_and_redacts_credentials() {
    let repository = repository().await;
    let created = UserRepository::create(&repository, new_user("Filippo@Example.com"))
        .await
        .expect("user should create");
    assert_eq!(created.email, "Filippo@Example.com");
    assert_eq!(
        UserRepository::create(&repository, new_user("filippo@example.com")).await,
        Err(RepositoryError::Conflict)
    );
    assert_eq!(
        repository
            .find_by_id(&created.id)
            .await
            .expect("lookup should work"),
        Some(created)
    );
    let credentials = repository
        .find_credentials_by_email(
            &NormalizedEmail::parse("FILIPPO@example.com").expect("email should normalize"),
        )
        .await
        .expect("credential lookup should work")
        .expect("credentials should exist");
    let debug = format!("{credentials:?}");
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("repository-fixture"));
}

#[tokio::test]
async fn auth_session_repository_is_unique_active_only_and_idempotently_revocable() {
    let repository = repository().await;
    let user = UserRepository::create(&repository, new_user("sessions@example.com"))
        .await
        .expect("user should create");
    let now = Utc::now();
    let digest = SessionDigest::parse("a".repeat(64)).expect("digest should be valid");
    let session = NewAuthSession {
        user_id: user.id.clone(),
        jti_digest: digest.clone(),
        issued_at: now,
        expires_at: now + TimeDelta::minutes(5),
        created_ip_digest: None,
        user_agent_summary: Some(format!("Workshop\n{}", "B".repeat(300))),
    };
    let created = AuthSessionRepository::create(&repository, session.clone())
        .await
        .expect("session should create");
    let summary = created
        .user_agent_summary
        .expect("sanitized summary should persist");
    assert!(!summary.chars().any(char::is_control));
    assert_eq!(summary.chars().count(), 256);
    assert_eq!(
        AuthSessionRepository::create(&repository, session).await,
        Err(RepositoryError::Conflict)
    );
    assert!(repository
        .find_active(&digest, now)
        .await
        .expect("lookup should work")
        .is_some());

    let first_repository = repository.clone();
    let first_digest = digest.clone();
    let second_repository = repository.clone();
    let (first, second) = tokio::join!(
        first_repository.revoke(&first_digest, now),
        second_repository.revoke(&digest, now)
    );
    let outcomes = [
        first.expect("first revocation should work"),
        second.expect("second revocation should work"),
    ];
    assert!(outcomes.contains(&RevokeOutcome::Revoked));
    assert!(outcomes.contains(&RevokeOutcome::AlreadyInactive));
    assert!(repository
        .find_active(&digest, now)
        .await
        .expect("revoked lookup should work")
        .is_none());

    let expired = SessionDigest::parse("b".repeat(64)).expect("digest should be valid");
    AuthSessionRepository::create(
        &repository,
        NewAuthSession {
            user_id: user.id,
            jti_digest: expired.clone(),
            issued_at: now - TimeDelta::minutes(5),
            expires_at: now - TimeDelta::seconds(1),
            created_ip_digest: None,
            user_agent_summary: None,
        },
    )
    .await
    .expect("expired session should persist until cleanup");
    assert!(repository
        .find_active(&expired, now)
        .await
        .expect("expired lookup should work")
        .is_none());
    assert_eq!(repository.delete_expired(now).await.expect("cleanup"), 1);
}

#[tokio::test]
async fn purge_expired_auth_sessions_removes_only_past_expiry() {
    let repository = repository().await;
    let user = UserRepository::create(&repository, new_user("purge@example.com"))
        .await
        .expect("user should create");
    let now = Utc::now();
    for (digest_byte, expires_at) in [
        ('c', now - TimeDelta::seconds(1)),
        ('d', now),
        ('e', now + TimeDelta::seconds(1)),
    ] {
        AuthSessionRepository::create(
            &repository,
            NewAuthSession {
                user_id: user.id.clone(),
                jti_digest: SessionDigest::parse(digest_byte.to_string().repeat(64))
                    .expect("digest should parse"),
                issued_at: now - TimeDelta::minutes(1),
                expires_at,
                created_ip_digest: None,
                user_agent_summary: None,
            },
        )
        .await
        .expect("session should create");
    }

    assert_eq!(
        repository
            .delete_expired(now)
            .await
            .expect("purge should work"),
        1
    );
    assert!(repository
        .find_active(
            &SessionDigest::parse("e".repeat(64)).expect("digest should parse"),
            now,
        )
        .await
        .expect("active lookup should work")
        .is_some());
}

#[tokio::test]
async fn login_throttle_repository_blocks_fifth_failure_and_clears_success() {
    let repository = repository().await;
    let now = Utc::now();
    let identifier = ThrottleDigest::parse("i".repeat(43)).expect("valid digest");
    let network = ThrottleDigest::parse("n".repeat(43)).expect("valid digest");
    for attempt in 1..=5 {
        let state = repository
            .record_failure(
                &identifier,
                &network,
                now,
                std::time::Duration::from_secs(15 * 60),
                5,
                std::time::Duration::from_secs(15 * 60),
            )
            .await
            .expect("failure should record");
        if attempt == 5 {
            assert!(matches!(state, ThrottleState::BlockedUntil(_)));
        } else {
            assert_eq!(state, ThrottleState::Allowed);
        }
    }
    repository
        .clear(&identifier, &network)
        .await
        .expect("successful authentication should clear state");
    assert_eq!(
        repository
            .state(&identifier, &network, now)
            .await
            .expect("state should load"),
        ThrottleState::Allowed
    );
}
