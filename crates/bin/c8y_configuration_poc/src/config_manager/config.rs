use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::TopicFilter;
use std::path::Path;
use std::path::PathBuf;
use tedge_api::health::health_check_topics;
use tedge_config::*;

use super::child_device::ConfigOperationResponseTopic;
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
    pub tedge_http_host: String,
    pub plugin_config_path: PathBuf,
    pub plugin_config: PluginConfig,
    pub c8y_request_topics: TopicFilter,
    pub health_check_topics: TopicFilter,
    pub config_snapshot_response_topics: TopicFilter,
    pub config_update_response_topics: TopicFilter,
}

impl ConfigManagerConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config_dir: PathBuf,
        tmp_dir: PathBuf,
        device_id: String,
        mqtt_host: IpAddress,
        mqtt_port: u16,
        c8y_url: ConnectUrl,
        tedge_http_address: IpAddress,
        tedge_http_port: u16,
    ) -> Self {
        let tedge_http_host = format!("{}:{}", tedge_http_address, tedge_http_port);

        let plugin_config_path = config_dir
            .join(DEFAULT_OPERATION_DIR_NAME)
            .join(DEFAULT_PLUGIN_CONFIG_FILE_NAME);

        let plugin_config = PluginConfig::new(&plugin_config_path);

        let c8y_request_topics: TopicFilter = C8yTopic::SmartRestRequest.into();
        let health_check_topics = health_check_topics(DEFAULT_PLUGIN_CONFIG_TYPE);
        let config_snapshot_response_topics: TopicFilter =
            ConfigOperationResponseTopic::SnapshotResponse.into();
        let config_update_response_topics: TopicFilter =
            ConfigOperationResponseTopic::UpdateResponse.into();

        ConfigManagerConfig {
            config_dir,
            tmp_dir,
            device_id,
            mqtt_host,
            mqtt_port,
            c8y_url,
            tedge_http_host,
            plugin_config_path,
            plugin_config,
            c8y_request_topics,
            health_check_topics,
            config_snapshot_response_topics,
            config_update_response_topics,
        }
    }

    pub fn from_tedge_config(
        config_dir: impl AsRef<Path>,
        tedge_config: &TEdgeConfig,
    ) -> Result<ConfigManagerConfig, TEdgeConfigError> {
        let config_dir: PathBuf = config_dir.as_ref().into();
        let device_id = tedge_config.query(DeviceIdSetting)?;
        let tmp_dir = tedge_config.query(TmpPathSetting)?.into();
        let mqtt_host = tedge_config.query(MqttBindAddressSetting)?;
        let mqtt_port = tedge_config.query(MqttPortSetting)?.into();
        let c8y_url = tedge_config.query(C8yUrlSetting)?;
        let tedge_http_address = tedge_config.query(HttpBindAddressSetting)?;
        let tedge_http_port: u16 = tedge_config.query(HttpPortSetting)?.into();

        Ok(ConfigManagerConfig::new(
            config_dir,
            tmp_dir,
            device_id,
            mqtt_host,
            mqtt_port,
            c8y_url,
            tedge_http_address,
            tedge_http_port,
        ))
    }
}
