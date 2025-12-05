use crate::error::ConfigManagementError;
use crate::plugin::ExternalPlugin;
use crate::plugin::LIST;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use log::error;
use log::info;
use std::collections::BTreeMap;
use std::sync::Arc;
use tedge_config::SudoCommandBuilder;
use tedge_config::SudoError;

pub type PluginType = String;

#[derive(Debug, Clone)]
pub struct ExternalPlugins {
    plugin_dirs: Vec<Utf8PathBuf>,
    plugin_map: BTreeMap<PluginType, ExternalPlugin>,
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

    pub async fn load(&mut self) -> Result<(), ConfigManagementError> {
        self.plugin_map.clear();

        for plugin_dir in &self.plugin_dirs {
            let entries = match plugin_dir.read_dir_utf8() {
                Ok(entries) => entries,
                Err(err) => {
                    error!(
                        target: "config plugins",
                        "Skipping directory {}: {}",
                        plugin_dir,
                        err
                    );
                    continue;
                }
            };

            for maybe_entry in entries {
                let entry = match maybe_entry {
                    Ok(entry) => entry,
                    Err(err) => {
                        error!(
                            target: "config plugins",
                            "Skipping directory entry in {}: {}",
                            plugin_dir,
                            err
                        );
                        continue;
                    }
                };
                let path = entry.path();
                if path.is_file() {
                    let Some(plugin_name) = path.file_name() else {
                        error!(
                            target: "config plugins",
                            "Skipping {path}: failed to extract plugin name",
                        );
                        continue;
                    };
                    if let Some(plugin) = self.plugin_map.get(plugin_name) {
                        info!("Skipping {path}: overridden by {}", plugin.path.display());
                        continue;
                    }

                    match self.sudo.ensure_command_succeeds(&path, &vec![LIST]) {
                        Ok(()) => {
                            info!(target: "config plugins", "Log plugin activated: {path}");
                        }
                        Err(SudoError::CannotSudo) => {
                            error!(target: "config plugins",
                                "Skipping {path}: not properly configured to run with sudo"
                            );
                            continue;
                        }
                        Err(SudoError::ExecutionFailed(_)) => {
                            error!(target: "config plugins",
                             "Skipping {path}: does not support list operation and may not be a valid plugin"
                            );
                            continue;
                        }
                        Err(err) => {
                            error!(target: "config plugins",
                                "Skipping {path}: can not be launched as a plugin: {err}"
                            );
                            continue;
                        }
                    }

                    let plugin = ExternalPlugin::new(
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

    pub fn is_empty(&self) -> bool {
        self.plugin_map.is_empty()
    }

    pub fn by_plugin_type(&self, plugin_type: &str) -> Option<&ExternalPlugin> {
        self.plugin_map.get(plugin_type)
    }

    pub fn get_all_plugin_types(&self) -> Vec<PluginType> {
        let mut plugin_types: Vec<PluginType> = self.plugin_map.keys().cloned().collect();
        plugin_types.sort();
        plugin_types
    }
}
