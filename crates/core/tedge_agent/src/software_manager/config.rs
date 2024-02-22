use camino::Utf8PathBuf;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_config::TEdgeConfigLocation;
#[derive(Debug, Clone)]
pub struct SoftwareManagerConfig {
    pub device: EntityTopicId,
    pub tmp_dir: Utf8PathBuf,
    pub config_dir: Utf8PathBuf,
    pub state_dir: Utf8PathBuf,
    pub sm_plugins_dir: Utf8PathBuf,
    pub log_dir: Utf8PathBuf,
    pub default_plugin_type: Option<String>,
    pub config_location: TEdgeConfigLocation,
}

impl SoftwareManagerConfig {
    pub fn from_tedge_config(
        tedge_config_location: &TEdgeConfigLocation,
    ) -> Result<SoftwareManagerConfig, tedge_config::TEdgeConfigError> {
        let tedge_config = tedge_config::TEdgeConfig::new(tedge_config_location.clone())?;
        let config_dir = &tedge_config_location.tedge_config_root_path;

        let default_plugin_type = tedge_config
            .software
            .plugin
            .default
            .clone()
            .or_none()
            .cloned();

        let device = tedge_config
            .mqtt
            .device_topic_id
            .parse()
            .unwrap_or(EntityTopicId::default_main_device());

        Ok(SoftwareManagerConfig {
            device,
            tmp_dir: tedge_config.tmp.path.clone(),
            config_dir: config_dir.clone(),
            state_dir: tedge_config.agent.state.path.clone(),
            sm_plugins_dir: config_dir.join("sm-plugins"),
            log_dir: tedge_config.logs.path.join("agent"),
            default_plugin_type,
            config_location: tedge_config_location.clone(),
        })
    }
}
