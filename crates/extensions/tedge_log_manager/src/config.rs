use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tedge_config::ReadError;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;

pub const DEFAULT_PLUGIN_CONFIG_FILE_NAME: &str = "tedge-log-plugin.toml";
pub const DEFAULT_PLUGIN_CONFIG_DIR_NAME: &str = "plugins/";

/// Configuration of the Configuration Manager
#[derive(Clone, Debug)]
pub struct LogManagerConfig {
    pub config_dir: PathBuf,
    pub mqtt_topic_root: String,
    pub mqtt_device_topic_id: String,
    pub plugin_config_dir: PathBuf,
    pub plugin_config_path: PathBuf,
    pub logtype_reload_topic: Topic,
    pub logfile_request_topic: TopicFilter,
    pub current_operations: HashSet<String>,
}

pub struct LogManagerOptions {
    pub config_dir: PathBuf,
    pub mqtt_topic_root: Arc<str>,
    pub mqtt_device_topic_id: Arc<str>,
}

impl LogManagerConfig {
    pub fn from_options(cliopts: LogManagerOptions) -> Result<Self, ReadError> {
        let config_dir = cliopts.config_dir;
        let mqtt_topic_root = cliopts.mqtt_topic_root;
        let mqtt_device_topic_id = cliopts.mqtt_device_topic_id;

        let plugin_config_dir = config_dir.join(DEFAULT_PLUGIN_CONFIG_DIR_NAME);
        let plugin_config_path = plugin_config_dir.join(DEFAULT_PLUGIN_CONFIG_FILE_NAME);

        // TODO: move topic parsing to tedge_api
        let logtype_reload_topic = Topic::new_unchecked(
            format!(
                "{}/{}/cmd/log_upload",
                mqtt_topic_root, mqtt_device_topic_id
            )
            .as_str(),
        );
        let logfile_request_topic = TopicFilter::new_unchecked(
            format!(
                "{}/{}/cmd/log_upload/+",
                mqtt_topic_root, mqtt_device_topic_id
            )
            .as_str(),
        );
        let current_operations = HashSet::new();

        Ok(Self {
            config_dir,
            mqtt_topic_root: mqtt_topic_root.to_string(),
            mqtt_device_topic_id: mqtt_device_topic_id.to_string(),
            plugin_config_dir,
            plugin_config_path,
            logtype_reload_topic,
            logfile_request_topic,
            current_operations,
        })
    }
}
