//! Secret-safe validation decisions used by migration verification and deployment gates.

use serde_json::Value;
use thiserror::Error;

const AUTHENTICATION_TABLES: [&str; 3] = ["auth_session", "login_throttle", "user"];

/// Failure returned while comparing live authentication definitions with the baseline catalog.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CatalogValidationError {
    /// The database contains a missing, extra, or changed table definition.
    #[error("authentication catalog drift detected for table {table}")]
    TableDrift { table: String },
    /// The catalog document itself is incomplete or has an unexpected shape.
    #[error("authentication catalog comparison could not read {section}")]
    InvalidCatalog { section: &'static str },
}

/// Compare authentication definitions without returning definition text or record contents.
pub fn validate_authentication_catalog(
    actual: &Value,
    expected: &Value,
) -> Result<(), CatalogValidationError> {
    let actual = actual
        .as_object()
        .ok_or(CatalogValidationError::InvalidCatalog {
            section: "live catalog",
        })?;
    let expected = expected
        .as_object()
        .ok_or(CatalogValidationError::InvalidCatalog {
            section: "expected catalog",
        })?;

    for table in AUTHENTICATION_TABLES {
        if actual.get(table) != expected.get(table) {
            return Err(CatalogValidationError::TableDrift {
                table: table.to_owned(),
            });
        }
    }

    if actual.len() != AUTHENTICATION_TABLES.len() || expected.len() != AUTHENTICATION_TABLES.len()
    {
        return Err(CatalogValidationError::InvalidCatalog {
            section: "authentication table set",
        });
    }

    Ok(())
}

/// The deployment decision for a recorded SurrealKit rollout state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeploymentDecision {
    /// Compatible application code may be deployed before the contract phase.
    Permit,
    /// Deployment must stop and the named operator action must be followed.
    Block { action: &'static str },
}

/// Apply the approved deployment policy to every SurrealKit rollout state.
#[must_use]
pub fn rollout_deployment_decision(status: &str) -> DeploymentDecision {
    match status {
        "ready_to_complete" => DeploymentDecision::Permit,
        "planned" => DeploymentDecision::Block {
            action: "run the reviewed rollout start phase",
        },
        "running_start" | "running_complete" | "running_rollback" => DeploymentDecision::Block {
            action: "inspect the rollout and follow the interrupted-rollout repair procedure",
        },
        "failed" => DeploymentDecision::Block {
            action: "inspect the failed phase and make a reviewed retry-or-rollback decision",
        },
        "completed" => DeploymentDecision::Block {
            action: "use a new forward rollout or isolated backup recovery",
        },
        "rolled_back" => DeploymentDecision::Block {
            action: "do not deploy code that requires the abandoned rollout",
        },
        _ => DeploymentDecision::Block {
            action: "preserve evidence and escalate the unknown rollout state",
        },
    }
}

/// Return the first documented operator command for the observed rollout state.
#[must_use]
pub fn rollout_recovery_command(status: &str, rollout_id: &str) -> Option<String> {
    match status {
        "planned" => Some(format!("./scripts/surrealkit rollout start {rollout_id}")),
        "running_start" | "running_complete" | "running_rollback" => {
            Some(format!("./scripts/surrealkit rollout repair {rollout_id}"))
        }
        "ready_to_complete" => Some(format!(
            "./scripts/surrealkit rollout complete {rollout_id}"
        )),
        "failed" => Some(format!("./scripts/surrealkit rollout status {rollout_id}")),
        "completed" | "rolled_back" => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{rollout_deployment_decision, rollout_recovery_command, DeploymentDecision};

    #[test]
    fn only_ready_to_complete_permits_deployment() {
        assert_eq!(
            rollout_deployment_decision("ready_to_complete"),
            DeploymentDecision::Permit
        );

        for status in [
            "planned",
            "running_start",
            "running_complete",
            "completed",
            "running_rollback",
            "rolled_back",
            "failed",
            "unexpected",
        ] {
            assert!(matches!(
                rollout_deployment_decision(status),
                DeploymentDecision::Block { .. }
            ));
        }
    }

    #[test]
    fn every_rollout_state_has_an_explicit_recovery_command_policy() {
        let rollout = "20260719090000__vehicle_index";
        let expectations = [
            (
                "planned",
                Some("./scripts/surrealkit rollout start 20260719090000__vehicle_index"),
            ),
            (
                "running_start",
                Some("./scripts/surrealkit rollout repair 20260719090000__vehicle_index"),
            ),
            (
                "ready_to_complete",
                Some("./scripts/surrealkit rollout complete 20260719090000__vehicle_index"),
            ),
            (
                "running_complete",
                Some("./scripts/surrealkit rollout repair 20260719090000__vehicle_index"),
            ),
            ("completed", None),
            (
                "running_rollback",
                Some("./scripts/surrealkit rollout repair 20260719090000__vehicle_index"),
            ),
            ("rolled_back", None),
            (
                "failed",
                Some("./scripts/surrealkit rollout status 20260719090000__vehicle_index"),
            ),
        ];

        for (status, expected) in expectations {
            assert_eq!(
                rollout_recovery_command(status, rollout).as_deref(),
                expected,
                "unexpected operator command for {status}"
            );
        }
    }
}
