use crate::error::LogManagementError;
use camino::Utf8Path;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::process::Output;
use std::sync::Arc;
use tedge_api::CommandLog;
use tedge_api::LoggedCommand;
use tedge_config::SudoCommandBuilder;
use time::OffsetDateTime;
use tracing::debug;

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

    pub async fn status(&self, command: LoggedCommand) -> Result<ExitStatus, LogManagementError> {
        let status = command
            .status()
            .await
            .map_err(|err| self.plugin_error(err))?;
        Ok(status)
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
    ) -> Result<BTreeSet<String>, LogManagementError> {
        let command = self.command(LIST)?;
        debug!(
            target: "log plugins",
            "Listing log types using plugin command: {}", command
        );
        let output = self.execute(command, command_log).await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LogManagementError::PluginError {
                plugin_name: self.name.clone(),
                reason: format!("List command failed: {}", stderr),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        let mut log_types = BTreeSet::new();
        for line in stdout.lines() {
            log_types.insert(line.trim().to_string());
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

        let stdout_file =
            tempfile::NamedTempFile::new_in(self.tmp_dir.as_std_path()).map_err(|err| {
                self.plugin_error(format!(
                    "Failed to create temporary file in {}: {err}",
                    tempfile::env::temp_dir().to_string_lossy()
                ))
            })?;
        let stderr_file =
            tempfile::NamedTempFile::new_in(self.tmp_dir.as_std_path()).map_err(|err| {
                self.plugin_error(format!(
                    "Failed to create temporary file in {}: {err}",
                    tempfile::env::temp_dir().to_string_lossy()
                ))
            })?;

        let stdout = OpenOptions::new()
            .create(false)
            .read(false)
            .write(true)
            .open(stdout_file.path())
            .map_err(|err| {
                self.plugin_error(format!(
                    "failed to open temporary file for writing process output '{}': {err}",
                    stdout_file.path().to_string_lossy()
                ))
            })?;
        let stderr = OpenOptions::new()
            .create(false)
            .read(false)
            .write(true)
            .open(stderr_file.path())
            .map_err(|err| {
                self.plugin_error(format!(
                    "failed to open temporary file for writing process output'{}': {err}",
                    stderr_file.path().to_string_lossy()
                ))
            })?;

        command.stdout(stdout);
        command.stderr(stderr);

        debug!(
            target: "log plugins",
            "Fetching log using command: {}", command
        );
        let status = self.status(command).await?;

        if !status.success() {
            // we assume stderr is short enough to load in memory
            let stderr = std::fs::read_to_string(stderr_file.path())?;
            return Err(self.plugin_error(format!("Get command error: {stderr}",)));
        }

        let stdout = File::open(stdout_file.path()).map_err(|err| {
            self.plugin_error(format!(
                "failed to open temporary file for writing process output'{}': {err}",
                stdout_file.path().to_string_lossy()
            ))
        })?;
        let stdout = BufReader::new(stdout);

        let file = File::create(output_file_path).map_err(|err| {
            self.plugin_error(format!(
                "Failed to create plugin output file at {} due to {}",
                output_file_path, err
            ))
        })?;
        let mut writer = std::io::BufWriter::new(&file);

        let mut filtered_lines = VecDeque::new();
        for line in stdout.lines() {
            let line = line.map_err(|err| {
                self.plugin_error(format!(
                    "error reading output file: '{}': {err:?}",
                    stdout_file.path().to_string_lossy()
                ))
            })?;

            if let Some(filter) = filter_text {
                if !filter.is_empty() && !line.contains(filter) {
                    continue;
                }
            }

            if let Some(limit) = lines {
                if filtered_lines.len() == limit {
                    filtered_lines.pop_front();
                }
            }
            filtered_lines.push_back(line);
        }

        for line in filtered_lines {
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
