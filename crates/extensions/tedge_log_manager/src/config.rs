use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use tedge_config::ReadError;
use tedge_config::TEdgeConfig;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;

pub const DEFAULT_PLUGIN_CONFIG_FILE_NAME: &str = "tedge-log-plugin.toml";
pub const DEFAULT_PLUGIN_CONFIG_DIR_NAME: &str = "plugins/";

/// Configuration of the Configuration Manager
#[derive(Clone, Debug)]
pub struct LogManagerConfig {
    pub config_dir: PathBuf,
    pub topic_root: String,
    pub topic_identifier: String,
    pub plugin_config_dir: PathBuf,
    pub plugin_config_path: PathBuf,
    pub logtype_reload_topic: Topic,
    pub logfile_request_topic: TopicFilter,
    pub current_operations: HashSet<String>,
}

impl LogManagerConfig {
    pub fn from_tedge_config(
        config_dir: impl AsRef<Path>,
        _tedge_config: &TEdgeConfig,
        topic_root: String,
        topic_identifier: String,
    ) -> Result<Self, ReadError> {
        let config_dir: PathBuf = config_dir.as_ref().into();

        let plugin_config_dir = config_dir.join(DEFAULT_PLUGIN_CONFIG_DIR_NAME);
        let plugin_config_path = plugin_config_dir.join(DEFAULT_PLUGIN_CONFIG_FILE_NAME);

        let logtype_reload_topic = Topic::new_unchecked(
            format!("{}/{}/cmd/log_upload", topic_root, topic_identifier).as_str(),
        );
        let logfile_request_topic = TopicFilter::new_unchecked(
            format!("{}/{}/cmd/log_upload/+", topic_root, topic_identifier).as_str(),
        );
        let current_operations = HashSet::new();

        Ok(Self {
            config_dir,
            topic_root,
            topic_identifier,
            plugin_config_dir,
            plugin_config_path,
            logtype_reload_topic,
            logfile_request_topic,
            current_operations,
        })
    }
}
