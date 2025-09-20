use crate::error::LogManagementError;
use async_trait::async_trait;
use camino::Utf8Path;
use log::warn;
use std::path::PathBuf;
use std::process::Output;
use std::sync::Arc;
use tedge_api::CommandLog;
use tedge_api::LoggedCommand;
use tedge_config::SudoCommandBuilder;
use time::OffsetDateTime;

#[async_trait]
pub trait Plugin {
    async fn list(
        &self,
        command_log: Option<&mut CommandLog>,
    ) -> Result<Vec<String>, LogManagementError>;

    async fn get(
        &self,
        log_type: &str,
        temp_file_path: &Utf8Path,
        since: Option<OffsetDateTime>,
        until: Option<OffsetDateTime>,
        filter_text: Option<&str>,
        lines: Option<usize>,
    ) -> Result<(), LogManagementError>;
}

#[derive(Debug)]
pub struct ExternalPluginCommand {
    pub name: String,
    pub path: PathBuf,
    pub sudo: SudoCommandBuilder,
    tmp_dir: Arc<Utf8Path>,
}

impl ExternalPluginCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        sudo: SudoCommandBuilder,
        tmp_dir: Arc<Utf8Path>,
    ) -> ExternalPluginCommand {
        ExternalPluginCommand {
            name: name.into(),
            path: path.into(),
            sudo,
            tmp_dir,
        }
    }

    pub fn command(&self, action: &str) -> Result<LoggedCommand, LogManagementError> {
        let mut command = self.sudo.command(&self.path);
        command.arg(action);

        let command = LoggedCommand::from_command(command, self.tmp_dir.as_ref());

        Ok(command)
    }

    pub async fn execute(
        &self,
        command: LoggedCommand,
        command_log: Option<&mut CommandLog>,
    ) -> Result<Output, LogManagementError> {
        let output = command
            .execute(command_log)
            .await
            .map_err(|err| self.plugin_error(err))?;
        Ok(output)
    }

    pub fn plugin_error(&self, err: impl std::fmt::Display) -> LogManagementError {
        LogManagementError::PluginError {
            plugin_name: self.name.clone(),
            reason: format!("{}", err),
        }
    }
}

pub const LIST: &str = "list";
const GET: &str = "get";

#[async_trait]
impl Plugin for ExternalPluginCommand {
    async fn list(
        &self,
        command_log: Option<&mut CommandLog>,
    ) -> Result<Vec<String>, LogManagementError> {
        let command = self.command(LIST)?;
        warn!("Listing log types using plugin command: {}", command);
        let output = self.execute(command, command_log).await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LogManagementError::PluginError {
                plugin_name: self.name.clone(),
                reason: format!("List command failed: {}", stderr),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let log_types = stdout
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();

        Ok(log_types)
    }

    async fn get(
        &self,
        log_type: &str,
        temp_file_path: &Utf8Path,
        since: Option<OffsetDateTime>,
        until: Option<OffsetDateTime>,
        filter_text: Option<&str>,
        lines: Option<usize>,
    ) -> Result<(), LogManagementError> {
        let mut command = self.command(GET)?;
        command.arg(log_type);
        command.arg(temp_file_path);

        if let Some(since_time) = since {
            command.arg("--since");
            command.arg(
                since_time
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap(),
            );
        }

        if let Some(until_time) = until {
            command.arg("--until");
            command.arg(
                until_time
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap(),
            );
        }

        if let Some(filter) = filter_text {
            command.arg("--filter");
            command.arg(filter);
        }

        if let Some(line_count) = lines {
            command.arg("--tail");
            command.arg(line_count.to_string());
        }

        warn!("Fetching log using command: {}", command);
        let output = self.execute(command, None).await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LogManagementError::PluginError {
                plugin_name: self.name.clone(),
                reason: format!("Get command failed: {}", stderr),
            });
        }

        Ok(())
    }
}
