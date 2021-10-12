use crate::logged_command::LoggedCommand;
use async_trait::async_trait;
use csv::ReaderBuilder;
use download::Downloader;
use json_sm::*;
use serde::Deserialize;
use std::{iter::Iterator, path::PathBuf, process::Output};
use tokio::io::BufWriter;
use tokio::{fs::File, io::AsyncWriteExt};

#[async_trait]
pub trait Plugin {
    async fn prepare(&self, logger: &mut BufWriter<File>) -> Result<(), SoftwareError>;
    async fn install(
        &self,
        module: &SoftwareModule,
        logger: &mut BufWriter<File>,
    ) -> Result<(), SoftwareError>;
    async fn remove(
        &self,
        module: &SoftwareModule,
        logger: &mut BufWriter<File>,
    ) -> Result<(), SoftwareError>;
    async fn finalize(&self, logger: &mut BufWriter<File>) -> Result<(), SoftwareError>;
    async fn list(
        &self,
        logger: &mut BufWriter<File>,
    ) -> Result<Vec<SoftwareModule>, SoftwareError>;
    async fn version(
        &self,
        module: &SoftwareModule,
        logger: &mut BufWriter<File>,
    ) -> Result<Option<String>, SoftwareError>;

    async fn apply(
        &self,
        update: &SoftwareModuleUpdate,
        logger: &mut BufWriter<File>,
    ) -> Result<(), SoftwareError> {
        match update.clone() {
            SoftwareModuleUpdate::Install { mut module } => {
                let module_url = module.url.clone();
                match module_url {
                    Some(url) => self.install_from_url(&mut module, &url, logger).await?,
                    None => self.install(&module, logger).await?,
                }

                Ok(())
            }
            SoftwareModuleUpdate::Remove { module } => self.remove(&module, logger).await,
        }
    }

    async fn apply_all(
        &self,
        updates: Vec<SoftwareModuleUpdate>,
        logger: &mut BufWriter<File>,
    ) -> Vec<SoftwareError> {
        let mut failed_updates = Vec::new();

        if let Err(prepare_error) = self.prepare(logger).await {
            failed_updates.push(prepare_error);
            return failed_updates;
        }

        for update in updates.iter() {
            if let Err(error) = self.apply(update, logger).await {
                failed_updates.push(error);
            };
        }

        if let Err(finalize_error) = self.finalize(logger).await {
            failed_updates.push(finalize_error);
        }

        failed_updates
    }

    async fn install_from_url(
        &self,
        module: &mut SoftwareModule,
        url: &DownloadInfo,
        logger: &mut BufWriter<File>,
    ) -> Result<(), SoftwareError> {
        let downloader = Downloader::new(&module.name, &module.version, "/tmp");

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
            logger
                .write_all(format!("error: {}\n", &err).as_bytes())
                .await?;

            return Err(err);
        }

        module.file_path = Some(downloader.filename().to_owned());
        let result = self.install(module, logger).await;
        if let Err(err) = downloader
            .cleanup()
            .await
            .map_err(|err| SoftwareError::DownloadError {
                reason: err.to_string(),
                url: url.url().to_string(),
            })
        {
            logger
                .write_all(format!("warn: {}\n", &err).as_bytes())
                .await?;
        }

        result
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
const FINALIZE: &str = "finalize";
pub const LIST: &str = "list";
const VERSION: &str = "version";

#[async_trait]
impl Plugin for ExternalPluginCommand {
    async fn prepare(&self, logger: &mut BufWriter<File>) -> Result<(), SoftwareError> {
        let command = self.command(PREPARE, None)?;
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
        let command = self.command(INSTALL, Some(module))?;
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
        let command = self.command(REMOVE, Some(module))?;
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

    async fn finalize(&self, logger: &mut BufWriter<File>) -> Result<(), SoftwareError> {
        let command = self.command(FINALIZE, None)?;
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
        let command = self.command(LIST, None)?;
        let output = self.execute(command, logger).await?;
        if output.status.success() {
            let mut software_list = Vec::new();
            let mut rdr = ReaderBuilder::new()
                .has_headers(false)
                .flexible(true)
                .delimiter(b'\t')
                .from_reader(output.stdout.as_slice());

            //dbg!(rdr);
            //let mut record: SoftwareModule = SoftwareModule::new();
            for module in rdr.deserialize() {
                dbg!(&module);
                let record: SoftwareModuleList = module.unwrap();
                dbg!(&record);
                software_list.push(SoftwareModule {
                    name: record.name,
                    version: record.version,
                    module_type: Some(self.name.clone()),
                    file_path: None,
                    url: None,
                });
            }
            // let modinfo: Vec<&str> = module.split('\t').collect();
            // if modinfo.len() >= 2 {
            //     let software_module = SoftwareModule {
            //         name: modinfo[0].into(),
            //         version: Some(modinfo[1].into()),
            //         module_type: Some(self.name.clone()),
            //         file_path: None,
            //         url: None,
            //     };

            // }

            Ok(software_list)
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
        let command = self.command(VERSION, Some(module))?;
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
