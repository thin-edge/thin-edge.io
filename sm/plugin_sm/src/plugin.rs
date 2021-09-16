use async_trait::async_trait;
use json_sm::*;
use std::{
    iter::Iterator,
    path::PathBuf,
    process::{Output, Stdio},
};
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::process::Command;

#[async_trait]
pub trait Plugin {
    async fn prepare(&self) -> Result<(), SoftwareError>;
    async fn install(&self, module: &SoftwareModule) -> Result<(), SoftwareError>;
    async fn remove(&self, module: &SoftwareModule) -> Result<(), SoftwareError>;
    async fn finalize(&self) -> Result<(), SoftwareError>;
    async fn list(&self) -> Result<Vec<SoftwareModule>, SoftwareError>;
    async fn version(&self, module: &SoftwareModule) -> Result<Option<String>, SoftwareError>;

    async fn apply(&self, update: &SoftwareModuleUpdate) -> Result<(), SoftwareError> {
        match update {
            SoftwareModuleUpdate::Install { module } => self.install(module).await,
            SoftwareModuleUpdate::Remove { module } => self.remove(module).await,
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

    pub async fn execute_and_log(
        &self,
        command: Command,
        logger: &mut BufWriter<File>,
    ) -> Result<Output, SoftwareError> {
        let command_args = format!("{:?}", &command);
        let outcome = self.execute(command).await;

        ExternalPluginCommand::log_command_result(&command_args, &outcome, logger).await?;

        outcome
    }

    pub async fn log_command_result(
        command_args: &str,
        result: &Result<Output, SoftwareError>,
        logger: &mut BufWriter<File>,
    ) -> Result<(), std::io::Error> {
        logger
            .write_all(format!("----- $ {:?}\n", command_args).as_bytes())
            .await?;

        match result.as_ref() {
            Ok(output) => {
                logger
                    .write_all(format!("{}\n\n", &output.status).as_bytes())
                    .await?;
                logger.write_all(b"stdout <<EOF\n").await?;
                logger.write_all(&output.stdout).await?;
                logger.write_all(b"EOF\n\n").await?;
                logger.write_all(b"stderr <<EOF\n").await?;
                logger.write_all(&output.stderr).await?;
                logger.write_all(b"EOF\n").await?;
            }
            Err(err) => {
                logger
                    .write_all(format!("error: {}\n", &err).as_bytes())
                    .await?;
            }
        }
        logger.flush().await?;
        Ok(())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::*;
    use tokio::fs::File;

    #[tokio::test]
    async fn log_command_status_stdout_and_stderr() -> Result<(), anyhow::Error> {
        let tmp_dir = TempDir::new()?;
        let file_path = tmp_dir.path().join("operation.log");

        let mut command = Command::new("echo");
        command
            .arg("Hello")
            .arg("World!")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let output = Ok(command.output().await?);

        let file = File::create(file_path.clone()).await?;
        let mut logger = BufWriter::new(file);
        ExternalPluginCommand::log_command_result("echo Hello World!", &output, &mut logger)
            .await?;

        let content = String::from_utf8(std::fs::read(&file_path)?)?;
        assert_eq!(
            content,
            r#"----- $ "echo Hello World!"
exit code: 0

stdout <<EOF
Hello World!
EOF

stderr <<EOF
EOF
"#
        );
        Ok(())
    }
}
