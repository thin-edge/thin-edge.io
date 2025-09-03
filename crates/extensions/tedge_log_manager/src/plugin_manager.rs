use crate::error::LogManagementError;
use crate::plugin::ExternalPluginCommand;
use crate::plugin::LIST;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::Stdio;
use tedge_config::SudoCommandBuilder;
use tracing::error;
use tracing::info;
use tracing::warn;

pub type PluginType = String;

/// The main responsibility of a `LogPlugins` implementation is to retrieve the appropriate plugin for a given log type.
pub trait Plugins {
    type Plugin;

    /// Return the plugin declared with the given name, if any.
    fn by_plugin_type(&self, plugin_type: &str) -> Option<&Self::Plugin>;

    fn get_all_plugin_types(&self) -> Vec<PluginType>;
}

#[derive(Debug)]
pub struct ExternalPlugins {
    plugin_dir: PathBuf,
    plugin_map: BTreeMap<PluginType, ExternalPluginCommand>,
    sudo: SudoCommandBuilder,
    config_dir: PathBuf,
}

impl Plugins for ExternalPlugins {
    type Plugin = ExternalPluginCommand;

    fn by_plugin_type(&self, plugin_type: &str) -> Option<&Self::Plugin> {
        self.plugin_map.get(plugin_type)
    }

    fn get_all_plugin_types(&self) -> Vec<PluginType> {
        let mut plugin_types: Vec<PluginType> = self.plugin_map.keys().cloned().collect();
        plugin_types.sort();
        plugin_types
    }
}

impl ExternalPlugins {
    pub fn new(config_dir: PathBuf) -> Self {
        ExternalPlugins {
            plugin_dir: config_dir.join("log-plugins"),
            plugin_map: BTreeMap::new(),
            sudo: SudoCommandBuilder::enabled(false),
            config_dir,
        }
    }

    pub async fn open(
        plugin_dir: impl Into<PathBuf>,
        sudo_enabled: bool,
        config_dir: PathBuf,
    ) -> Result<ExternalPlugins, LogManagementError> {
        let mut plugins = ExternalPlugins {
            plugin_dir: plugin_dir.into(),
            plugin_map: BTreeMap::new(),
            sudo: SudoCommandBuilder::enabled(sudo_enabled),
            config_dir,
        };
        if let Err(e) = plugins.load().await {
            warn!(
                "Reading the plugins directory ({:?}): failed with: {e:?}",
                &plugins.plugin_dir
            );
        }

        Ok(plugins)
    }

    pub async fn load(&mut self) -> anyhow::Result<()> {
        self.plugin_map.clear();

        let config = tedge_config::TEdgeConfig::load(&self.config_dir)
            .await
            .map_err(|err| io::Error::other(format!("Failed to load tedge config: {}", err)))?;

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
                        info!("Log plugin activated: {}", path.display());
                    }

                    // If the file is not executable or returned non 0 status code we assume it is not a valid log plugin and skip further processing.
                    Ok(_) => {
                        error!(
                            "File {} in log plugin directory does not support list operation and may not be a valid plugin, skipping.",
                            path.display()
                        );
                        continue;
                    }

                    Err(err) if err.kind() == ErrorKind::PermissionDenied => {
                        error!(
                            "File {} Permission Denied, is the file an executable?\n
                            The file will not be registered as a log plugin.",
                            path.display()
                        );
                        continue;
                    }

                    Err(err) => {
                        error!(
                            "An error occurred while trying to run: {}: {}\n
                            The file will not be registered as a log plugin.",
                            path.display(),
                            err
                        );
                        continue;
                    }
                }

                if let Some(file_name) = path.file_name() {
                    if let Some(plugin_name) = file_name.to_str() {
                        let plugin = ExternalPluginCommand::new(
                            plugin_name.to_string(),
                            path.clone(),
                            self.sudo.clone(),
                            config.tmp.path.as_path().into(),
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
}
