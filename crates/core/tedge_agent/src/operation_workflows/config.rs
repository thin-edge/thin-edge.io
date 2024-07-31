use camino::Utf8PathBuf;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;

#[derive(Debug, Clone)]
pub struct OperationConfig {
    pub mqtt_schema: MqttSchema,
    pub device_topic_id: EntityTopicId,
    pub log_dir: Utf8PathBuf,
    pub config_dir: Utf8PathBuf,
    pub state_dir: Utf8PathBuf,
}

impl OperationConfig {
    pub fn from_tedge_config(
        topic_root: String,
        device_topic_id: &EntityTopicId,
        tedge_config_location: &tedge_config::TEdgeConfigLocation,
    ) -> Result<OperationConfig, tedge_config::TEdgeConfigError> {
        let tedge_config = tedge_config::TEdgeConfig::try_new(tedge_config_location.clone())?;

        Ok(OperationConfig {
            mqtt_schema: MqttSchema::with_root(topic_root),
            device_topic_id: device_topic_id.clone(),
            log_dir: tedge_config.logs.path.join("agent"),
            config_dir: tedge_config_location.tedge_config_root_path.clone(),
            state_dir: tedge_config.agent.state.path.clone(),
        })
    }
}
