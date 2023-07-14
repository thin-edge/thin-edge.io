use camino::Utf8PathBuf;
use tedge_config::TEdgeConfigLocation;
#[derive(Debug, Clone)]
pub struct SoftwareManagerConfig {
    pub tmp_dir: Utf8PathBuf,
    pub config_dir: Utf8PathBuf,
    pub sm_plugins_dir: Utf8PathBuf,
    pub log_dir: Utf8PathBuf,
    pub default_plugin_type: Option<String>,
    pub config_location: TEdgeConfigLocation,
}

impl SoftwareManagerConfig {
    pub fn new(
        tmp_dir: &Utf8PathBuf,
        config_dir: &Utf8PathBuf,
        sm_plugins_dir: &Utf8PathBuf,
        log_dir: &Utf8PathBuf,
        default_plugin_type: Option<String>,
        config_location: &TEdgeConfigLocation,
    ) -> Self {
        Self {
            tmp_dir: tmp_dir.clone(),
            config_dir: config_dir.clone(),
            sm_plugins_dir: sm_plugins_dir.clone(),
            log_dir: log_dir.clone(),
            default_plugin_type,
            config_location: config_location.clone(),
        }
    }

    pub fn from_tedge_config(
        tedge_config_location: &TEdgeConfigLocation,
    ) -> Result<SoftwareManagerConfig, tedge_config::TEdgeConfigError> {
        let config_repository =
            tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
        let tedge_config = config_repository.load_new()?;

        let config_dir = tedge_config_location.tedge_config_root_path.clone();

        let tmp_dir = &tedge_config.tmp.path;
        let sm_plugins_dir = config_dir.join("sm-plugins");
        let log_dir = tedge_config.logs.path.join("tedge").join("agent");
        let default_plugin_type = tedge_config
            .software
            .plugin
            .default
            .clone()
            .or_none()
            .cloned();

        Ok(Self::new(
            tmp_dir,
            &config_dir,
            &sm_plugins_dir,
            &log_dir,
            default_plugin_type,
            tedge_config_location,
        ))
    }
}
