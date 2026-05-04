use camino::Utf8PathBuf;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_config::SudoCommandBuilder;
use tedge_config::TEdgeConfig;
use tedge_utils::paths::ManagedDir;
use tedge_utils::paths::TedgePaths;

#[derive(Debug, Clone)]
pub struct SoftwareManagerConfig {
    pub device: EntityTopicId,
    pub tmp_dir: Utf8PathBuf, // TODO: update to TedgePaths?
    pub config_dir: TedgePaths,
    pub state_dir: TedgePaths,
    pub sm_plugins_dir: ManagedDir,
    pub log_dir: ManagedDir,
    pub default_plugin_type: Option<String>,
    pub sudo: SudoCommandBuilder,
}

impl SoftwareManagerConfig {
    pub async fn from_tedge_config(
        tedge_config: &TEdgeConfig,
    ) -> Result<SoftwareManagerConfig, tedge_config::TEdgeConfigError> {
        let config_dir = tedge_config.config_root();
        let default_plugin_type = tedge_config
            .software
            .plugin
            .default
            .clone()
            .or_none()
            .cloned();

        let device = tedge_config.mqtt.device_topic_id.clone();

        Ok(SoftwareManagerConfig {
            device,
            tmp_dir: tedge_config.tmp.path.clone().into(),
            config_dir: config_dir.clone(),
            state_dir: tedge_config.state_root(),
            sm_plugins_dir: config_dir.dir("sm-plugins")?,
            log_dir: tedge_config.logs_root().dir("agent")?,
            default_plugin_type,
            sudo: SudoCommandBuilder::new(tedge_config),
        })
    }
}
