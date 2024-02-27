use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tedge_config::ReadError;
use tedge_config::TEdgeConfig;
use tedge_config::TopicPrefix;

pub const DEFAULT_PLUGIN_CONFIG_FILE_NAME: &str = "c8y-log-plugin.toml";
pub const DEFAULT_PLUGIN_CONFIG_DIR_NAME: &str = "c8y/";

/// Configuration of the Configuration Manager
#[derive(Clone, Debug)]
pub struct LogManagerConfig {
    pub config_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub log_dir: PathBuf,
    pub device_id: String,
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub tedge_http_host: Arc<str>,
    pub tedge_http_port: u16,
    pub ops_dir: PathBuf,
    pub plugin_config_dir: PathBuf,
    pub plugin_config_path: PathBuf,
    pub c8y_prefix: TopicPrefix,
}

impl LogManagerConfig {
    pub fn from_tedge_config(
        config_dir: impl AsRef<Path>,
        tedge_config: &TEdgeConfig,
    ) -> Result<Self, ReadError> {
        let config_dir: PathBuf = config_dir.as_ref().into();
        let device_id = tedge_config.device.id.try_read(tedge_config)?.to_string();
        let tmp_dir = tedge_config.tmp.path.as_std_path().to_path_buf();
        let log_dir = tedge_config.logs.path.as_std_path().to_path_buf();
        let mqtt_host = tedge_config.mqtt.client.host.clone();
        let mqtt_port = u16::from(tedge_config.mqtt.client.port);
        let c8y_prefix = tedge_config.c8y.bridge.topic_prefix.clone();

        let tedge_http_host = tedge_config.http.client.host.clone();
        let tedge_http_port = tedge_config.http.client.port;

        let ops_dir = config_dir.join("operations/c8y");

        let plugin_config_dir = config_dir.join(DEFAULT_PLUGIN_CONFIG_DIR_NAME);

        let plugin_config_path = plugin_config_dir.join(DEFAULT_PLUGIN_CONFIG_FILE_NAME);

        Ok(Self {
            config_dir,
            tmp_dir,
            log_dir,
            device_id,
            mqtt_host,
            mqtt_port,
            tedge_http_host,
            tedge_http_port,
            ops_dir,
            plugin_config_dir,
            plugin_config_path,
            c8y_prefix,
        })
    }
}
