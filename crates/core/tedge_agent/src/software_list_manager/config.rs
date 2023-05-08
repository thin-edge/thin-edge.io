use camino::Utf8PathBuf;
use tedge_config::ConfigRepository;
use tedge_config::ConfigSettingAccessor;
use tedge_config::ConfigSettingAccessorStringExt;
use tedge_config::LogPathSetting;
use tedge_config::SoftwarePluginDefaultSetting;
use tedge_config::TEdgeConfigError;
use tedge_config::TEdgeConfigLocation;
use tedge_config::TmpPathSetting;

#[derive(Debug, Clone)]
pub struct SoftwareListManagerConfig {
    pub tmp_dir: Utf8PathBuf,
    pub config_dir: Utf8PathBuf,
    pub sm_plugins_dir: Utf8PathBuf,
    pub log_dir: Utf8PathBuf,
    pub default_plugin_type: Option<String>,
}

impl SoftwareListManagerConfig {
    pub fn new(
        tmp_dir: &Utf8PathBuf,
        config_dir: &Utf8PathBuf,
        sm_plugins_dir: &Utf8PathBuf,
        log_dir: &Utf8PathBuf,
        default_plugin_type: Option<String>,
    ) -> Self {
        Self {
            tmp_dir: tmp_dir.clone(),
            config_dir: config_dir.clone(),
            sm_plugins_dir: sm_plugins_dir.clone(),
            log_dir: log_dir.clone(),
            default_plugin_type,
        }
    }

    pub fn from_tedge_config(
        tedge_config_location: &TEdgeConfigLocation,
    ) -> Result<SoftwareListManagerConfig, TEdgeConfigError> {
        let config_repository =
            tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
        let tedge_config = config_repository.load()?;

        let config_dir = tedge_config_location.tedge_config_root_path.clone();

        let tmp_dir = tedge_config.query(TmpPathSetting)?;
        let sm_plugins_dir = config_dir.join("sm-plugins");
        let log_dir = tedge_config
            .query(LogPathSetting)?
            .join("tedge")
            .join("agent");
        let default_plugin_type =
            tedge_config.query_string_optional(SoftwarePluginDefaultSetting)?;

        Ok(Self::new(
            &tmp_dir,
            &config_dir,
            &sm_plugins_dir,
            &log_dir,
            default_plugin_type,
        ))
    }
}
