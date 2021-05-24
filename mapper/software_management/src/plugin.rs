use crate::software::*;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

pub trait Plugin {
    fn list(&self) -> Result<SoftwareList, SoftwareError>;
    fn version(&self, module: &SoftwareModule) -> Result<Option<String>, SoftwareError>;
    fn install(&self, module: &SoftwareModule) -> Result<(), SoftwareError>;
    fn uninstall(&self, module: &SoftwareModule) -> Result<(), SoftwareError>;

    fn apply(&self, update: &SoftwareUpdate) -> SoftwareUpdateStatus {
        let result = match update {
            SoftwareUpdate::Install { module } => self.install(&module),
            SoftwareUpdate::UnInstall { module } => self.uninstall(&module),
        };
        let status = match result {
            Ok(()) => { UpdateStatus::Success }
            Err(reason) => { UpdateStatus::Error { reason }}
        };

        SoftwareUpdateStatus {
            update: update.clone(),
            status,
        }
    }
}

/// TODO: consider tokio::process
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

    pub fn check_module_type(&self, module: &SoftwareModule) -> Result<(), SoftwareError> {
        if module.software_type == self.name {
            Ok(())
        } else {
            Err(SoftwareError::WrongModuleType {
                actual_type: self.name.clone(),
                expected_type: module.software_type.clone(),
            })
        }
    }

    pub fn command(
        &self,
        action: &str,
        maybe_module: Option<&SoftwareModule>,
    ) -> Result<Command, SoftwareError> {
        let mut command = Command::new(&self.path);

        if let Some(module) = maybe_module {
            self.check_module_type(module)?;
            command.arg(action).arg(&module.name);
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

    pub fn execute(&self, mut command: Command) -> Result<Output, SoftwareError> {
        let output = command.output().map_err(|err| self.plugin_error(err))?;
        Ok(output)
    }

    pub fn content(&self, bytes: Vec<u8>) -> Result<String, SoftwareError> {
        Ok(String::from_utf8(bytes).map_err(|err| self.plugin_error(err))?)
    }

    pub fn plugin_error(&self, err: impl std::fmt::Display) -> SoftwareError {
        SoftwareError::PluginError {
            software_type: self.name.clone(),
            reason: format!("{}", err),
        }
    }
}

const LIST: &'static str = "list";
const VERSION: &'static str = "version";
const INSTALL: &'static str = "install";
const UN_INSTALL: &'static str = "uninstall";

impl Plugin for ExternalPluginCommand {
    fn list(&self) -> Result<SoftwareList, SoftwareError> {
        let command = self.command(LIST, None)?;
        let output = self.execute(command)?;

        if output.status.success() {
            Ok(vec![])
        } else {
            Err(SoftwareError::PluginError {
                software_type: self.name.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }

    fn version(&self, module: &SoftwareModule) -> Result<Option<String>, SoftwareError> {
        let command = self.command(VERSION, Some(module))?;
        let output = self.execute(command)?;

        if output.status.success() {
            let version = String::from(self.content(output.stdout)?.trim());
            if version.is_empty() {
                Ok(None)
            } else {
                Ok(Some(version.into()))
            }
        } else {
            Err(SoftwareError::PluginError {
                software_type: self.name.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }

    fn install(&self, module: &SoftwareModule) -> Result<(), SoftwareError> {
        let command = self.command(INSTALL, Some(module))?;
        let output = self.execute(command)?;

        if output.status.success() {
            Ok(())
        } else {
            Err(SoftwareError::InstallError {
                module: module.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }

    fn uninstall(&self, module: &SoftwareModule) -> Result<(), SoftwareError> {
        let command = self.command(UN_INSTALL, Some(module))?;
        let output = self.execute(command)?;

        if output.status.success() {
            Ok(())
        } else {
            Err(SoftwareError::UnInstallError {
                module: module.clone(),
                reason: self.content(output.stderr)?,
            })
        }
    }
}
