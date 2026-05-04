use crate::Capabilities;
use camino::Utf8PathBuf;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_config::TEdgeConfig;
use tedge_utils::paths::ManagedDir;
use tedge_utils::paths::TedgePaths;

#[derive(Debug, Clone)]
pub struct OperationConfig {
    pub mqtt_schema: MqttSchema,
    pub device_topic_id: EntityTopicId,
    pub service_topic_id: EntityTopicId,
    pub log_dir: ManagedDir,
    pub config_dir: TedgePaths,
    pub state_dir: TedgePaths,
    pub operations_dir: ManagedDir,
    pub tmp_dir: Utf8PathBuf, // TODO: change it to TedgePaths
    pub capabilities: Capabilities,
}

impl OperationConfig {
    pub async fn from_tedge_config(
        topic_root: String,
        device_topic_id: &EntityTopicId,
        service_topic_id: EntityTopicId,
        tedge_config: &TEdgeConfig,
    ) -> Result<OperationConfig, tedge_config::TEdgeConfigError> {
        let config_dir = tedge_config.config_root();
        let capabilities = Capabilities {
            config_update: tedge_config.agent.enable.config_update,
            config_snapshot: tedge_config.agent.enable.config_snapshot,
            log_upload: tedge_config.agent.enable.log_upload,
        };

        Ok(OperationConfig {
            mqtt_schema: MqttSchema::with_root(topic_root),
            device_topic_id: device_topic_id.clone(),
            service_topic_id,
            log_dir: tedge_config.logs_root().dir("agent")?,
            config_dir: tedge_config.config_root(),
            state_dir: tedge_config.state_root(),
            operations_dir: config_dir.dir("operations")?,
            tmp_dir: tedge_config.tmp.path.clone().into(),
            capabilities,
        })
    }
}
