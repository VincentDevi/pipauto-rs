//! Terminal-only authentication administration tasks.

use std::io;

use async_trait::async_trait;
use loco_rs::{
    app::AppContext,
    task::{Task, TaskInfo, Vars},
    Error, Result,
};

use crate::models::auth::AuthenticationModel as AuthService;

/// Password prompt seam that keeps task tests independent of a real terminal.
pub trait PasswordReader: Send {
    /// Read one password from an attached terminal without echoing it.
    fn read_password(&mut self, prompt: &str) -> io::Result<String>;
}

/// Production password reader backed by the controlling terminal.
pub struct TerminalPasswordReader;

impl PasswordReader for TerminalPasswordReader {
    fn read_password(&mut self, prompt: &str) -> io::Result<String> {
        rpassword::prompt_password(prompt)
    }
}

/// Interactively provision one application user.
pub struct CreateUser;

#[async_trait]
impl Task for CreateUser {
    fn task(&self) -> TaskInfo {
        TaskInfo {
            name: "create_user".to_owned(),
            detail: "Create a Pipauto user using a non-echoing password prompt".to_owned(),
        }
    }

    async fn run(&self, ctx: &AppContext, vars: &Vars) -> Result<()> {
        self.run_with_reader(ctx, vars, &mut TerminalPasswordReader)
            .await
    }
}

impl CreateUser {
    /// Execute the task using an injected terminal reader.
    ///
    /// # Errors
    ///
    /// Returns a safe task failure for missing variables, unavailable terminal input, invalid
    /// input, duplicate email, password mismatch, or persistence failure.
    pub async fn run_with_reader(
        &self,
        ctx: &AppContext,
        vars: &Vars,
        reader: &mut dyn PasswordReader,
    ) -> Result<()> {
        let _database = ctx
            .shared_store
            .get::<crate::database::client::AppDatabase>()
            .ok_or_else(|| Error::string("application database is not installed"))?;
        let email = vars.cli_arg("email")?;
        let display_name = vars.cli_arg("display_name")?;
        let password = reader
            .read_password("Password: ")
            .map_err(|_| Error::string("an interactive terminal is required"))?;
        let confirmation = reader
            .read_password("Confirm password: ")
            .map_err(|_| Error::string("an interactive terminal is required"))?;
        if password != confirmation {
            return Err(Error::string("password confirmation does not match"));
        }
        let service = ctx
            .shared_store
            .get::<AuthService>()
            .ok_or_else(|| Error::string("authentication service is not installed"))?;
        let user = service
            .create_user(email, display_name, &password)
            .await
            .map_err(Error::msg)?;
        let normalized = crate::models::auth::NormalizedEmail::parse(email).map_err(Error::msg)?;
        println!(
            "created {} {} success",
            user.id.as_str(),
            normalized.as_str()
        );
        Ok(())
    }
}
