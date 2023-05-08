use camino::Utf8PathBuf;
use tedge_config::ConfigRepository;
use tedge_config::ConfigSettingAccessor;
use tedge_config::TEdgeConfigError;
use tedge_config::TEdgeConfigLocation;
use tedge_config::TmpPathSetting;

#[derive(Debug, Clone)]
pub struct RestartManagerConfig {
    pub tmp_dir: Utf8PathBuf,
    pub config_dir: Utf8PathBuf,
}

impl RestartManagerConfig {
    pub fn new(tmp_dir: &Utf8PathBuf, config_dir: &Utf8PathBuf) -> Self {
        Self {
            tmp_dir: tmp_dir.clone(),
            config_dir: config_dir.clone(),
        }
    }

    pub fn from_tedge_config(
        tedge_config_location: &TEdgeConfigLocation,
    ) -> Result<RestartManagerConfig, TEdgeConfigError> {
        let config_repository =
            tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
        let tedge_config = config_repository.load()?;

        let tmp_dir = tedge_config.query(TmpPathSetting)?;
        let config_dir = tedge_config_location.tedge_config_root_path.clone();

        Ok(Self::new(&tmp_dir, &config_dir))
    }
}
