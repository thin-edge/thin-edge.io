use crate::{error::PluginError, plugin::*};
use json_sm::{
    messages::{
        SoftwareModuleItem, SoftwareOperationStatus, SoftwareRequestResponse,
        SoftwareRequestResponseSoftwareList,
    },
    software::*,
};
use std::{collections::HashMap, fs, io, path::PathBuf};

/// The main responsibility of a `Plugins` implementation is to retrieve the appropriate plugin for a given software module.
pub trait Plugins {
    type Plugin;

    /// Return the plugin to be used by default when installing a software module, if any.
    fn default(&self) -> Option<&Self::Plugin>;

    /// Return the plugin declared with the given name, if any.
    fn by_software_type(&self, software_type: &str) -> Option<&Self::Plugin>;

    /// Return the plugin associated with the file extension of the module name, if any.
    fn by_file_extension(&self, module_name: &str) -> Option<&Self::Plugin>;

    fn plugin(&self, software_type: &str) -> Result<&Self::Plugin, PluginError> {
        let module_plugin = self.by_software_type(software_type).ok_or_else(|| {
            PluginError::UnknownSoftwareType {
                software_type: software_type.into(),
            }
        })?;

        Ok(module_plugin)
    }
}

// type PluginName = String;
#[derive(Debug)]
pub struct ExternalPlugins {
    plugin_dir: PathBuf,
    plugin_map: HashMap<String, ExternalPluginCommand>,
}

impl Plugins for ExternalPlugins {
    type Plugin = ExternalPluginCommand;

    fn default(&self) -> Option<&Self::Plugin> {
        self.by_software_type("default")
    }

    fn by_software_type(&self, software_type: &str) -> Option<&Self::Plugin> {
        self.plugin_map.get(software_type)
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
    pub fn open(plugin_dir: impl Into<PathBuf>) -> io::Result<ExternalPlugins> {
        let mut plugins = ExternalPlugins {
            plugin_dir: plugin_dir.into(),
            plugin_map: HashMap::new(),
        };
        let () = plugins.load()?;
        Ok(plugins)
    }

    pub fn load(&mut self) -> io::Result<()> {
        self.plugin_map.clear();
        for maybe_entry in fs::read_dir(&self.plugin_dir)? {
            let entry = maybe_entry?;
            let path = entry.path();
            if path.is_file() {
                // TODO check the file is exec

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

    pub async fn list(&self) -> Result<Vec<SoftwareRequestResponseSoftwareList>, PluginError> {
        let mut complete_software_list = Vec::new();
        for software_type in self.plugin_map.keys() {
            let software_list = self.plugin(&software_type)?.list().await?;
            let plugin_software_list = software_list
                .into_iter()
                .map(|item| item.into())
                .collect::<Vec<SoftwareModuleItem>>();

            complete_software_list.push(SoftwareRequestResponseSoftwareList {
                plugin_type: software_type.clone(),
                list: plugin_software_list,
            });
        }
        Ok(complete_software_list)
    }

    pub async fn process(
        &self,
        request: &SoftwareRequestUpdate,
    ) -> Result<SoftwareRequestResponse, PluginError> {
        let mut response = SoftwareRequestResponse {
            id: request.id,
            status: SoftwareOperationStatus::Failed,
            reason: None,
            current_software_list: Vec::new(),
            failures: Vec::new(),
        };

        for software_list_type in &request.update_list {
            let plugin = self
                .by_software_type(&software_list_type.plugin_type)
                .unwrap();

            // What to do if prepare fails?
            // What should be in failures list?
            if let Err(e) = plugin.prepare().await {
                response.reason = Some(format!("Failed prepare stage: {}", e));

                continue;
            };

            let failed_actions = self
                .install_or_remove(&software_list_type.list, plugin)
                .await;

            // What to do if finalize fails?
            let () = plugin.finalize().await?;

            response.failures.push(SoftwareRequestResponseSoftwareList {
                plugin_type: plugin.name.clone(),
                list: failed_actions,
            });
        }

        Ok(response)
    }

    async fn install_or_remove(
        &self,
        items: &[SoftwareModuleItem],
        plugin: &ExternalPluginCommand,
    ) -> Vec<SoftwareModuleItem> {
        let updates = items
            .iter()
            .filter_map(|item| item.clone().into())
            .collect::<Vec<SoftwareModuleUpdate>>();
        let failed_updates = plugin.apply_all(&updates).await;
        failed_updates
            .into_iter()
            .map(|update_result| update_result.into())
            .collect::<Vec<SoftwareModuleItem>>()
    }
}
