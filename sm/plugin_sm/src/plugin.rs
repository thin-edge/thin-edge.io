use async_trait::async_trait;
use download::download::download;
use json_sm::*;
use std::{
    iter::Iterator,
    path::{Path, PathBuf},
    process::{Output, Stdio},
    sync::Arc,
};
use tokio::process::Command;

#[async_trait]
pub trait Plugin {
    async fn prepare(&self) -> Result<(), SoftwareError>;
    async fn install(&self, module: &SoftwareModule) -> Result<(), SoftwareError>;
    async fn remove(&self, module: &SoftwareModule) -> Result<(), SoftwareError>;
    async fn finalize(&self) -> Result<(), SoftwareError>;
    async fn list(&self) -> Result<Vec<SoftwareModule>, SoftwareError>;
    async fn version(&self, module: &SoftwareModule) -> Result<Option<String>, SoftwareError>;

    async fn download(
        &self,
        name: &str,
        version: &Option<String>,
        url: &DownloadInfo,
    ) -> Result<PathBuf, SoftwareError>;

    async fn cleanup_downloaded(&self, path: Arc<Path>) -> Result<(), SoftwareError>;

    async fn apply(&self, update: &SoftwareModuleUpdate) -> Result<(), SoftwareError> {
        match update.clone() {
            SoftwareModuleUpdate::Install { mut module } => {
                if let Some(url) = &module.url {
                    module.file_path =
                        Some(self.download(&module.name, &module.version, url).await?);
                }
                self.install(&module).await?;

                if let Some(path) = module.file_path {
                    self.cleanup_downloaded(path.into()).await?;
                    module.file_path = None;
                }

                Ok(())
            }
            SoftwareModuleUpdate::Remove { module } => self.remove(&module).await,
        }
    }

    async fn apply_all(&self, updates: Vec<SoftwareModuleUpdate>) -> Vec<SoftwareError> {
        let mut failed_updates = Vec::new();

        if let Err(prepare_error) = self.prepare().await {
            failed_updates.push(prepare_error);
            return failed_updates;
        }

        for update in updates.iter() {
            if let Err(error) = self.apply(update).await {
                failed_updates.push(error);
            };
        }

        if let Err(finalize_error) = self.finalize().await {
            failed_updates.push(finalize_error);
        }

        failed_updates
    }
}

#[derive(Debug)]
pub struct ExternalPluginCommand {
    pub name: SoftwareType,
    pub path: PathBuf,
    pub sudo: Option<PathBuf>,
}

impl ExternalPluginCommand {
    pub fn new(name: impl Into<SoftwareType>, path: impl Into<PathBuf>) -> ExternalPluginCommand {
        ExternalPluginCommand {
            name: name.into(),
            path: path.into(),
            sudo: Some("sudo".into()),
        }
    }

    pub fn command(
        &self,
        action: &str,
        maybe_module: Option<&SoftwareModule>,
    ) -> Result<Command, SoftwareError> {
        let mut command = if let Some(sudo) = &self.sudo {
            let mut command = Command::new(&sudo);
            command.arg(&self.path);
            command
        } else {
            Command::new(&self.path)
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

        command
            .current_dir("/tmp")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        Ok(command)
    }

    pub async fn execute(&self, mut command: Command) -> Result<Output, SoftwareError> {
        let output = command
            .output()
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
            Some(name) if name == DEFAULT => Ok(()),
            Some(name) => Err(SoftwareError::WrongModuleType {
                actual: self.name.clone(),
                expected: name.clone(),
            }),
            None => Ok(()), // A software module without a type can be handled by any plugin that's configured as default plugin
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
    async fn prepare(&self) -> Result<(), SoftwareError> {
        let command = self.command(PREPARE, None)?;
        let output = self.execute(command).await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(SoftwareError::Prepare {
                software_type: self.name.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn install(&self, module: &SoftwareModule) -> Result<(), SoftwareError> {
        let command = self.command(INSTALL, Some(module))?;
        let output = self.execute(command).await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(SoftwareError::Install {
                module: module.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn remove(&self, module: &SoftwareModule) -> Result<(), SoftwareError> {
        let command = self.command(REMOVE, Some(module))?;
        let output = self.execute(command).await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(SoftwareError::Remove {
                module: module.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn finalize(&self) -> Result<(), SoftwareError> {
        let command = self.command(FINALIZE, None)?;
        let output = self.execute(command).await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(SoftwareError::Finalize {
                software_type: self.name.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn list(&self) -> Result<Vec<SoftwareModule>, SoftwareError> {
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
                    let mut software_module =
                        serde_json::from_str::<SoftwareModule>(software_json_line).unwrap();
                    software_module.module_type = Some(self.name.clone());
                    software_list.push(software_module);
                });

            Ok(software_list)
        } else {
            Err(SoftwareError::Plugin {
                software_type: self.name.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn version(&self, module: &SoftwareModule) -> Result<Option<String>, SoftwareError> {
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
            Err(SoftwareError::Plugin {
                software_type: self.name.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn download(
        &self,
        name: &str,
        version: &Option<String>,
        url: &DownloadInfo,
    ) -> Result<PathBuf, SoftwareError> {
        let mut filename = name.to_string();
        if let Some(version) = version {
            filename.push('_');
            filename.push_str(version.as_str());
        }

        let downloaded_path = match download(url, Path::new("/tmp"), &filename).await {
            Ok(path) => path,
            Err(err) => {
                return Err(SoftwareError::DownloadError {
                    reason: err.to_string(),
                    url: url.url().to_string(),
                });
            }
        };

        Ok(downloaded_path)
    }

    async fn cleanup_downloaded(&self, path: Arc<Path>) -> Result<(), SoftwareError> {
        let _res = tokio::fs::remove_file(path).await;
        Ok(())
    }
}
