use camino::Utf8PathBuf;
use tedge_api::mqtt_topics::EntityTopicId;

#[derive(Debug, Clone)]
pub struct RestartManagerConfig {
    pub device_topic_id: EntityTopicId,
    pub tmp_dir: Utf8PathBuf,
    pub config_dir: Utf8PathBuf,
    pub state_dir: Utf8PathBuf,
    pub is_sudo_enabled: bool,
}

impl RestartManagerConfig {
    pub fn from_tedge_config(
        device_topic_id: &EntityTopicId,
        tedge_config_location: &tedge_config::TEdgeConfigLocation,
    ) -> Result<RestartManagerConfig, tedge_config::TEdgeConfigError> {
        let tedge_config = tedge_config::TEdgeConfig::try_new(tedge_config_location.clone())?;

        Ok(RestartManagerConfig {
            device_topic_id: device_topic_id.clone(),
            tmp_dir: tedge_config.tmp.path.clone(),
            config_dir: tedge_config_location.tedge_config_root_path.clone(),
            state_dir: tedge_config.agent.state.path.clone(),
            is_sudo_enabled: tedge_config.sudo.enable,
        })
    }
}
