use crate::logged_command::LoggedCommand;
use agent_interface::*;
use async_trait::async_trait;
use csv::ReaderBuilder;
use download::Downloader;
use serde::Deserialize;
use std::path::Path;
use std::{path::PathBuf, process::Output};
use tokio::io::BufWriter;
use tokio::{fs::File, io::AsyncWriteExt};
use tracing::error;

#[async_trait]
pub trait Plugin {
    async fn prepare(&self, logger: &mut BufWriter<File>) -> Result<(), SoftwareError> {
        Ok(())
    }

    async fn install(
        &self,
        _module: &SoftwareModule,
        _logger: &mut BufWriter<File>,
    ) -> Result<(), SoftwareError> {
        Ok(())
    }

    async fn remove(
        &self,
        _module: &SoftwareModule,
        _logger: &mut BufWriter<File>,
    ) -> Result<(), SoftwareError> {
        Ok(())
    }

    async fn update_list(
        &self,
        _modules: &[SoftwareModuleUpdate],
        _logger: &mut BufWriter<File>,
        _maybe_plugin: Option<&str>,
    ) -> Result<(), SoftwareError> {
        Ok(())
    }

    async fn finalize(&self, _logger: &mut BufWriter<File>) -> Result<(), SoftwareError> {
        Ok(())
    }

    async fn list(
        &self,
        _logger: &mut BufWriter<File>,
    ) -> Result<Vec<SoftwareModule>, SoftwareError> {
        Ok(vec![])
    }

    async fn version(
        &self,
        module: &SoftwareModule,
        logger: &mut BufWriter<File>,
    ) -> Result<Option<String>, SoftwareError>;

    async fn apply(
        &self,
        update: &SoftwareModuleUpdate,
        logger: &mut BufWriter<File>,
        download_path: &Path,
    ) -> Result<(), SoftwareError> {
        match update.clone() {
            SoftwareModuleUpdate::Install { mut module } => {
                let module_url = module.url.clone();
                match module_url {
                    Some(url) => {
                        self.install_from_url(&mut module, &url, logger, download_path)
                            .await?
                    }
                    None => self.install(&module, logger).await?,
                }

                Ok(())
            }
            SoftwareModuleUpdate::Remove { module } => self.remove(&module, logger).await,
        }
    }

    async fn apply_all(
        &self,
        mut updates: Vec<SoftwareModuleUpdate>,
        logger: &mut BufWriter<File>,
        download_path: &Path,
        maybe_plugin: Option<&str>,
    ) -> Vec<SoftwareError> {
        let mut failed_updates = Vec::new();

        // Prepare the updates
        if let Err(prepare_error) = self.prepare(logger).await {
            failed_updates.push(prepare_error);
            return failed_updates;
        }

        // Download all modules for which a download URL is provided
        let mut downloaders = Vec::new();
        for update in updates.iter_mut() {
            let module = match update {
                SoftwareModuleUpdate::Remove { module } => module,
                SoftwareModuleUpdate::Install { module } => module,
            };
            let module_url = module.url.clone();
            if let Some(url) = module_url {
                match Self::download_from_url(module, &url, logger, download_path).await {
                    Err(prepare_error) => {
                        failed_updates.push(prepare_error);
                        break;
                    }
                    Ok(downloader) => downloaders.push(downloader),
                }
            }
        }

        // Execute the updates
        if failed_updates.is_empty() {
            let outcome = self.update_list(&updates, logger, maybe_plugin).await;
            if let Err(SoftwareError::UpdateListNotSupported(_)) = outcome {
                for update in updates.iter() {
                    if let Err(error) = self.apply(update, logger, download_path).await {
                        failed_updates.push(error);
                    };
                }
            } else if let Err(update_list_error) = outcome {
                failed_updates.push(update_list_error);
            }
        }

        // Finalize the updates
        if let Err(finalize_error) = self.finalize(logger).await {
            failed_updates.push(finalize_error);
        }

        // Cleanup all the downloaded modules
        for downloader in downloaders {
            if let Err(cleanup_error) = Self::cleanup_downloaded_artefacts(downloader, logger).await
            {
                failed_updates.push(cleanup_error);
            }
        }

        failed_updates
    }

    // async fn apply_tedge(
    //     &self,
    //     mut updates: Vec<SoftwareModuleUpdate>,
    //     logger: &mut BufWriter<File>,
    //     download_path: &Path,
    //     maybe_plugin: Option<&str>,
    // ) -> Vec<SoftwareError> {
    //     let mut failed_updates = Vec::new();

    //     // Prepare the updates
    //     if let Err(prepare_error) = self.prepare(logger).await {
    //         failed_updates.push(prepare_error);
    //         return failed_updates;
    //     }

    //     // Download all modules for which a download URL is provided
    //     let mut downloaders = Vec::new();
    //     for update in updates.iter_mut() {
    //         let module = match update {
    //             SoftwareModuleUpdate::Remove { module } => module,
    //             SoftwareModuleUpdate::Install { module } => module,
    //         };
    //         let module_url = module.url.clone();
    //         if let Some(url) = module_url {
    //             match Self::download_from_url(module, &url, logger, download_path).await {
    //                 Err(prepare_error) => {
    //                     failed_updates.push(prepare_error);
    //                     break;
    //                 }
    //                 Ok(downloader) => downloaders.push(downloader),
    //             }
    //         }
    //     }

    //     // Execute the updates
    //     if failed_updates.is_empty() {
    //         let outcome = self.update_list(&updates, logger, maybe_plugin).await;
    //         if let Err(SoftwareError::UpdateListNotSupported(_)) = outcome {
    //             for update in updates.iter() {
    //                 if let Err(error) = self.apply(update, logger, download_path).await {
    //                     failed_updates.push(error);
    //                 };
    //             }
    //         } else if let Err(update_list_error) = outcome {
    //             failed_updates.push(update_list_error);
    //         }
    //     }

    //     // Finalize the updates
    //     if let Err(finalize_error) = self.finalize(logger).await {
    //         failed_updates.push(finalize_error);
    //     }

    //     // Cleanup all the downloaded modules
    //     for downloader in downloaders {
    //         if let Err(cleanup_error) = Self::cleanup_downloaded_artefacts(downloader, logger).await
    //         {
    //             failed_updates.push(cleanup_error);
    //         }
    //     }

    //     failed_updates
    // }

    async fn install_from_url(
        &self,
        module: &mut SoftwareModule,
        url: &DownloadInfo,
        logger: &mut BufWriter<File>,
        download_path: &Path,
    ) -> Result<(), SoftwareError> {
        let downloader = Self::download_from_url(module, url, logger, download_path).await?;
        let result = self.install(module, logger).await;
        Self::cleanup_downloaded_artefacts(downloader, logger).await?;

        result
    }

    async fn download_from_url(
        module: &mut SoftwareModule,
        url: &DownloadInfo,
        logger: &mut BufWriter<File>,
        download_path: &Path,
    ) -> Result<Downloader, SoftwareError> {
        let downloader = Downloader::new(&module.name, &module.version, &download_path);

        logger
            .write_all(
                format!(
                    "----- $ Downloading: {} to {} \n",
                    &url.url(),
                    &downloader.filename().to_string_lossy().to_string()
                )
                .as_bytes(),
            )
            .await?;

        if let Err(err) =
            downloader
                .download(url)
                .await
                .map_err(|err| SoftwareError::DownloadError {
                    reason: err.to_string(),
                    url: url.url().to_string(),
                })
        {
            error!("Download error: {}", &err);
            logger
                .write_all(format!("error: {}\n", &err).as_bytes())
                .await?;
            return Err(err);
        }

        module.file_path = Some(downloader.filename().to_owned());

        Ok(downloader)
    }

    async fn cleanup_downloaded_artefacts(
        downloader: Downloader,
        logger: &mut BufWriter<File>,
    ) -> Result<(), SoftwareError> {
        if let Err(err) = downloader
            .cleanup()
            .await
            .map_err(|err| SoftwareError::IoError {
                reason: err.to_string(),
            })
        {
            logger
                .write_all(format!("warn: {}\n", &err).as_bytes())
                .await?;
        }
        Ok(())
    }
}

// This struct is used for deserializing the list of modules that are returned by a plugin.
#[derive(Debug, Deserialize)]
struct ModuleInfo {
    name: String,
    #[serde(default)]
    version: Option<String>,
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
const UPDATE_LIST: &str = "update-list";
const FINALIZE: &str = "finalize";
pub const LIST: &str = "list";
const VERSION: &str = "version";

#[async_trait]
impl Plugin for ExternalPluginCommand {
    async fn prepare(&self, logger: &mut BufWriter<File>) -> Result<(), SoftwareError> {
        let command = self.command(PREPARE, None, None)?;
        let output = self.execute(command, logger).await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(SoftwareError::Prepare {
                software_type: self.name.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn install(
        &self,
        module: &SoftwareModule,
        logger: &mut BufWriter<File>,
    ) -> Result<(), SoftwareError> {
        let command = self.command(INSTALL, Some(module), None)?;
        let output = self.execute(command, logger).await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(SoftwareError::Install {
                module: module.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn remove(
        &self,
        module: &SoftwareModule,
        logger: &mut BufWriter<File>,
    ) -> Result<(), SoftwareError> {
        let command = self.command(REMOVE, Some(module), None)?;
        let output = self.execute(command, logger).await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(SoftwareError::Remove {
                module: module.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn update_list(
        &self,
        updates: &[SoftwareModuleUpdate],
        logger: &mut BufWriter<File>,
        maybe_plugin: Option<&str>,
    ) -> Result<(), SoftwareError> {
        let mut command = self.command(UPDATE_LIST, None, maybe_plugin)?;

        let mut child = command.spawn()?;
        let child_stdin =
            child
                .inner_child
                .stdin
                .as_mut()
                .ok_or_else(|| SoftwareError::IoError {
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

            child_stdin.write_all(action.as_bytes()).await?
        }

        let output = child.wait_with_output(logger).await?;
        match output.status.code() {
            Some(0) => Ok(()),
            Some(1) => Err(SoftwareError::UpdateListNotSupported(self.name.clone())),
            Some(_) => Err(SoftwareError::UpdateList {
                software_type: self.name.clone(),
                reason: self.content(output.stderr)?,
            }),
            None => Err(SoftwareError::UpdateList {
                software_type: self.name.clone(),
                reason: "Interrupted".into(),
            }),
        }
    }

    async fn finalize(&self, logger: &mut BufWriter<File>) -> Result<(), SoftwareError> {
        let command = self.command(FINALIZE, None, None)?;
        let output = self.execute(command, logger).await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(SoftwareError::Finalize {
                software_type: self.name.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn list(
        &self,
        logger: &mut BufWriter<File>,
    ) -> Result<Vec<SoftwareModule>, SoftwareError> {
        let command = self.command(LIST, None, None)?;
        let output = self.execute(command, logger).await?;
        if output.status.success() {
            Ok(deserialize_module_info(
                self.name.clone(),
                output.stdout.as_slice(),
            )?)
        } else {
            Err(SoftwareError::Plugin {
                software_type: self.name.clone(),
                reason: self.content(output.stderr)?,
            })
        }
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

pub fn deserialize_module_info(
    module_type: String,
    input: impl std::io::Read,
) -> Result<Vec<SoftwareModule>, SoftwareError> {
    let mut records = ReaderBuilder::new()
        .has_headers(false)
        .delimiter(b'\t')
        .flexible(true)
        .from_reader(input);
    let mut software_list = Vec::new();
    for module in records.deserialize() {
        let minfo: ModuleInfo = module?;
        software_list.push(SoftwareModule {
            name: minfo.name,
            version: minfo.version,
            module_type: Some(module_type.clone()),
            file_path: None,
            url: None,
        });
    }
    Ok(software_list)
}
