use crate::error::ConfigManagementError;
use crate::plugin::ExternalPlugin;
use crate::plugin::LIST;
use camino::Utf8Path;
use log::error;
use log::info;
use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tedge_config::SudoCommandBuilder;

pub type PluginType = String;

#[derive(Debug, Clone)]
pub struct ExternalPlugins {
    plugin_dir: PathBuf,
    plugin_map: BTreeMap<PluginType, ExternalPlugin>,
    sudo: SudoCommandBuilder,
    tmp_dir: Arc<Utf8Path>,
}

impl ExternalPlugins {
    pub fn new(plugin_dir: impl Into<PathBuf>, sudo_enabled: bool, tmp_dir: Arc<Utf8Path>) -> Self {
        ExternalPlugins {
            plugin_dir: plugin_dir.into(),
            plugin_map: BTreeMap::new(),
            sudo: SudoCommandBuilder::enabled(sudo_enabled),
            tmp_dir,
        }
    }

    pub async fn load(&mut self) -> Result<(), ConfigManagementError> {
        self.plugin_map.clear();

        for maybe_entry in fs::read_dir(&self.plugin_dir)? {
            let entry = maybe_entry?;
            let path = entry.path();
            if path.is_file() {
                let mut command = self.sudo.command(&path);

                match command
                    .arg(LIST)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                {
                    Ok(code) if code.success() => {
                        info!("Config plugin activated: {}", path.display());
                    }

                    // If the file is not executable or returned non 0 status code we assume it is not a valid config plugin and skip further processing.
                    Ok(_) => {
                        error!(
                            "File: {} in config plugin directory does not support list operation and may not be a valid plugin, skipping.",
                            path.display()
                        );
                        continue;
                    }

                    Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                        error!(
                            "File {} Permission Denied, is the file an executable?\n
                            The file will not be registered as a config plugin.",
                            path.display()
                        );
                        continue;
                    }

                    Err(err) => {
                        error!(
                            "An error occurred while trying to run: {}: {}\n
                            The file will not be registered as a config plugin.",
                            path.display(),
                            err
                        );
                        continue;
                    }
                }

                if let Some(file_name) = path.file_name() {
                    if let Some(plugin_name) = file_name.to_str() {
                        let plugin = ExternalPlugin::new(
                            plugin_name.to_string(),
                            path.clone(),
                            self.sudo.clone(),
                            self.tmp_dir.clone(),
                        );
                        self.plugin_map.insert(plugin_name.into(), plugin);
                    }
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
