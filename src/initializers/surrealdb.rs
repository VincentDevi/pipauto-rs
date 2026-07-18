//! Loco lifecycle adapter for installing the application database.

use loco_rs::{app::AppContext, Error, Result};

use crate::database::{client::AppDatabase, settings::DatabaseSettings};

/// Create and verify the one application database, then install it in the shared store.
///
/// # Errors
///
/// Returns a safe startup error when configuration or database initialization fails.
pub async fn install(ctx: &AppContext) -> Result<()> {
    let settings = DatabaseSettings::from_config(&ctx.config).map_err(Error::msg)?;
    let database = AppDatabase::connect(&settings).await.map_err(Error::msg)?;
    ctx.shared_store.insert(database);
    Ok(())
}
