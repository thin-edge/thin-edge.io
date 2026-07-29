use crate::Capabilities;
use std::sync::Arc;
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

    /// The URL of the entity store collection, e.g. `http://127.0.0.1:8000/te/v1/entities`
    ///
    /// Built from `http.client.host` and `http.client.port`, which point to the main device's
    /// HTTP server: the entity store runs there and nowhere else.
    pub entities_url: Arc<str>,
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

        let http_client = &tedge_config.http.client;
        let protocol = if tedge_config.http.is_secure() {
            "https"
        } else {
            "http"
        };
        let entities_url = format!(
            "{protocol}://{}:{}/te/v1/entities",
            http_client.host, http_client.port
        )
        .into();

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
            entities_url,
        })
    }
}
