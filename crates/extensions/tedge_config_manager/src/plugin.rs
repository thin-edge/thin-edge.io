use crate::error::ConfigManagementError;
use camino::Utf8Path;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::Output;
use std::sync::Arc;
use tedge_api::CommandLog;
use tedge_api::LoggedCommand;
use tedge_config::SudoCommandBuilder;

pub const LIST: &str = "list";
const GET: &str = "get";
const SET: &str = "set";

#[derive(Debug, Clone)]
pub struct ExternalPlugin {
    pub name: String,
    pub path: PathBuf,
    pub sudo: SudoCommandBuilder,
    tmp_dir: Arc<Utf8Path>,
}

impl ExternalPlugin {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        sudo: SudoCommandBuilder,
        tmp_dir: Arc<Utf8Path>,
    ) -> ExternalPlugin {
        ExternalPlugin {
            name: name.into(),
            path: path.into(),
            sudo,
            tmp_dir,
        }
    }

    pub fn command(&self, action: &str) -> Result<LoggedCommand, ConfigManagementError> {
        let mut command = self.sudo.command(&self.path);
        command.arg(action);

        let command = LoggedCommand::from_command(command, self.tmp_dir.as_ref());

        Ok(command)
    }

    pub async fn execute(
        &self,
        command: LoggedCommand,
        command_log: Option<&mut CommandLog>,
    ) -> Result<Output, ConfigManagementError> {
        let output = command
            .execute(command_log)
            .await
            .map_err(|err| self.plugin_error(err))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ConfigManagementError::PluginError {
                plugin_name: self.name.clone(),
                reason: format!("Command execution failed: {}", stderr),
            });
        }

        Ok(output)
    }

    pub fn plugin_error(&self, err: impl std::fmt::Display) -> ConfigManagementError {
        ConfigManagementError::PluginError {
            plugin_name: self.name.clone(),
            reason: format!("{}", err),
        }
    }

    pub(crate) async fn list(
        &self,
        command_log: Option<&mut CommandLog>,
    ) -> Result<Vec<String>, ConfigManagementError> {
        let command = self.command(LIST)?;
        let output = self.execute(command, command_log).await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ConfigManagementError::PluginError {
                plugin_name: self.name.clone(),
                reason: format!("List command failed: {}", stderr),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let config_types = stdout
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();

        Ok(config_types)
    }

    pub(crate) async fn get(
        &self,
        config_type: &str,
        target_file_path: &Utf8Path,
    ) -> Result<(), ConfigManagementError> {
        let mut command = self.command(GET)?;
        command.arg(config_type);

        let output = self.execute(command, None).await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let file = File::create(target_file_path).map_err(|err| {
            self.plugin_error(format!(
                "Failed to create plugin output file at {target_file_path} due to {err}",
            ))
        })?;
        let mut writer = std::io::BufWriter::new(&file);

        write!(writer, "{}", stdout).map_err(|err| {
            self.plugin_error(format!(
                "Failed to write plugin output to {target_file_path} due to {err}",
            ))
        })?;

        writer.flush().map_err(|err| {
            self.plugin_error(format!(
                "Failed to flush plugin output to {target_file_path} due to {err}",
            ))
        })?;
        file.sync_all().map_err(|err| {
            self.plugin_error(format!(
                "Failed to sync plugin output to {target_file_path} due to {err}",
            ))
        })?;

        Ok(())
    }

    pub(crate) async fn set(
        &self,
        config_type: &str,
        config_file_path: &Utf8Path,
    ) -> Result<(), ConfigManagementError> {
        let mut command = self.command(SET)?;
        command.arg(config_type);
        command.arg(config_file_path);

        self.execute(command, None).await?;

        Ok(())
    }
}
