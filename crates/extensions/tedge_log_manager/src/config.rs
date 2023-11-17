use std::path::PathBuf;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_config::ReadError;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;

pub const DEFAULT_PLUGIN_CONFIG_FILE_NAME: &str = "tedge-log-plugin.toml";
pub const DEFAULT_PLUGIN_CONFIG_DIR_NAME: &str = "plugins/";

/// Configuration of the Configuration Manager
#[derive(Clone, Debug)]
pub struct LogManagerConfig {
    pub config_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub plugin_config_dir: PathBuf,
    pub plugin_config_path: PathBuf,
    pub logtype_reload_topic: Topic,
    pub logfile_request_topic: TopicFilter,
}

pub struct LogManagerOptions {
    pub config_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub mqtt_schema: MqttSchema,
    pub mqtt_device_topic_id: EntityTopicId,
}

impl LogManagerConfig {
    pub fn from_options(cliopts: LogManagerOptions) -> Result<Self, ReadError> {
        let config_dir = cliopts.config_dir;
        let tmp_dir = cliopts.tmp_dir;
        let mqtt_schema = cliopts.mqtt_schema;
        let mqtt_device_topic_id = cliopts.mqtt_device_topic_id;

        let plugin_config_dir = config_dir.join(DEFAULT_PLUGIN_CONFIG_DIR_NAME);
        let plugin_config_path = plugin_config_dir.join(DEFAULT_PLUGIN_CONFIG_FILE_NAME);

        let logtype_reload_topic = mqtt_schema.topic_for(
            &mqtt_device_topic_id,
            &Channel::CommandMetadata {
                operation: OperationType::LogUpload,
            },
        );

        let logfile_request_topic = mqtt_schema.topics(
            EntityFilter::Entity(&mqtt_device_topic_id),
            ChannelFilter::Command(OperationType::LogUpload),
        );

        Ok(Self {
            config_dir,
            tmp_dir,
            plugin_config_dir,
            plugin_config_path,
            logtype_reload_topic,
            logfile_request_topic,
        })
    }
}
