//! Operator-invoked stored-attachment reconciliation task.

use async_trait::async_trait;
use loco_rs::{
    app::AppContext,
    task::{Task, TaskInfo, Vars},
    Error, Result,
};

use crate::services::attachment_reconciliation::{AttachmentReconciler, ReconciliationMode};

pub struct ReconcileAttachments;

#[async_trait]
impl Task for ReconcileAttachments {
    fn task(&self) -> TaskInfo {
        TaskInfo {
            name: "attachment_reconciliation".to_owned(),
            detail: "Dry-run or explicitly apply safe attachment storage recovery".to_owned(),
        }
    }

    async fn run(&self, ctx: &AppContext, vars: &Vars) -> Result<()> {
        let mode = reconciliation_mode(vars)?;
        let service = ctx
            .shared_store
            .get::<AttachmentReconciler>()
            .ok_or_else(|| Error::string("attachment reconciliation is not installed"))?;
        let report = service.reconcile(mode).await.map_err(Error::msg)?;
        println!("{}", report.safe_output(mode));
        Ok(())
    }
}

fn reconciliation_mode(vars: &Vars) -> Result<ReconciliationMode> {
    let apply = parse_flag(vars, "apply")?;
    let quiesced_writes = parse_flag(vars, "quiesced_writes")?;
    if apply {
        if !quiesced_writes {
            return Err(Error::string(
                "apply requires quiesced_writes=true after attachment writes are stopped",
            ));
        }
        Ok(ReconciliationMode::Apply)
    } else {
        Ok(ReconciliationMode::DryRun)
    }
}

fn parse_flag(vars: &Vars, name: &str) -> Result<bool> {
    match vars.cli.get(name).map(String::as_str) {
        None | Some("false") => Ok(false),
        Some("true") => Ok(true),
        Some(_) => Err(Error::Message(format!("{name} must be true or false"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_reconciliation_defaults_to_dry_run() {
        assert_eq!(
            reconciliation_mode(&Vars::default()).expect("dry-run default"),
            ReconciliationMode::DryRun
        );
    }

    #[test]
    fn attachment_reconciliation_apply_requires_quiesced_writes() {
        let vars = Vars::from_cli_args(vec![("apply".to_owned(), "true".to_owned())]);
        assert!(reconciliation_mode(&vars).is_err());
        let vars = Vars::from_cli_args(vec![
            ("apply".to_owned(), "true".to_owned()),
            ("quiesced_writes".to_owned(), "true".to_owned()),
        ]);
        assert_eq!(
            reconciliation_mode(&vars).expect("explicit safe apply"),
            ReconciliationMode::Apply
        );
    }
}
