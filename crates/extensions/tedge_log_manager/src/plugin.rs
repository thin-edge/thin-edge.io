use crate::error::LogManagementError;
use camino::Utf8Path;
use log::warn;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::Output;
use std::sync::Arc;
use tedge_api::CommandLog;
use tedge_api::LoggedCommand;
use tedge_config::SudoCommandBuilder;
use time::OffsetDateTime;

pub const LIST: &str = "list";
const GET: &str = "get";

#[derive(Debug)]
pub struct ExternalPluginCommand {
    pub name: String,
    pub path: PathBuf,
    pub sudo: SudoCommandBuilder,
    tmp_dir: Arc<Utf8Path>,
}

impl ExternalPluginCommand {
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

    pub(crate) async fn list(
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

        let mut log_types = vec![];
        for line in stdout.lines() {
            log_types.push(line.trim().to_string())
        }

        Ok(log_types)
    }

    pub(crate) async fn get(
        &self,
        log_type: &str,
        output_file_path: &Utf8Path,
        since: Option<OffsetDateTime>,
        until: Option<OffsetDateTime>,
        filter_text: Option<&str>,
        lines: Option<usize>,
    ) -> Result<(), LogManagementError> {
        let mut command = self.command(GET)?;
        command.arg(log_type);

        if let Some(since_time) = since {
            command.arg("--since");
            command.arg(since_time.unix_timestamp().to_string());
        }

        if let Some(until_time) = until {
            command.arg("--until");
            command.arg(until_time.unix_timestamp().to_string());
        }

        if let Some(line_count) = lines {
            command.arg("--tail");
            command.arg(line_count.to_string());
        }

        warn!("Fetching log using command: {}", command);
        let output = self.execute(command, None).await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(self.plugin_error(format!("Get command error: {}", stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let file = File::create(output_file_path).map_err(|err| {
            self.plugin_error(format!(
                "Failed to create plugin output file at {} due to {}",
                output_file_path, err
            ))
        })?;
        let mut writer = std::io::BufWriter::new(&file);

        for line in stdout.lines() {
            if let Some(filter) = filter_text {
                if !line.is_empty() && !line.contains(filter) {
                    continue;
                }
            }

            writeln!(writer, "{}", line).map_err(|err| {
                self.plugin_error(format!(
                    "Failed to write plugin output to {} due to {}",
                    output_file_path, err
                ))
            })?;
        }

        writer.flush().map_err(|err| {
            self.plugin_error(format!(
                "Failed to flush plugin output to {} due to {}",
                output_file_path, err
            ))
        })?;
        file.sync_all().map_err(|err| {
            self.plugin_error(format!(
                "Failed to sync plugin output to {} due to {}",
                output_file_path, err
            ))
        })?;

        Ok(())
    }
}
