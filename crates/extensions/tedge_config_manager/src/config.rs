use camino::Utf8Path;
use log::error;
use log::info;
use log::warn;
use serde::Deserialize;
use std::borrow::Borrow;
use std::collections::HashSet;
use std::fs;
use std::hash::Hash;
use std::hash::Hasher;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_config::ReadError;
use tedge_mqtt_ext::TopicFilter;
use tedge_utils::file::PermissionEntry;

use super::error::InvalidConfigTypeError;

pub const DEFAULT_PLUGIN_CONFIG_FILE_NAME: &str = "tedge-configuration-plugin.toml";
pub const DEFAULT_OPERATION_DIR_NAME: &str = "plugins/";
pub const DEFAULT_PLUGIN_CONFIG_TYPE: &str = "tedge-configuration-plugin";

/// Configuration of the Configuration Manager
#[derive(Clone, Debug)]
pub struct ConfigManagerConfig {
    pub config_dir: PathBuf,
    pub plugin_config_dir: PathBuf,
    pub plugin_config_path: PathBuf,
    pub tmp_path: Arc<Utf8Path>,
    pub config_reload_topics: TopicFilter,
    pub config_update_topic: TopicFilter,
    pub config_snapshot_topic: TopicFilter,

    /// If enabled, config file updates are deployed by tedge-write.
    pub use_tedge_write: TedgeWriteStatus,
}

pub struct ConfigManagerOptions {
    pub config_dir: PathBuf,
    pub mqtt_topic_root: MqttSchema,
    pub mqtt_device_topic_id: EntityTopicId,
    pub tmp_path: Arc<Utf8Path>,
    pub is_sudo_enabled: bool,
}

impl ConfigManagerConfig {
    pub fn from_options(cliopts: ConfigManagerOptions) -> Result<Self, ReadError> {
        let config_dir = cliopts.config_dir;
        let mqtt_topic_root = cliopts.mqtt_topic_root;
        let mqtt_device_topic_id = cliopts.mqtt_device_topic_id;

        let plugin_config_dir = config_dir.join(DEFAULT_OPERATION_DIR_NAME);
        let plugin_config_path = plugin_config_dir.join(DEFAULT_PLUGIN_CONFIG_FILE_NAME);

        let config_reload_topics = [OperationType::ConfigSnapshot, OperationType::ConfigUpdate]
            .into_iter()
            .map(|cmd| {
                mqtt_topic_root.topics(
                    EntityFilter::Entity(&mqtt_device_topic_id),
                    ChannelFilter::CommandMetadata(cmd),
                )
            })
            .collect();

        let config_update_topic = mqtt_topic_root.topics(
            EntityFilter::Entity(&mqtt_device_topic_id),
            ChannelFilter::Command(OperationType::ConfigUpdate),
        );

        let config_snapshot_topic = mqtt_topic_root.topics(
            EntityFilter::Entity(&mqtt_device_topic_id),
            ChannelFilter::Command(OperationType::ConfigSnapshot),
        );

        Ok(Self {
            config_dir,
            plugin_config_dir,
            plugin_config_path,
            tmp_path: cliopts.tmp_path,
            config_reload_topics,
            config_update_topic,
            config_snapshot_topic,
            use_tedge_write: TedgeWriteStatus::Enabled {
                sudo: cliopts.is_sudo_enabled,
            },
        })
    }
}

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

impl Borrow<str> for FileEntry {
    fn borrow(&self) -> &str {
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
    pub fn new(config_file_path: &Path) -> Self {
        Self::read_config(config_file_path)
    }

    pub fn read_config(path: &Path) -> Self {
        let path_str = path.display().to_string();
        info!("Using the configuration from {}", path_str);
        match fs::read_to_string(path) {
            Ok(contents) => match toml::from_str(contents.as_str()) {
                Ok(config) => config,
                _ => {
                    warn!("The config file {} is malformed.", path_str);
                    Self::default()
                }
            },
            Err(_) => {
                warn!(
                    "The config file {} does not exist or is not readable.",
                    path_str
                );
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
        let file_entry = FileEntry::new(
            config_file_path.display().to_string(),
            DEFAULT_PLUGIN_CONFIG_TYPE.into(),
            None,
            None,
            None,
        );
        Self {
            files: HashSet::from([file_entry]),
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

    pub fn get_file_entry_from_type(
        &self,
        config_type: &str,
    ) -> Result<&FileEntry, InvalidConfigTypeError> {
        self.files.get(config_type).ok_or(InvalidConfigTypeError {
            config_type: config_type.to_owned(),
        })
    }

    pub fn get_all_file_types(&self) -> Vec<String> {
        self.files
            .iter()
            .map(|x| x.config_type.to_string())
            .collect::<Vec<_>>()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TedgeWriteStatus {
    Enabled { sudo: bool },
    Disabled,
}
