use async_trait::async_trait;
use certificate::CloudHttpConfig;
use csv::ReaderBuilder;
use download::Downloader;
use regex::Regex;
use reqwest::Identity;
use serde::Deserialize;
use std::error::Error;
use std::path::Path;
use std::path::PathBuf;
use std::process::Output;
use tedge_api::CommandLog;
use tedge_api::DownloadInfo;
use tedge_api::LoggedCommand;
use tedge_api::SoftwareError;
use tedge_api::SoftwareModule;
use tedge_api::SoftwareModuleUpdate;
use tedge_api::SoftwareType;
use tedge_api::DEFAULT;
use tedge_config::SudoCommandBuilder;
use tokio::io::AsyncWriteExt;
use tracing::error;
use tracing::info;

#[async_trait]
pub trait Plugin {
    async fn prepare(&self, command_log: Option<&mut CommandLog>) -> Result<(), SoftwareError>;

    async fn install(
        &self,
        module: &SoftwareModule,
        command_log: Option<&mut CommandLog>,
    ) -> Result<(), SoftwareError>;

    async fn remove(
        &self,
        module: &SoftwareModule,
        command_log: Option<&mut CommandLog>,
    ) -> Result<(), SoftwareError>;

    async fn update_list(
        &self,
        modules: &[SoftwareModuleUpdate],
        command_log: Option<&mut CommandLog>,
    ) -> Result<(), SoftwareError>;

    async fn finalize(&self, command_log: Option<&mut CommandLog>) -> Result<(), SoftwareError>;

    async fn list(
        &self,
        command_log: Option<&mut CommandLog>,
    ) -> Result<Vec<SoftwareModule>, SoftwareError>;

    async fn version(
        &self,
        module: &SoftwareModule,
        command_log: Option<&mut CommandLog>,
    ) -> Result<Option<String>, SoftwareError>;

    async fn apply(
        &self,
        update: &SoftwareModuleUpdate,
        command_log: Option<&mut CommandLog>,
        download_path: &Path,
    ) -> Result<(), SoftwareError> {
        match update.clone() {
            SoftwareModuleUpdate::Install { mut module } => {
                let module_url = module.url.clone();
                match module_url {
                    Some(url) if module.file_path.is_none() => {
                        self.install_from_url(
                            &mut module,
                            &url,
                            command_log,
                            download_path,
                            self.identity(),
                            self.cloud_root_certs().clone(),
                        )
                        .await?
                    }
                    _ => self.install(&module, command_log).await?,
                }

                Ok(())
            }
            SoftwareModuleUpdate::Remove { module } => self.remove(&module, command_log).await,
        }
    }

    fn identity(&self) -> Option<&Identity>;
    fn cloud_root_certs(&self) -> &CloudHttpConfig;

    async fn apply_all(
        &self,
        mut updates: Vec<SoftwareModuleUpdate>,
        mut command_log: Option<&mut CommandLog>,
        download_path: &Path,
    ) -> Vec<SoftwareError> {
        let mut failed_updates = Vec::new();

        // Prepare the updates
        if let Err(prepare_error) = self.prepare(command_log.as_deref_mut()).await {
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
                match Self::download_from_url(
                    module,
                    &url,
                    command_log.as_deref_mut(),
                    download_path,
                    self.identity(),
                    self.cloud_root_certs().clone(),
                )
                .await
                {
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
            let outcome = self.update_list(&updates, command_log.as_deref_mut()).await;
            if let Err(err @ SoftwareError::UpdateListNotSupported(_)) = outcome {
                info!("{err}");
                for update in updates.iter() {
                    if let Err(error) = self
                        .apply(update, command_log.as_deref_mut(), download_path)
                        .await
                    {
                        failed_updates.push(error);
                    };
                }
            } else if let Err(update_list_error) = outcome {
                failed_updates.push(update_list_error);
            }
        }

        // Finalize the updates
        if let Err(finalize_error) = self.finalize(command_log.as_deref_mut()).await {
            failed_updates.push(finalize_error);
        }

        // Cleanup all the downloaded modules
        for downloader in downloaders {
            if let Err(cleanup_error) =
                Self::cleanup_downloaded_artefacts(downloader, command_log.as_deref_mut()).await
            {
                failed_updates.push(cleanup_error);
            }
        }

        failed_updates
    }

    async fn install_from_url(
        &self,
        module: &mut SoftwareModule,
        url: &DownloadInfo,
        mut command_log: Option<&mut CommandLog>,
        download_path: &Path,
        identity: Option<&Identity>,
        cloud_root_certs: CloudHttpConfig,
    ) -> Result<(), SoftwareError> {
        let downloader = Self::download_from_url(
            module,
            url,
            command_log.as_deref_mut(),
            download_path,
            identity,
            cloud_root_certs,
        )
        .await?;
        let result = self.install(module, command_log.as_deref_mut()).await;
        Self::cleanup_downloaded_artefacts(downloader, command_log).await?;

        result
    }

    async fn download_from_url(
        module: &mut SoftwareModule,
        url: &DownloadInfo,
        mut command_log: Option<&mut CommandLog>,
        download_path: &Path,
        identity: Option<&Identity>,
        cloud_root_certs: CloudHttpConfig,
    ) -> Result<Downloader, SoftwareError> {
        let sm_path = sm_path(&module.name, &module.version, download_path);
        let downloader =
            Downloader::new(sm_path, identity.map(|id| id.to_owned()), cloud_root_certs);

        if let Some(ref mut logger) = command_log {
            logger
                .write(
                    format!(
                        "----- $ Downloading: {} to {} \n",
                        &url.url(),
                        &downloader.filename().to_string_lossy().to_string()
                    )
                    .as_bytes(),
                )
                .await?;
        }

        if let Err(err) =
            downloader
                .download(url)
                .await
                .map_err(|err| SoftwareError::DownloadError {
                    reason: err.to_string(),
                    source_err: err.source().map(|e| e.to_string()).unwrap_or_default(),
                    url: url.url().to_string(),
                })
        {
            error!("Download error: {err:#?}");
            if let Some(ref mut logger) = command_log {
                logger
                    .write(format!("error: {}\n", &err).as_bytes())
                    .await?;
            }
            return Err(err);
        }

        module.file_path = Some(downloader.filename().to_owned());

        Ok(downloader)
    }

    async fn cleanup_downloaded_artefacts(
        downloader: Downloader,
        command_log: Option<&mut CommandLog>,
    ) -> Result<(), SoftwareError> {
        if let Err(err) = downloader
            .cleanup()
            .await
            .map_err(|err| SoftwareError::IoError {
                reason: err.to_string(),
            })
        {
            if let Some(logger) = command_log {
                logger.write(format!("warn: {}\n", &err).as_bytes()).await?;
            }
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
    pub sudo: SudoCommandBuilder,
    pub max_packages: u32,
    exclude: Option<String>,
    include: Option<String>,
    identity: Option<Identity>,
    cloud_root_certs: CloudHttpConfig,
}

impl ExternalPluginCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: impl Into<SoftwareType>,
        path: impl Into<PathBuf>,
        sudo: SudoCommandBuilder,
        max_packages: u32,
        exclude: Option<String>,
        include: Option<String>,
        identity: Option<Identity>,
        cloud_root_certs: CloudHttpConfig,
    ) -> ExternalPluginCommand {
        ExternalPluginCommand {
            name: name.into(),
            path: path.into(),
            sudo,
            max_packages,
            exclude,
            include,
            identity,
            cloud_root_certs,
        }
    }

    pub fn command(
        &self,
        action: &str,
        maybe_module: Option<&SoftwareModule>,
    ) -> Result<LoggedCommand, SoftwareError> {
        let mut command = self.sudo.command(&self.path);
        command.arg(action);

        let mut command = LoggedCommand::from(command);

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

        Ok(command)
    }

    pub async fn execute(
        &self,
        command: LoggedCommand,
        command_log: Option<&mut CommandLog>,
    ) -> Result<Output, SoftwareError> {
        let output = command
            .execute(command_log)
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
    async fn prepare(&self, command_log: Option<&mut CommandLog>) -> Result<(), SoftwareError> {
        let command = self.command(PREPARE, None)?;
        let output = self.execute(command, command_log).await?;

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
        command_log: Option<&mut CommandLog>,
    ) -> Result<(), SoftwareError> {
        let command = self.command(INSTALL, Some(module))?;
        let output = self.execute(command, command_log).await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(SoftwareError::Install {
                module: Box::new(module.clone()),
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn remove(
        &self,
        module: &SoftwareModule,
        command_log: Option<&mut CommandLog>,
    ) -> Result<(), SoftwareError> {
        let command = self.command(REMOVE, Some(module))?;
        let output = self.execute(command, command_log).await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(SoftwareError::Remove {
                module: Box::new(module.clone()),
                reason: self.content(output.stderr)?,
            })
        }
    }

    async fn update_list(
        &self,
        updates: &[SoftwareModuleUpdate],
        command_log: Option<&mut CommandLog>,
    ) -> Result<(), SoftwareError> {
        let mut command = self.command(UPDATE_LIST, None)?;

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

            child_stdin.write_all(action.as_bytes()).await?;
            child_stdin.flush().await?;
        }

        let output = child.wait_with_output(command_log).await?;
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

    async fn finalize(&self, command_log: Option<&mut CommandLog>) -> Result<(), SoftwareError> {
        let command = self.command(FINALIZE, None)?;
        let output = self.execute(command, command_log).await?;

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
        command_log: Option<&mut CommandLog>,
    ) -> Result<Vec<SoftwareModule>, SoftwareError> {
        let command = self.command(LIST, None)?;
        let output = self.execute(command, command_log).await?;
        if output.status.success() {
            let filtered_output = match (&self.exclude, &self.include) {
                (None, None) => output.stdout,
                _ => {
                    // If no exclude pattern is given, exclude everything (except what matches the include pattern)
                    let exclude_filter =
                        Regex::new(self.exclude.as_ref().unwrap_or(&r".*".to_string()))?;
                    // If no include pattern is given, include nothing (except what doesn't match the exclude pattern)
                    let include_filter =
                        Regex::new(self.include.as_ref().unwrap_or(&r"^$".to_string()))?;

                    output
                        .stdout
                        .split_inclusive(|c| *c == b'\n')
                        .filter_map(|line| std::str::from_utf8(line).ok())
                        .filter(|line| {
                            line.split_once('\t').is_some_and(|(name, _)| {
                                include_filter.is_match(name) || !exclude_filter.is_match(name)
                            })
                        })
                        .flat_map(|line| line.as_bytes().to_vec())
                        .collect()
                }
            };

            // If max_packages is set to an invalid value, use 0 which represents all
            // of the content, don't bother filtering the content when all of it will
            // be included anyway
            let max_packages = usize::try_from(self.max_packages).unwrap_or(0);
            let last_char = match max_packages {
                0 => 0,
                _ => String::from_utf8(filtered_output.as_slice().to_vec())
                    .unwrap_or_default()
                    .char_indices()
                    .filter(|(_, c)| *c == '\n')
                    .nth(max_packages - 1)
                    .map(|(i, _)| i)
                    .unwrap_or_default(),
            };

            Ok(deserialize_module_info(
                self.name.clone(),
                match last_char {
                    0 => &filtered_output[..],
                    _ => &filtered_output[..=last_char],
                },
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
        command_log: Option<&mut CommandLog>,
    ) -> Result<Option<String>, SoftwareError> {
        let command = self.command(VERSION, Some(module))?;
        let output = self.execute(command, command_log).await?;

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

    fn identity(&self) -> Option<&Identity> {
        self.identity.as_ref()
    }

    fn cloud_root_certs(&self) -> &CloudHttpConfig {
        &self.cloud_root_certs
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

pub fn sm_path(name: &str, version: &Option<String>, target_dir_path: impl AsRef<Path>) -> PathBuf {
    let mut filename = name.to_string();
    if let Some(version) = version {
        filename.push('_');
        filename.push_str(version.as_str());
    }

    target_dir_path.as_ref().join(sanitize_filename(&filename))
}

fn sanitize_filename(filename: &str) -> String {
    let mut result = String::new();
    filename.chars().for_each(|c| {
        if matches!(c as u8, b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' |  b'-' | b'.' | b'_' | b'~') {
            result.push(c)
        } else {
            result.push_str(&format!("%{:x?}", c as u8))
        }
    });
    result
}
