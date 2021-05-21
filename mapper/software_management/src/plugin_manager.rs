use crate::plugin::*;

use crate::software::{SoftwareError, SoftwareModule};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;

/// The main responsibility of a `Plugins` implementation is to retrieve the appropriate plugin for a given software module.
///
pub trait Plugins {
    type Plugin;

    /// Return the plugin to be used by default when installing a software module, if any.
    fn default(&self) -> Option<&Self::Plugin>;

    /// Return the plugin declared with the given name, if any.
    fn by_software_type(&self, software_type: &str) -> Option<&Self::Plugin>;

    /// Return the plugin associated with the file extension of the module name, if any.
    fn by_file_extension(&self, module_name: &str) -> Option<&Self::Plugin>;

    fn plugin(&self, module: &SoftwareModule) -> Result<&Self::Plugin, SoftwareError> {
        let software_type= &module.software_type;
        let module_plugin = self
            .by_software_type(software_type)
            .ok_or_else(|| SoftwareError::UnknownSoftwareType { software_type: software_type.into() })?;

        Ok(module_plugin)
    }
}

/// A set of plugins materialized by executable files stored in a defined location.
///
/// * The plugin store is defined by a directory.
/// * Each file of that directory is assumed to be the definition of a plugin.
/// * To define a default plugin simply create a link named `default` pointing to the appropriate plugin.
///    * `ln apt default`
/// * To associate a file extension to a plugin create a link named after the file extension.
///    * `ln dpkg deb`
///    * `ln apama epl`
///
/// ## TODO:
/// * Consider to reload the plugins on directory update.
/// * How to check that a file is actual a plugin?
/// * How to deal with file extension that are quite general as `.zip`?
/// * Is there a better way to match a software name with a plugin?
///    * An idea can be to add an action `extensions` that return a list of extensions managed by the plugin.
///      This will be also helpful to check that a file in the plugin dir is actually a plugin.
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
                // TODO check the exec is a plugin
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
}

/// The set of all installed plugins can be used as a plugin too.
impl Plugin for ExternalPlugins {
    type SoftwareList = ();

    fn list(&self) -> Result<Self::SoftwareList, SoftwareError> {
        unimplemented!()
    }

    fn version(&self, module: &SoftwareModule) -> Result<Option<String>, SoftwareError> {
        self.plugin(module)?.version(module)
    }

    fn install(&self, module: &SoftwareModule) -> Result<(), SoftwareError> {
        self.plugin(module)?.install(module)
    }

    fn uninstall(&self, module: &SoftwareModule) -> Result<(), SoftwareError> {
        self.plugin(module)?.uninstall(module)
    }
}
