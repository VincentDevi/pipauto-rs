//! Application-owned `SurrealDB` client and engine-neutral health interface.

use std::{future::IntoFuture, sync::Arc, time::Duration};

use async_trait::async_trait;
use surrealdb::{
    engine::{any, any::Any},
    opt::auth::Root,
    Surreal,
};
use thiserror::Error;
use tokio::time::timeout;

use super::settings::{DatabaseEngine, DatabaseSettings};

/// Cheap-to-clone application database service stored in Loco's shared store.
#[derive(Clone)]
pub struct AppDatabase {
    health_service: Arc<dyn DatabaseHealthService>,
}

impl AppDatabase {
    /// Connect, authenticate when remote, select the namespace/database, and verify health.
    ///
    /// # Errors
    ///
    /// Returns a stage-specific, secret-free error if startup cannot safely continue.
    pub async fn connect(settings: &DatabaseSettings) -> Result<Self, DatabaseStartupError> {
        let client = run_startup_operation(
            settings.connection_timeout(),
            DatabaseStartupStage::Connection,
            any::connect(settings.endpoint()),
        )
        .await?;

        if settings.engine() == DatabaseEngine::Websocket {
            let credentials = Root {
                username: settings.username().to_owned(),
                password: settings.password().to_owned(),
            };
            run_startup_operation(
                settings.connection_timeout(),
                DatabaseStartupStage::Authentication,
                client.signin(credentials),
            )
            .await?;
        }

        run_startup_operation(
            settings.connection_timeout(),
            DatabaseStartupStage::Selection,
            client
                .use_ns(settings.namespace())
                .use_db(settings.database()),
        )
        .await?;

        let database = Self {
            health_service: Arc::new(SurrealHealthService {
                client,
                timeout: settings.connection_timeout(),
            }),
        };
        database
            .health()
            .await
            .map_err(|_| DatabaseStartupError::Failed {
                stage: DatabaseStartupStage::Health,
            })?;

        Ok(database)
    }

    /// Build an `AppDatabase` around an engine-neutral service.
    ///
    /// This is primarily a test seam for deterministic request-level health behavior. Production
    /// startup uses [`Self::connect`].
    #[doc(hidden)]
    #[must_use]
    pub fn from_health_service(service: Arc<dyn DatabaseHealthService>) -> Self {
        Self {
            health_service: service,
        }
    }

    /// Check whether the selected database can execute a minimal query.
    ///
    /// # Errors
    ///
    /// Returns an opaque error. Connection details and raw query errors are deliberately hidden.
    pub async fn health(&self) -> Result<(), DatabaseHealthError> {
        self.health_service.health().await
    }
}

/// Engine-neutral health behavior used by `AppDatabase`.
#[async_trait]
pub trait DatabaseHealthService: Send + Sync {
    /// Execute a database health check.
    ///
    /// # Errors
    ///
    /// Returns an opaque unavailable error without infrastructure details.
    async fn health(&self) -> Result<(), DatabaseHealthError>;
}

struct SurrealHealthService {
    client: Surreal<Any>,
    timeout: Duration,
}

#[async_trait]
impl DatabaseHealthService for SurrealHealthService {
    async fn health(&self) -> Result<(), DatabaseHealthError> {
        let query = async {
            self.client.query("RETURN true;").await?.check()?;
            Ok::<(), surrealdb::Error>(())
        };

        match timeout(self.timeout, query).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) | Err(_) => Err(DatabaseHealthError),
        }
    }
}

async fn run_startup_operation<T, F>(
    operation_timeout: Duration,
    stage: DatabaseStartupStage,
    operation: F,
) -> Result<T, DatabaseStartupError>
where
    F: IntoFuture<Output = surrealdb::Result<T>>,
{
    match timeout(operation_timeout, operation.into_future()).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(_)) => Err(DatabaseStartupError::Failed { stage }),
        Err(_) => Err(DatabaseStartupError::TimedOut { stage }),
    }
}

/// Startup stage used in safe operational errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DatabaseStartupStage {
    /// Establishing the client connection.
    Connection,
    /// Authenticating the remote client.
    Authentication,
    /// Selecting the namespace and database.
    Selection,
    /// Running the initial health query.
    Health,
}

impl std::fmt::Display for DatabaseStartupStage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let stage = match self {
            Self::Connection => "connection",
            Self::Authentication => "authentication",
            Self::Selection => "namespace/database selection",
            Self::Health => "initial health check",
        };
        formatter.write_str(stage)
    }
}

/// Secret-free failure produced while creating the application database.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum DatabaseStartupError {
    /// The indicated startup operation failed.
    #[error("SurrealDB {stage} failed")]
    Failed { stage: DatabaseStartupStage },
    /// The indicated startup operation exceeded the configured timeout.
    #[error("SurrealDB {stage} timed out")]
    TimedOut { stage: DatabaseStartupStage },
}

/// Opaque health failure suitable for crossing controller/service boundaries.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("SurrealDB is unavailable")]
pub struct DatabaseHealthError;
