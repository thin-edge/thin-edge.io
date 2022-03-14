use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use agent_interface::{SoftwareError, SoftwareModule, SoftwareModuleUpdate, SoftwareType};
use async_trait::async_trait;
use serde::Deserialize;
use tokio::{fs::File, io::BufWriter};

use crate::{logged_command::LoggedCommand, plugin::Plugin};

const UPDATE_LIST: &str = "update-list";
const VERSION: &str = "version";

#[derive(Debug)]
pub struct Updater {
    pub name: SoftwareType,
    pub path: PathBuf,
    pub sudo: Option<PathBuf>,
}

impl Updater {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            name: "tedge_updater".into(),
            path: path.into(),
            sudo: Some("sudo".into()),
        }
    }

    pub fn command(
        &self,
        action: &str,
        maybe_module: Option<&SoftwareModule>,
        maybe_plugin: Option<&str>,
    ) -> Result<LoggedCommand, SoftwareError> {
        let mut command = if let Some(sudo) = &self.sudo {
            let mut command = LoggedCommand::new(sudo);
            command.arg(&self.path);
            command
        } else {
            LoggedCommand::new(&self.path)
        };
        command.arg(action);

        if let Some(module) = maybe_module {
            self.check_module_type(module)?;
            command.arg(&module.name);
            if let Some(ref version) = module.version {
                command.arg("--module-version");
                command.arg(version);
            }

            if let Some(ref path) = module.file_path {
                command.arg("--file");
                command.arg(path);
            }
        }

        if let Some(plugin) = maybe_plugin {
            command.arg("--plugin-name");
            command.arg(plugin);
        }

        Ok(command)
    }

    pub async fn execute(
        &self,
        command: LoggedCommand,
        logger: &mut BufWriter<File>,
    ) -> Result<Output, SoftwareError> {
        let output = command
            .execute(logger)
            .await
            .map_err(|err| self.plugin_error(err))?;
        Ok(output)
    }

    pub fn content(&self, bytes: Vec<u8>) -> Result<String, SoftwareError> {
        String::from_utf8(bytes).map_err(|err| self.plugin_error(err))
    }

    pub fn plugin_error(&self, err: impl std::fmt::Display) -> SoftwareError {
        SoftwareError::Plugin {
            software_type: self.name.clone(),
            reason: format!("{}", err),
        }
    }

    /// This test validates if an incoming module can be handled by it, by matching the module type with the plugin type
    pub fn check_module_type(&self, module: &SoftwareModule) -> Result<(), SoftwareError> {
        match &module.module_type {
            Some(name) if name == &self.name.clone() => Ok(()),
            // Some(name) if name == DEFAULT => Ok(()),
            Some(name) => Err(SoftwareError::WrongModuleType {
                actual: self.name.clone(),
                expected: name.clone(),
            }),
            None => Ok(()), // A software module without a type can be handled by any plugin that's configured as default plugin
        }
    }
}
#[async_trait]
impl Plugin for Updater {
    async fn update_list(
        &self,
        updates: &[SoftwareModuleUpdate],
        _logger: &mut BufWriter<File>,
        maybe_plugin: Option<&str>,
    ) -> Result<(), SoftwareError> {
        use fork::daemon;

        match daemon(false, true) {
            Ok(_a) => {
                let mut command = if let Some(sudo) = &self.sudo {
                    let mut command = Command::new(sudo);
                    command.arg(&self.path);
                    command
                } else {
                    Command::new(&self.path)
                };
                command.arg(UPDATE_LIST);

                if let Some(plugin) = maybe_plugin {
                    command.arg("--plugin-name");
                    command.arg(plugin);
                }

                let mut child = command.spawn()?;
                let child_stdin = child.stdin.as_mut().ok_or_else(|| SoftwareError::IoError {
                    reason: "Plugin stdin unavailable".into(),
                })?;

                for update in updates {
                    let action = match update {
                        SoftwareModuleUpdate::Install { module } => {
                            format!(
                                "install\t{}\t{}\t{}\n",
                                module.name,
                                module.version.clone().map_or("".into(), |v| v),
                                module.file_path.clone().map_or("".into(), |v| v
                                    .to_str()
                                    .map_or("".into(), |u| u.to_string()))
                            )
                        }

                        SoftwareModuleUpdate::Remove { module } => {
                            format!(
                                "remove\t{}\t{}\t\n",
                                module.name,
                                module.version.clone().map_or("".into(), |v| v),
                            )
                        }
                    };

                    let _ = child_stdin.write_all(action.as_bytes());
                }
                let _ = child.wait();
            }
            Err(err) => {
                dbg!(err);
            }
        }

        Ok(())
    }

    async fn version(
        &self,
        module: &SoftwareModule,
        logger: &mut BufWriter<File>,
    ) -> Result<Option<String>, SoftwareError> {
        let command = self.command(VERSION, Some(module), None)?;
        let output = self.execute(command, logger).await?;

        if output.status.success() {
            let version = String::from(self.content(output.stdout)?.trim());
            if version.is_empty() {
                Ok(None)
            } else {
                Ok(Some(version))
            }
        } else {
            Err(SoftwareError::Plugin {
                software_type: self.name.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }
}

#[derive(Debug, Deserialize)]
struct Components {
    components: Vec<String>,
}

fn get_tedge_components(path: impl AsRef<Path>) -> Result<Components, SoftwareError> {
    match fs::read("/etc/tedge/tedge_components.toml") {
        Ok(bytes) => toml::from_slice::<Components>(bytes.as_slice()).map_err(|err| {
            SoftwareError::FromToml {
                reason: err.to_string(),
            }
        }),

        Err(err) => Err(SoftwareError::IoError {
            reason: err.to_string(),
        }),
    }
}
