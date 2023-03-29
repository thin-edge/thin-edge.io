use super::DEFAULT_PLUGIN_CONFIG_TYPE;
use c8y_api::smartrest::topic::C8yTopic;
use log::error;
use log::info;
use serde::Deserialize;
use std::borrow::Borrow;
use std::collections::HashSet;
use std::fs;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::Path;
use tedge_mqtt_ext::MqttError;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_utils::file::PermissionEntry;

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
struct RawPluginConfig {
    pub files: Vec<RawFileEntry>,
}

#[derive(Deserialize, Debug, Default, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RawFileEntry {
    pub path: String,
    #[serde(rename = "type")]
    config_type: Option<String>,
    user: Option<String>,
    group: Option<String>,
    mode: Option<u32>,
}

#[derive(Debug, Eq, PartialEq, Default, Clone)]
pub struct PluginConfig {
    pub files: HashSet<FileEntry>,
}

#[derive(Debug, Eq, Default, Clone)]
pub struct FileEntry {
    pub path: String,
    pub config_type: String,
    pub file_permissions: PermissionEntry,
}

impl Hash for FileEntry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.config_type.hash(state);
    }
}

impl PartialEq for FileEntry {
    fn eq(&self, other: &Self) -> bool {
        self.config_type == other.config_type
    }
}

impl Borrow<String> for FileEntry {
    fn borrow(&self) -> &String {
        &self.config_type
    }
}

impl FileEntry {
    pub fn new(
        path: String,
        config_type: String,
        user: Option<String>,
        group: Option<String>,
        mode: Option<u32>,
    ) -> Self {
        Self {
            path,
            config_type,
            file_permissions: PermissionEntry { user, group, mode },
        }
    }
}

impl RawPluginConfig {
    fn new(config_file_path: &Path) -> Self {
        let path_str = config_file_path.display().to_string();
        info!("Reading the config file from {}", path_str);
        match fs::read_to_string(config_file_path) {
            Ok(contents) => match toml::from_str(contents.as_str()) {
                Ok(config) => config,
                Err(err) => {
                    error!("The config file {path_str} is malformed. {err}");
                    Self::default()
                }
            },
            Err(err) => {
                error!("The config file {path_str} does not exist or is not readable. {err}");
                Self::default()
            }
        }
    }
}

impl PluginConfig {
    pub fn new(config_file_path: &Path) -> Self {
        let plugin_config = Self::new_with_config_file_entry(config_file_path);
        let raw_config = RawPluginConfig::new(config_file_path);
        plugin_config.add_entries_from_raw_config(raw_config)
    }

    fn new_with_config_file_entry(config_file_path: &Path) -> Self {
        let c8y_configuration_plugin = FileEntry::new(
            config_file_path.display().to_string(),
            DEFAULT_PLUGIN_CONFIG_TYPE.into(),
            None,
            None,
            None,
        );
        Self {
            files: HashSet::from([c8y_configuration_plugin]),
        }
    }

    fn add_entries_from_raw_config(mut self, raw_config: RawPluginConfig) -> Self {
        let original_plugin_config = self.clone();
        for raw_entry in raw_config.files {
            let config_type = raw_entry
                .config_type
                .unwrap_or_else(|| raw_entry.path.clone());

            if config_type.contains(['+', '#']) {
                error!(
                    "The config type '{}' contains the forbidden characters, '+' or '#'.",
                    config_type
                );
                return original_plugin_config;
            }

            let entry = FileEntry::new(
                raw_entry.path,
                config_type.clone(),
                raw_entry.user,
                raw_entry.group,
                raw_entry.mode,
            );
            if !self.files.insert(entry) {
                error!("The config file has the duplicated type '{}'.", config_type);
                return original_plugin_config;
            }
        }
        self
    }

    pub fn to_supported_config_types_message(&self) -> Result<MqttMessage, MqttError> {
        let topic = C8yTopic::SmartRestResponse.to_topic()?;
        Ok(MqttMessage::new(&topic, self.to_smartrest_payload()))
    }

    pub fn to_supported_config_types_message_for_child(
        &self,
        child_id: &str,
    ) -> Result<MqttMessage, MqttError> {
        let topic_str = &format!("c8y/s/us/{child_id}");
        let topic = Topic::new(topic_str)?;
        Ok(MqttMessage::new(&topic, self.to_smartrest_payload()))
    }

    pub fn get_all_file_types(&self) -> Vec<String> {
        self.files
            .iter()
            .map(|x| x.config_type.to_string())
            .collect::<Vec<_>>()
    }

    pub fn get_file_entry_from_type(
        &self,
        config_type: &str,
    ) -> Result<FileEntry, InvalidConfigTypeError> {
        let file_entry = self
            .files
            .get(&config_type.to_string())
            .ok_or(InvalidConfigTypeError {
                config_type: config_type.to_owned(),
            })?
            .to_owned();
        Ok(file_entry)
    }

    // 119,typeA,typeB,...
    fn to_smartrest_payload(&self) -> String {
        let mut config_types = self.get_all_file_types();
        // Sort because hashset doesn't guarantee the order
        config_types.sort();
        let supported_config_types = config_types.join(",");
        format!("119,{supported_config_types}")
    }
}

#[derive(thiserror::Error, Debug)]
#[error("The requested config_type {config_type} is not defined in the plugin configuration file.")]
pub struct InvalidConfigTypeError {
    pub config_type: String,
}
