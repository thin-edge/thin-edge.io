use crate::plugin::*;
use json_sm::*;
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

    fn plugin(&self, software_type: &str) -> Result<&Self::Plugin, SoftwareError> {
        let module_plugin = self.by_software_type(software_type).ok_or_else(|| {
            SoftwareError::UnknownSoftwareType {
                software_type: software_type.into(),
            }
        })?;

        Ok(module_plugin)
    }
}

#[derive(Debug)]
pub struct ExternalPlugins {
    plugin_dir: PathBuf,
    plugin_map: HashMap<SoftwareType, ExternalPluginCommand>,
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
                // TODO check the command is actually a plugin

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

    pub async fn list(&self, request: &SoftwareListRequest) -> SoftwareListResponse {
        let mut response = SoftwareListResponse::new(request);

        for (software_type, plugin) in self.plugin_map.iter() {
            match plugin.list().await {
                Ok(software_list) => response.add_modules(&software_type, software_list),
                Err(err) => {
                    // TODO fix the response format to handle an error per module type
                    let reason = format!("{}", err);
                    response.set_error(&reason);
                    return response;
                }
            }
        }
        response
    }

    pub async fn process(&self, request: &SoftwareUpdateRequest) -> SoftwareUpdateResponse {
        let mut response = SoftwareUpdateResponse::new(request);

        for software_type in request.modules_types() {
            let errors = if let Some(plugin) = self.plugin_map.get(&software_type) {
                let updates = request.updates_for(&software_type);
                plugin.apply_all(updates).await
            } else {
                vec![SoftwareError::UnknownSoftwareType {
                    software_type: software_type.clone(),
                }]
            };

            response.add_errors(&software_type, errors)
        }

        for (software_type, plugin) in self.plugin_map.iter() {
            match plugin.list().await {
                Ok(software_list) => response.add_modules(&software_type, software_list),
                Err(err) => response.add_errors(&software_type, vec![err]),
            }
        }

        response
    }
}
