use crate::error::LogManagementError;
use crate::plugin::ExternalPluginCommand;
use crate::plugin::LIST;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::process::Stdio;
use std::sync::Arc;
use tedge_config::SudoCommandBuilder;
use tracing::error;
use tracing::info;

pub type PluginType = String;

#[derive(Debug)]
pub struct ExternalPlugins {
    plugin_dirs: Vec<Utf8PathBuf>,
    plugin_map: BTreeMap<PluginType, ExternalPluginCommand>,
    sudo: SudoCommandBuilder,
    tmp_dir: Arc<Utf8Path>,
}

impl ExternalPlugins {
    pub fn new(plugin_dirs: Vec<Utf8PathBuf>, sudo_enabled: bool, tmp_dir: Arc<Utf8Path>) -> Self {
        ExternalPlugins {
            plugin_dirs,
            plugin_map: BTreeMap::new(),
            sudo: SudoCommandBuilder::enabled(sudo_enabled),
            tmp_dir,
        }
    }

    pub async fn load(&mut self) -> Result<(), LogManagementError> {
        self.plugin_map.clear();
        for plugin_dir in &self.plugin_dirs {
            let entries = match plugin_dir.read_dir_utf8() {
                Ok(entries) => entries,
                Err(err) => {
                    error!(
                        target: "log plugins",
                        "Failed to read log plugin directory {plugin_dir} due to: {err}, skipping"
                    );
                    continue;
                }
            };

            for maybe_entry in entries {
                let entry = match maybe_entry {
                    Ok(entry) => entry,
                    Err(err) => {
                        error!(target: "log plugins",
                            "Failed to read log plugin directory entry in {plugin_dir}: due to {err}, skipping",
                        );
                        continue;
                    }
                };
                let path = entry.path();
                if path.is_file() {
                    let Some(plugin_name) = path.file_name() else {
                        error!(
                            target: "log plugins",
                            "Failed to extract log plugin name from {path}, skipping",
                        );
                        continue;
                    };
                    if let Some(plugin) = self.plugin_map.get(plugin_name) {
                        info!(
                            target: "log plugins",
                            "The log plugin {path} is overriden by {}, skipping",
                            plugin.path.display()
                        );
                        continue;
                    }

                    let mut command = self.sudo.command(path);

                    match command
                        .arg(LIST)
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                    {
                        Ok(code) if code.success() => {
                            info!(target: "log plugins", "Log plugin activated: {path}");
                        }

                        // If the file is not executable or returned non 0 status code we assume it is not a valid log plugin and skip further processing.
                        Ok(_) => {
                            error!(target: "log plugins",
                                "File {path} in log plugin directory does not support list operation and may not be a valid plugin, skipping."
                            );
                            continue;
                        }

                        Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                            error!(
                                target: "log plugins",
                                "File {path} Permission Denied, is the file an executable?\n
                                The file will not be registered as a log plugin."
                            );
                            continue;
                        }

                        Err(err) => {
                            error!(
                                target: "log plugins",
                                "An error occurred while trying to run: {path}: {err}\n
                                The file will not be registered as a log plugin."
                            );
                            continue;
                        }
                    }

                    let plugin = ExternalPluginCommand::new(
                        plugin_name.to_string(),
                        path,
                        self.sudo.clone(),
                        self.tmp_dir.clone(),
                    );
                    self.plugin_map.insert(plugin_name.into(), plugin);
                }
            }
        }

        Ok(())
    }

    pub(crate) fn by_plugin_type(&self, plugin_type: &str) -> Option<&ExternalPluginCommand> {
        self.plugin_map.get(plugin_type)
    }

    pub(crate) fn get_all_plugin_types(&self) -> Vec<PluginType> {
        let mut plugin_types: Vec<PluginType> = self.plugin_map.keys().cloned().collect();
        plugin_types.sort();
        plugin_types
    }

    pub fn is_empty(&self) -> bool {
        self.plugin_map.is_empty()
    }
}
