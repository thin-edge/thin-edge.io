use camino::Utf8PathBuf;
use std::path::Path;
use std::path::PathBuf;
use tedge_config::C8yUrlSetting;
use tedge_config::ConfigSettingAccessor;
use tedge_config::DeviceIdSetting;
use tedge_config::DeviceTypeSetting;
use tedge_config::LogPathSetting;
use tedge_config::ServiceTypeSetting;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigError;

pub const MQTT_MESSAGE_SIZE_THRESHOLD: usize = 16184;

pub struct C8yMapperConfig {
    pub config_dir: PathBuf,
    pub logs_path: Utf8PathBuf,
    pub device_id: String,
    pub device_type: String,
    pub service_type: String,
    pub ops_dir: PathBuf,
    pub c8y_host: String,
}

impl C8yMapperConfig {
    pub fn new(
        config_dir: PathBuf,
        logs_path: Utf8PathBuf,
        device_id: String,
        device_type: String,
        service_type: String,
        c8y_host: String,
    ) -> Self {
        let ops_dir = config_dir.join("operations").join("c8y");

        Self {
            config_dir,
            logs_path,
            device_id,
            device_type,
            service_type,
            ops_dir,
            c8y_host,
        }
    }

    pub fn from_tedge_config(
        config_dir: impl AsRef<Path>,
        tedge_config: &TEdgeConfig,
    ) -> Result<C8yMapperConfig, TEdgeConfigError> {
        let config_dir: PathBuf = config_dir.as_ref().into();

        let logs_path = tedge_config.query(LogPathSetting)?;
        let device_id = tedge_config.query(DeviceIdSetting)?;
        let device_type = tedge_config.query(DeviceTypeSetting)?;
        let service_type = tedge_config.query(ServiceTypeSetting)?;
        let c8y_host = tedge_config.query(C8yUrlSetting)?.into();

        Ok(C8yMapperConfig::new(
            config_dir,
            logs_path,
            device_id,
            device_type,
            service_type,
            c8y_host,
        ))
    }
}