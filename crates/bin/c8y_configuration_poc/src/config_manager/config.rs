use std::path::{Path, PathBuf};
use tedge_config::*;

/// Configuration of the Configuration Manager
#[derive(Clone, Debug)]
pub struct ConfigConfigManager {
    pub config_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub device_id: String,
    pub mqtt_host: IpAddress,
    pub mqtt_port: u16,
    pub c8y_url: ConnectUrl,
    pub tedge_http_host: IpAddress,
    pub tedge_http_port: u16,
}

impl ConfigConfigManager {
    pub fn from_default_tedge_config() -> Result<ConfigConfigManager, TEdgeConfigError> {
        ConfigConfigManager::from_tedge_config(DEFAULT_TEDGE_CONFIG_PATH)
    }

    pub fn from_tedge_config(
        config_dir: impl AsRef<Path>,
    ) -> Result<ConfigConfigManager, TEdgeConfigError> {
        let config_dir: PathBuf = config_dir.as_ref().into();
        let config_location =
            tedge_config::TEdgeConfigLocation::from_custom_root(config_dir.clone());
        let config_repository = tedge_config::TEdgeConfigRepository::new(config_location);
        let tedge_config = config_repository.load()?;

        let device_id = tedge_config.query(DeviceIdSetting)?;
        let tmp_dir = tedge_config.query(TmpPathSetting)?.into();
        let mqtt_host = tedge_config.query(MqttBindAddressSetting)?;
        let mqtt_port = tedge_config.query(MqttPortSetting)?.into();

        let c8y_url = tedge_config.query(C8yUrlSetting)?;

        let tedge_http_host = tedge_config.query(HttpBindAddressSetting)?;
        let tedge_http_port: u16 = tedge_config.query(HttpPortSetting)?.into();

        Ok(ConfigConfigManager {
            config_dir,
            tmp_dir,
            device_id,
            mqtt_host,
            mqtt_port,
            c8y_url,
            tedge_http_host,
            tedge_http_port,
        })
    }
}
