//! Authentication persistence maintenance tasks.

use async_trait::async_trait;
use chrono::Utc;
use loco_rs::{
    app::AppContext,
    task::{Task, TaskInfo, Vars},
    Error, Result,
};

use crate::{
    database::{client::AppDatabase, schema::apply_auth_schema},
    repositories::{auth::AuthSessionRepository, surreal::auth::SurrealAuthRepository},
};

/// Idempotently apply strict authentication tables, fields, and indexes.
pub struct ApplyAuthSchema;

#[async_trait]
impl Task for ApplyAuthSchema {
    fn task(&self) -> TaskInfo {
        TaskInfo {
            name: "apply_auth_schema".to_owned(),
            detail: "Apply Pipauto authentication schema definitions".to_owned(),
        }
    }

    async fn run(&self, ctx: &AppContext, _vars: &Vars) -> Result<()> {
        let database = ctx
            .shared_store
            .get::<AppDatabase>()
            .ok_or_else(|| Error::string("application database is not installed"))?;
        let client = database.client().map_err(Error::msg)?;
        apply_auth_schema(&client).await.map_err(Error::msg)?;
        println!("authentication schema applied");
        Ok(())
    }
}

/// Delete only registry sessions whose fixed expiry is in the past.
pub struct PurgeExpiredAuthSessions;

#[async_trait]
impl Task for PurgeExpiredAuthSessions {
    fn task(&self) -> TaskInfo {
        TaskInfo {
            name: "purge_expired_auth_sessions".to_owned(),
            detail: "Delete expired authentication session records".to_owned(),
        }
    }

    async fn run(&self, ctx: &AppContext, _vars: &Vars) -> Result<()> {
        let database = ctx
            .shared_store
            .get::<AppDatabase>()
            .ok_or_else(|| Error::string("application database is not installed"))?;
        let client = database.client().map_err(Error::msg)?;
        let repository = SurrealAuthRepository::new(client);
        let count = repository
            .delete_expired(Utc::now())
            .await
            .map_err(Error::msg)?;
        println!("removed {count} expired authentication sessions");
        Ok(())
    }
}
