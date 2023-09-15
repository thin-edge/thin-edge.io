use camino::Utf8PathBuf;
use tedge_api::mqtt_topics::EntityTopicId;

#[derive(Debug, Clone)]
pub struct RestartManagerConfig {
    pub device_topic_id: EntityTopicId,
    pub tmp_dir: Utf8PathBuf,
    pub config_dir: Utf8PathBuf,
}

impl RestartManagerConfig {
    pub fn new(
        device_topic_id: &EntityTopicId,
        tmp_dir: &Utf8PathBuf,
        config_dir: &Utf8PathBuf,
    ) -> Self {
        Self {
            device_topic_id: device_topic_id.clone(),
            tmp_dir: tmp_dir.clone(),
            config_dir: config_dir.clone(),
        }
    }

    pub fn from_tedge_config(
        device_topic_id: &EntityTopicId,
        tedge_config_location: &tedge_config::TEdgeConfigLocation,
    ) -> Result<RestartManagerConfig, tedge_config::TEdgeConfigError> {
        let config_repository =
            tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
        let tedge_config = config_repository.load()?;

        let tmp_dir = tedge_config.tmp.path.clone();
        let config_dir = tedge_config_location.tedge_config_root_path.clone();

        Ok(Self::new(device_topic_id, &tmp_dir, &config_dir))
    }
}
