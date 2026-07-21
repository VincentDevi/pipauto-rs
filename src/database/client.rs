//! Application-owned `SurrealDB` client and engine-neutral health interface.

use std::{future::IntoFuture, sync::Arc, time::Duration};

use async_trait::async_trait;
use surrealdb::{
    engine::{any, any::Any},
    opt::auth::Root,
    opt::{
        capabilities::{Capabilities, ExperimentalFeature},
        Config as SurrealConfig,
    },
    types::Value,
    Surreal,
};
use thiserror::Error;
use tokio::time::timeout;

use super::settings::{DatabaseEngine, DatabaseSettings};

/// One private bucket shared by all stored attachments.
pub const ATTACHMENT_BUCKET_NAME: &str = "pipauto_attachments";

/// Cheap-to-clone application database service stored in Loco's shared store.
#[derive(Clone)]
pub struct AppDatabase {
    client: Option<Surreal<Any>>,
    health_service: Arc<dyn DatabaseHealthService>,
}

impl AppDatabase {
    /// Connect, authenticate when remote, select the namespace/database, and verify health.
    ///
    /// # Errors
    ///
    /// Returns a stage-specific, secret-free error if startup cannot safely continue.
    pub async fn connect(settings: &DatabaseSettings) -> Result<Self, DatabaseStartupError> {
        let client = match settings.engine() {
            DatabaseEngine::Websocket => {
                run_startup_operation(
                    settings.connection_timeout(),
                    DatabaseStartupStage::Connection,
                    any::connect(settings.endpoint()),
                )
                .await?
            }
            DatabaseEngine::Memory => {
                let capabilities = Capabilities::new()
                    .with_experimental_feature_allowed(ExperimentalFeature::Files);
                let config = SurrealConfig::new().capabilities(capabilities);
                run_startup_operation(
                    settings.connection_timeout(),
                    DatabaseStartupStage::Connection,
                    any::connect(("mem://", config)),
                )
                .await?
            }
        };

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
            client: Some(client.clone()),
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
            client: None,
            health_service: service,
        }
    }

    /// Clone the selected application client for repository composition.
    ///
    /// # Errors
    ///
    /// Returns an error only for health-only test doubles that deliberately have no client.
    pub fn client(&self) -> Result<Surreal<Any>, DatabaseAccessError> {
        self.client.clone().ok_or(DatabaseAccessError)
    }

    /// Check whether the selected database can execute a minimal query.
    ///
    /// # Errors
    ///
    /// Returns an opaque error. Connection details and raw query errors are deliberately hidden.
    pub async fn health(&self) -> Result<(), DatabaseHealthError> {
        self.health_service.health().await
    }

    /// Inspect the selected database's bucket catalog without defining schema or touching objects.
    ///
    /// The safe status is suitable for readiness diagnostics and rollout preflight checks. It does
    /// not expose the backend path or raw database errors.
    pub async fn attachment_bucket_status(
        &self,
    ) -> Result<AttachmentBucketStatus, DatabaseBucketCapabilityError> {
        let client = self.client.as_ref().ok_or(DatabaseBucketCapabilityError)?;
        let mut response = client
            .query("RETURN type::file($bucket, '/capability-check'); INFO FOR DB;")
            .bind(("bucket", ATTACHMENT_BUCKET_NAME))
            .await
            .map_err(|_| DatabaseBucketCapabilityError)?
            .check()
            .map_err(|_| DatabaseBucketCapabilityError)?;
        let capability: Value = response
            .take(0)
            .map_err(|_| DatabaseBucketCapabilityError)?;
        let Value::File(capability) = capability else {
            return Err(DatabaseBucketCapabilityError);
        };
        if capability.bucket() != ATTACHMENT_BUCKET_NAME {
            return Err(DatabaseBucketCapabilityError);
        }
        let catalog: Value = response
            .take(1)
            .map_err(|_| DatabaseBucketCapabilityError)?;
        let catalog = catalog.into_json_value();
        let definition = catalog
            .get("buckets")
            .and_then(|buckets| buckets.get(ATTACHMENT_BUCKET_NAME))
            .and_then(serde_json::Value::as_str);

        Ok(match definition {
            None => AttachmentBucketStatus::Missing,
            Some(definition)
                if definition.contains("PERMISSIONS NONE")
                    && (definition.contains("BACKEND 'memory'")
                        || definition.contains("BACKEND 'file:")) =>
            {
                AttachmentBucketStatus::Ready
            }
            Some(_) => AttachmentBucketStatus::Misconfigured,
        })
    }
}

/// Safe attachment-bucket catalog state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentBucketStatus {
    Missing,
    Ready,
    Misconfigured,
}

impl AttachmentBucketStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Ready => "ready",
            Self::Misconfigured => "misconfigured",
        }
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

/// The database value is a health-only test seam and cannot serve repositories.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("application database client is unavailable")]
pub struct DatabaseAccessError;

/// Opaque failure from the non-mutating attachment-bucket catalog check.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("attachment storage capability is unavailable")]
pub struct DatabaseBucketCapabilityError;
