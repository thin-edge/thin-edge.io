use crate::error::PluginError;
use async_trait::async_trait;
use json_sm::{SoftwareModule, SoftwareModuleUpdate, SoftwareType};
use std::{
    iter::Iterator,
    path::PathBuf,
    process::{Output, Stdio},
};
use tokio::process::Command;

#[async_trait]
pub trait Plugin {
    async fn prepare(&self) -> Result<(), PluginError>;
    async fn install(&self, module: &SoftwareModule) -> Result<(), PluginError>;
    async fn remove(&self, module: &SoftwareModule) -> Result<(), PluginError>;
    async fn finalize(&self) -> Result<(), PluginError>;
    async fn list(&self) -> Result<Vec<SoftwareModule>, PluginError>;
    async fn version(&self, module: &SoftwareModule) -> Result<Option<String>, PluginError>;

    async fn apply(&self, update: &SoftwareModuleUpdate) -> Result<(), PluginError> {
        match update {
            SoftwareModuleUpdate::Install { module } => self.install(&module).await,
            SoftwareModuleUpdate::Remove { module } => self.remove(&module).await,
        }
    }

    async fn apply_all(&self, updates: &[SoftwareModuleUpdate]) -> Vec<SoftwareModuleUpdateResult> {
        let mut failed_updates = Vec::new();

        // TODO: Implement proper handling of results here.
        let _ = self.prepare().await;

        for update in updates.iter() {
            if let Err(error) = self.apply(update).await {
                let () = failed_updates.push(SoftwareModuleUpdateResult {
                    update: update.clone(),
                    error: Some(error),
                });
            };
        }

        // TODO: Implement proper handling of results here.
        let _ = self.finalize().await;

        failed_updates
    }
}

#[derive(Debug)]
pub struct ExternalPluginCommand {
    pub name: SoftwareType,
    pub path: PathBuf,
}

impl ExternalPluginCommand {
    pub fn new(name: impl Into<SoftwareType>, path: impl Into<PathBuf>) -> ExternalPluginCommand {
        ExternalPluginCommand {
            name: name.into(),
            path: path.into(),
        }
    }

    pub fn command(
        &self,
        action: &str,
        maybe_module: Option<&SoftwareModule>,
    ) -> Result<Command, PluginError> {
        let mut command = Command::new(&self.path);
        command.arg(action);

        if let Some(module) = maybe_module {
            // self.check_module_type(module)?;
            command.arg(&module.name);
            if let Some(ref version) = module.version {
                command.arg(version);
            }
        }

        command
            .current_dir("/tmp")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        Ok(command)
    }

    pub async fn execute(&self, mut command: Command) -> Result<Output, PluginError> {
        let output = command
            .output()
            .await
            .map_err(|err| self.plugin_error(err))?;
        Ok(output)
    }

    pub fn content(&self, bytes: Vec<u8>) -> Result<String, PluginError> {
        String::from_utf8(bytes).map_err(|err| self.plugin_error(err))
    }

    pub fn plugin_error(&self, err: impl std::fmt::Display) -> PluginError {
        PluginError::Plugin {
            software_type: self.name.clone(),
            reason: format!("{}", err),
        }
    }
}

const PREPARE: &str = "prepare";
const INSTALL: &str = "install";
const REMOVE: &str = "remove";
const FINALIZE: &str = "finalize";
const LIST: &str = "list";
const VERSION: &str = "version";

#[async_trait]
impl Plugin for ExternalPluginCommand {
    async fn prepare(&self) -> Result<(), PluginError> {
        let command = self.command(PREPARE, None)?;
        let output = self.execute(command).await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(PluginError::Prepare {
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn install(&self, module: &SoftwareModule) -> Result<(), PluginError> {
        let command = self.command(INSTALL, Some(module))?;
        let output = self.execute(command).await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(PluginError::Install {
                module: module.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn remove(&self, module: &SoftwareModule) -> Result<(), PluginError> {
        let command = self.command(REMOVE, Some(module))?;
        let output = self.execute(command).await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(PluginError::Uninstall {
                module: module.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn finalize(&self) -> Result<(), PluginError> {
        let command = self.command(FINALIZE, None)?;
        let output = self.execute(command).await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(PluginError::Finalize {
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn list(&self) -> Result<Vec<SoftwareModule>, PluginError> {
        let command = self.command(LIST, None)?;
        let output = self.execute(command).await?;

        if output.status.success() {
            let mut software_list = Vec::new();
            let mystr = output.stdout;

            mystr
                .split(|n: &u8| n.is_ascii_whitespace())
                .filter(|split| !split.is_empty())
                .for_each(|split: &[u8]| {
                    let software_json_line = std::str::from_utf8(split).unwrap();
                    let software_module =
                        serde_json::from_str::<SoftwareModule>(software_json_line).unwrap();
                    software_list.push(software_module);
                });

            dbg!(&software_list);
            Ok(software_list)
        } else {
            Err(PluginError::Plugin {
                software_type: self.name.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn version(&self, module: &SoftwareModule) -> Result<Option<String>, PluginError> {
        let command = self.command(VERSION, Some(module))?;
        let output = self.execute(command).await?;

        if output.status.success() {
            let version = String::from(self.content(output.stdout)?.trim());
            if version.is_empty() {
                Ok(None)
            } else {
                Ok(Some(version))
            }
        } else {
            Err(PluginError::Plugin {
                software_type: self.name.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }
}
