use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::path::PathBuf;
use std::sync::Arc;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_config::tedge_toml::ReadError;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;

pub const DEFAULT_PLUGIN_CONFIG_FILE_NAME: &str = "tedge-configuration-plugin.toml";
pub const DEFAULT_OPERATION_DIR_NAME: &str = "plugins/";
pub const DEFAULT_PLUGIN_CONFIG_TYPE: &str = "tedge-configuration-plugin";

/// Configuration of the Configuration Manager
#[derive(Clone, Debug)]
pub struct ConfigManagerConfig {
    pub config_dir: PathBuf,
    pub plugin_dirs: Vec<Utf8PathBuf>,
    pub plugin_config_dir: PathBuf,
    pub plugin_config_path: PathBuf,
    pub tmp_path: Arc<Utf8Path>,
    pub mqtt_schema: MqttSchema,
    pub config_reload_topics: Vec<Topic>,
    pub config_update_topic: TopicFilter,
    pub config_snapshot_topic: TopicFilter,
    pub tedge_http_host: Arc<str>,
    pub config_update_enabled: bool,
    pub sudo_enabled: bool,
}

pub struct ConfigManagerOptions {
    pub config_dir: PathBuf,
    pub mqtt_topic_root: MqttSchema,
    pub mqtt_device_topic_id: EntityTopicId,
    pub tedge_http_host: Arc<str>,
    pub tmp_path: Arc<Utf8Path>,
    pub is_sudo_enabled: bool,
    pub config_update_enabled: bool,
    pub plugin_dirs: Vec<Utf8PathBuf>,
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
            .map(|cmd| mqtt_topic_root.capability_topic_for(&mqtt_device_topic_id, cmd))
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
            plugin_dirs: cliopts.plugin_dirs,
            plugin_config_dir,
            plugin_config_path,
            tmp_path: cliopts.tmp_path,
            mqtt_schema: mqtt_topic_root,
            config_reload_topics,
            config_update_topic,
            config_snapshot_topic,
            tedge_http_host: cliopts.tedge_http_host,
            config_update_enabled: cliopts.config_update_enabled,
            sudo_enabled: cliopts.is_sudo_enabled,
        })
    }
}
