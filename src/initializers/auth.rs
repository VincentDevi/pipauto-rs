//! Authentication service composition for the Loco application lifecycle.

use std::sync::Arc;

use loco_rs::{app::AppContext, environment::Environment, Error, Result};

use crate::{
    auth::{
        cookies::AuthCookies,
        crypto::{adapters, LocoPasswordEngine},
        csrf::CsrfService,
        settings::AuthSettings,
    },
    database::{client::AppDatabase, schema::apply_auth_schema},
    repositories::surreal::auth::SurrealAuthRepository,
    services::auth::{AuthService, PasswordEngine},
};

/// Validate settings and install one shared authentication service.
///
/// # Errors
///
/// Returns a safe startup error before the listener binds when composition fails.
pub async fn install(ctx: &AppContext) -> Result<()> {
    let settings = AuthSettings::from_environment(&ctx.environment).map_err(Error::msg)?;
    let database = ctx
        .shared_store
        .get::<AppDatabase>()
        .ok_or_else(|| Error::string("application database is not installed"))?;
    let client = database.client().map_err(Error::msg)?;
    if ctx.environment == Environment::Test {
        apply_auth_schema(&client).await.map_err(Error::msg)?;
    }

    let repository = Arc::new(SurrealAuthRepository::new(client));
    let passwords: Arc<dyn PasswordEngine> = Arc::new(LocoPasswordEngine);
    let dummy_password_hash = passwords
        .hash("pipauto dummy verification password")
        .await
        .map_err(Error::msg)?;
    let (clock, random, jwt) = adapters(settings.jwt_secret());
    let service = AuthService::new(
        settings.clone(),
        repository.clone(),
        repository.clone(),
        repository,
        passwords,
        jwt,
        clock,
        random,
        dummy_password_hash,
    );

    ctx.shared_store.insert(settings.clone());
    ctx.shared_store.insert(CsrfService::new(settings.clone()));
    ctx.shared_store.insert(AuthCookies::new(settings));
    ctx.shared_store.insert(service);
    Ok(())
}
