use super::child_device::ConfigOperationResponseTopic;
use c8y_api::smartrest::topic::C8yTopic;
use std::path::Path;
use std::path::PathBuf;
use tedge_config::*;
use tedge_mqtt_ext::TopicFilter;

pub const DEFAULT_PLUGIN_CONFIG_FILE_NAME: &str = "c8y-configuration-plugin.toml";
pub const DEFAULT_OPERATION_DIR_NAME: &str = "c8y/";
pub const DEFAULT_PLUGIN_CONFIG_TYPE: &str = "c8y-configuration-plugin";

/// Configuration of the Configuration Manager
#[derive(Clone, Debug)]
pub struct ConfigManagerConfig {
    pub config_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub file_transfer_dir: PathBuf,
    pub device_id: String,
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub tedge_http_host: String,
    pub ops_dir: PathBuf,
    pub plugin_config_dir: PathBuf,
    pub plugin_config_path: PathBuf,
    pub c8y_request_topics: TopicFilter,
    pub config_snapshot_response_topics: TopicFilter,
    pub config_update_response_topics: TopicFilter,
}

impl ConfigManagerConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config_dir: PathBuf,
        tmp_dir: PathBuf,
        data_dir: PathBuf,
        device_id: String,
        mqtt_host: String,
        mqtt_port: u16,
        tedge_http_address: IpAddress,
        tedge_http_port: u16,
    ) -> Self {
        let tedge_http_host = format!("{}:{}", tedge_http_address, tedge_http_port);

        let ops_dir = config_dir.join("operations/c8y");
        let plugin_config_dir = config_dir.join(DEFAULT_OPERATION_DIR_NAME);
        let plugin_config_path = plugin_config_dir.join(DEFAULT_PLUGIN_CONFIG_FILE_NAME);

        let file_transfer_dir = data_dir.join(DEFAULT_FILE_TRANSFER_DIR_NAME);

        let c8y_request_topics: TopicFilter = C8yTopic::SmartRestRequest.into();
        let config_snapshot_response_topics: TopicFilter =
            ConfigOperationResponseTopic::SnapshotResponse.into();
        let config_update_response_topics: TopicFilter =
            ConfigOperationResponseTopic::UpdateResponse.into();

        ConfigManagerConfig {
            config_dir,
            tmp_dir,
            file_transfer_dir,
            device_id,
            mqtt_host,
            mqtt_port,
            tedge_http_host,
            ops_dir,
            plugin_config_dir,
            plugin_config_path,
            c8y_request_topics,
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
        let data_dir: PathBuf = tedge_config.query(DataPathSetting)?.into();
        let mqtt_host = tedge_config.query(MqttClientHostSetting)?;
        let mqtt_port = tedge_config.query(MqttClientPortSetting)?.into();
        let tedge_http_address = tedge_config.query(HttpBindAddressSetting)?;
        let tedge_http_port: u16 = tedge_config.query(HttpPortSetting)?.into();

        let config = ConfigManagerConfig::new(
            config_dir,
            tmp_dir,
            data_dir,
            device_id,
            mqtt_host,
            mqtt_port,
            tedge_http_address,
            tedge_http_port,
        );
        Ok(config)
    }
}
