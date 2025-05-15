use camino::Utf8PathBuf;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_config::SudoCommandBuilder;

#[derive(Debug, Clone)]
pub struct RestartManagerConfig {
    pub device_topic_id: EntityTopicId,
    pub tmp_dir: Utf8PathBuf,
    pub config_dir: Utf8PathBuf,
    pub state_dir: Utf8PathBuf,
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
            config_dir: tedge_config.location().tedge_config_root_path.clone(),
            state_dir: tedge_config.agent.state.path.clone().into(),
            sudo: SudoCommandBuilder::new(tedge_config),
        })
    }
}
