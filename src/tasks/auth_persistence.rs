//! Authentication persistence maintenance tasks.

use async_trait::async_trait;
use loco_rs::{
    app::AppContext,
    task::{Task, TaskInfo, Vars},
    Error, Result,
};

use crate::{database::client::AppDatabase, models::auth::AuthenticationModel as AuthService};

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
        let _client = database.client().map_err(Error::msg)?;
        let service = ctx
            .shared_store
            .get::<AuthService>()
            .ok_or_else(|| Error::string("authentication service is not installed"))?;
        let count = service.purge_expired_sessions().await.map_err(Error::msg)?;
        println!("removed {count} expired authentication sessions");
        Ok(())
    }
}
