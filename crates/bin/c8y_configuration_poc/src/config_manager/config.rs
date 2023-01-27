use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::TopicFilter;
use std::path::Path;
use std::path::PathBuf;
use tedge_api::health::health_check_topics;
use tedge_config::*;

use super::plugin_config::PluginConfig;

pub const DEFAULT_PLUGIN_CONFIG_FILE_NAME: &str = "c8y-configuration-plugin.toml";
pub const DEFAULT_OPERATION_DIR_NAME: &str = "c8y/";
pub const DEFAULT_PLUGIN_CONFIG_TYPE: &str = "c8y-configuration-plugin";

/// Configuration of the Configuration Manager
#[derive(Clone, Debug)]
pub struct ConfigManagerConfig {
    pub config_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub device_id: String,
    pub mqtt_host: IpAddress,
    pub mqtt_port: u16,
    pub c8y_url: ConnectUrl,
    pub tedge_http_host: IpAddress,
    pub tedge_http_port: u16,
    pub plugin_config_path: PathBuf,
    pub plugin_config: PluginConfig,
    pub c8y_request_topics: TopicFilter,
    pub health_check_topics: TopicFilter,
}

impl ConfigManagerConfig {
    pub fn from_default_tedge_config() -> Result<ConfigManagerConfig, TEdgeConfigError> {
        ConfigManagerConfig::from_tedge_config(DEFAULT_TEDGE_CONFIG_PATH)
    }

    pub fn from_tedge_config(
        config_dir: impl AsRef<Path>,
    ) -> Result<ConfigManagerConfig, TEdgeConfigError> {
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

        let plugin_config_path = config_dir
            .join(DEFAULT_OPERATION_DIR_NAME)
            .join(DEFAULT_PLUGIN_CONFIG_FILE_NAME);

        let plugin_config = PluginConfig::new(&plugin_config_path);

        let c8y_request_topics: TopicFilter = C8yTopic::SmartRestRequest.into();
        let health_check_topics = health_check_topics(DEFAULT_PLUGIN_CONFIG_TYPE);

        Ok(ConfigManagerConfig {
            config_dir,
            tmp_dir,
            device_id,
            mqtt_host,
            mqtt_port,
            c8y_url,
            tedge_http_host,
            tedge_http_port,
            plugin_config_path,
            plugin_config,
            c8y_request_topics,
            health_check_topics,
        })
    }
}
