use crate::plugin::{Plugin, LIST};
use crate::{log_file::LogFile, plugin::ExternalPluginCommand};
use agent_interface::{
    SoftwareError, SoftwareListRequest, SoftwareListResponse, SoftwareType, SoftwareUpdateRequest,
    SoftwareUpdateResponse, DEFAULT,
};
use std::path::Path;
use std::{
    collections::HashMap,
    fs,
    io::{self, ErrorKind},
    path::PathBuf,
    process::{Command, Stdio},
};
use tracing::{error, info, warn};

/// The main responsibility of a `Plugins` implementation is to retrieve the appropriate plugin for a given software module.
pub trait Plugins {
    type Plugin;

    /// Return the plugin to be used by default when installing a software module, if any.
    fn default(&self) -> Option<&Self::Plugin>;

    /// Return the plugin declared with the given name, if any.
    fn by_software_type(&self, software_type: &str) -> Option<&Self::Plugin>;

    /// Return the plugin associated with the file extension of the module name, if any.
    fn by_file_extension(&self, module_name: &str) -> Option<&Self::Plugin>;

    fn plugin(&self, software_type: &str) -> Result<&Self::Plugin, SoftwareError> {
        let module_plugin = self.by_software_type(software_type).ok_or_else(|| {
            SoftwareError::UnknownSoftwareType {
                software_type: software_type.into(),
            }
        })?;

        Ok(module_plugin)
    }

    fn update_default(&mut self, new_default: &Option<SoftwareType>) -> Result<(), SoftwareError>;
}

#[derive(Debug)]
pub struct ExternalPlugins {
    plugin_dir: PathBuf,
    plugin_map: HashMap<SoftwareType, ExternalPluginCommand>,
    default_plugin_type: Option<SoftwareType>,
    sudo: Option<PathBuf>,
}

impl Plugins for ExternalPlugins {
    type Plugin = ExternalPluginCommand;

    fn default(&self) -> Option<&Self::Plugin> {
        if let Some(default_plugin_type) = &self.default_plugin_type {
            self.by_software_type(default_plugin_type.as_str())
        } else if self.plugin_map.len() == 1 {
            Some(self.plugin_map.iter().next().unwrap().1) //Unwrap is safe here as one entry is guaranteed
        } else {
            None
        }
    }

    fn update_default(&mut self, new_default: &Option<SoftwareType>) -> Result<(), SoftwareError> {
        self.default_plugin_type = new_default.to_owned();
        Ok(())
    }

    fn by_software_type(&self, software_type: &str) -> Option<&Self::Plugin> {
        if software_type.eq(DEFAULT) {
            self.default()
        } else {
            self.plugin_map.get(software_type)
        }
    }

    fn by_file_extension(&self, module_name: &str) -> Option<&Self::Plugin> {
        if let Some(dot) = module_name.rfind('.') {
            let (_, extension) = module_name.split_at(dot + 1);
            self.by_software_type(extension)
        } else {
            self.default()
        }
    }
}

impl ExternalPlugins {
    pub fn open(
        plugin_dir: impl Into<PathBuf>,
        default_plugin_type: Option<String>,
        sudo: Option<PathBuf>,
    ) -> Result<ExternalPlugins, SoftwareError> {
        let mut plugins = ExternalPlugins {
            plugin_dir: plugin_dir.into(),
            plugin_map: HashMap::new(),
            default_plugin_type: default_plugin_type.clone(),
            sudo,
        };
        let () = plugins.load()?;

        match default_plugin_type {
            Some(default_plugin_type) => {
                if plugins
                    .by_software_type(default_plugin_type.as_str())
                    .is_none()
                {
                    warn!(
                        "The configured default plugin: {} not found",
                        default_plugin_type
                    );
                }
                info!("Default plugin type: {}", default_plugin_type)
            }
            None => {
                info!("Default plugin type: Not configured")
            }
        }

        Ok(plugins)
    }

    pub fn load(&mut self) -> io::Result<()> {
        self.plugin_map.clear();
        for maybe_entry in fs::read_dir(&self.plugin_dir)? {
            let entry = maybe_entry?;
            let path = entry.path();
            if path.is_file() {
                let mut command = if let Some(sudo) = &self.sudo {
                    let mut command = Command::new(sudo);
                    command.arg(&path);
                    command
                } else {
                    Command::new(&path)
                };

                match command
                    .arg(LIST)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                {
                    Ok(code) if code.success() => {
                        info!("Plugin activated: {}", path.display());
                    }

                    // If the file is not executable or returned non 0 status code we assume it is not a valid and skip further processing.
                    Ok(_) => {
                        error!(
                            "File {} in plugin directory does not support list operation and may not be a valid plugin, skipping.",
                            path.display()
                        );
                        continue;
                    }

                    Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                        error!(
                            "File {} Permission Denied, is the file an executable?\n
                            The file will not be registered as a plugin.",
                            path.display()
                        );
                        continue;
                    }

                    Err(err) => {
                        error!(
                            "An error occurred while trying to run: {}: {}\n
                            The file will not be registered as a plugin.",
                            path.display(),
                            err
                        );
                        continue;
                    }
                }

                if let Some(file_name) = path.file_name() {
                    if let Some(plugin_name) = file_name.to_str() {
                        let plugin = ExternalPluginCommand::new(plugin_name, &path);
                        self.plugin_map.insert(plugin_name.into(), plugin);
                    }
                }
            }
        }

        Ok(())
    }

    pub fn empty(&self) -> bool {
        self.plugin_map.is_empty()
    }

    pub async fn list(
        &self,
        request: &SoftwareListRequest,
        mut log_file: LogFile,
    ) -> SoftwareListResponse {
        let mut response = SoftwareListResponse::new(request);
        let logger = log_file.buffer();
        let mut error_count = 0;

        for (software_type, plugin) in self.plugin_map.iter() {
            match plugin.list(logger).await {
                Ok(software_list) => response.add_modules(software_type, software_list),
                Err(_) => {
                    error_count += 1;
                }
            }
        }

        if let Some(reason) = ExternalPlugins::error_message(log_file.path(), error_count) {
            response.set_error(&reason);
        }

        response
    }

    pub async fn process(
        &self,
        request: &SoftwareUpdateRequest,
        mut log_file: LogFile,
        download_path: &Path,
    ) -> SoftwareUpdateResponse {
        let mut response = SoftwareUpdateResponse::new(request);
        let logger = log_file.buffer();
        let mut error_count = 0;

        for software_type in request.modules_types() {
            let errors = if let Some(plugin) = self.by_software_type(&software_type) {
                let updates = request.updates_for(&software_type);
                plugin.apply_all(updates, logger, download_path).await
            } else {
                vec![SoftwareError::UnknownSoftwareType {
                    software_type: software_type.clone(),
                }]
            };

            if !errors.is_empty() {
                error_count += 1;
                response.add_errors(&software_type, errors);
            }
        }

        for (software_type, plugin) in self.plugin_map.iter() {
            match plugin.list(logger).await {
                Ok(software_list) => response.add_modules(software_type, software_list),
                Err(err) => {
                    error_count += 1;
                    response.add_errors(software_type, vec![err])
                }
            }
        }

        if let Some(reason) = ExternalPlugins::error_message(log_file.path(), error_count) {
            response.set_error(&reason);
        }

        response
    }

    fn error_message(log_file: &Path, error_count: i32) -> Option<String> {
        if error_count > 0 {
            let reason = if error_count == 1 {
                format!("1 error, see device log file {}", log_file.display())
            } else {
                format!(
                    "{} errors, see device log file {}",
                    error_count,
                    log_file.display()
                )
            };
            Some(reason)
        } else {
            None
        }
    }
}
