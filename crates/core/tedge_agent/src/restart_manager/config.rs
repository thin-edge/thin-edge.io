use camino::Utf8PathBuf;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_config::SudoCommandBuilder;
use tedge_utils::paths::TedgePaths;

#[derive(Debug, Clone)]
pub struct RestartManagerConfig {
    pub device_topic_id: EntityTopicId,
    pub tmp_dir: Utf8PathBuf,
    pub config_dir: TedgePaths,
    pub state_dir: TedgePaths,
    pub sudo: SudoCommandBuilder,
}

impl RestartManagerConfig {
    pub async fn from_tedge_config(
        device_topic_id: &EntityTopicId,
        tedge_config: &tedge_config::TEdgeConfig,
    ) -> Result<RestartManagerConfig, tedge_config::TEdgeConfigError> {
        Ok(RestartManagerConfig {
            device_topic_id: device_topic_id.clone(),
            tmp_dir: tedge_config.tmp.path.clone().into(),
            config_dir: tedge_config.config_root(),
            state_dir: tedge_config.state_root(),
            sudo: SudoCommandBuilder::new(tedge_config),
        })
    }
}
