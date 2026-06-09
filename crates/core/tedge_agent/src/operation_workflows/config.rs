use crate::Capabilities;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::workflow::log::log_dir::OperationLogs;
use tedge_config::TEdgeConfig;
use tedge_utils::paths::ManagedDir;
use tedge_utils::paths::TedgePaths;

#[derive(Debug, Clone)]
pub struct OperationConfig {
    pub mqtt_schema: MqttSchema,
    pub device_topic_id: EntityTopicId,
    pub service_topic_id: EntityTopicId,
    pub log_dir: OperationLogs,
    pub config_dir: TedgePaths,
    pub state_dir: TedgePaths,
    pub operations_dir: ManagedDir,
    pub tmp_dir: TedgePaths,
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
        let log_dir = tedge_config.operation_logs();

        Ok(OperationConfig {
            mqtt_schema: MqttSchema::with_root(topic_root),
            device_topic_id: device_topic_id.clone(),
            service_topic_id,
            log_dir,
            config_dir: tedge_config.config_root(),
            state_dir: tedge_config.state_root(),
            operations_dir: config_dir.dir("operations")?,
            tmp_dir: tedge_config.tmp_root(),
            capabilities,
        })
    }
}
