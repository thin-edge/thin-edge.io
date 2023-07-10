use c8y_api::smartrest::topic::C8yTopic;
use log::info;
use log::warn;
use serde::Deserialize;
use std::borrow::Borrow;
use std::collections::HashSet;
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::path::PathBuf;
use tedge_config::new::ReadError;
use tedge_config::new::TEdgeConfig;
use tedge_mqtt_ext::MqttMessage;

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
    pub tedge_http_host: IpAddr,
    pub tedge_http_port: u16,
    pub ops_dir: PathBuf,
    pub plugin_config_dir: PathBuf,
    pub plugin_config_path: PathBuf,
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

        let tedge_http_host = tedge_config.http.bind.address;
        let tedge_http_port = tedge_config.http.bind.port;

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
        })
    }
}

#[derive(Clone, Deserialize, Debug, Eq, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct LogPluginConfig {
    pub files: Vec<FileEntry>,
}

#[derive(Deserialize, Debug, Eq, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    pub(crate) path: String,
    #[serde(rename = "type")]
    pub config_type: String,
}

impl PartialEq for FileEntry {
    fn eq(&self, other: &Self) -> bool {
        self.config_type == other.config_type
    }
}

impl Borrow<String> for FileEntry {
    fn borrow(&self) -> &String {
        &self.config_type
    }
}

impl LogPluginConfig {
    pub fn new(config_file_path: &Path) -> Self {
        Self::read_config(config_file_path)
    }

    pub fn read_config(path: &Path) -> Self {
        let path_str = path.display().to_string();
        info!("Using the configuration from {}", path_str);
        match fs::read_to_string(path) {
            Ok(contents) => match toml::from_str(contents.as_str()) {
                Ok(config) => config,
                _ => {
                    warn!("The config file {} is malformed.", path_str);
                    Self::default()
                }
            },
            Err(_) => {
                warn!(
                    "The config file {} does not exist or is not readable.",
                    path_str
                );
                Self::default()
            }
        }
    }

    pub fn to_supported_config_types_message(&self) -> Result<MqttMessage, anyhow::Error> {
        let topic = C8yTopic::SmartRestResponse.to_topic()?;
        Ok(MqttMessage::new(&topic, self.to_smartrest_payload()))
    }

    pub fn get_all_file_types(&self) -> Vec<String> {
        self.files
            .iter()
            .map(|x| x.config_type.to_string())
            .collect::<HashSet<_>>()
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
    }

    // 118,typeA,typeB,...
    fn to_smartrest_payload(&self) -> String {
        let mut config_types = self.get_all_file_types();
        config_types.sort();
        let supported_config_types = config_types.join(",");
        format!("118,{supported_config_types}")
    }
}

#[test]
fn test_no_duplicated_file_types() {
    let files = vec![
        FileEntry {
            path: "a/path".to_string(),
            config_type: "type_one".to_string(),
        },
        FileEntry {
            path: "some/path".to_string(),
            config_type: "type_one".to_string(),
        },
    ];
    let logs_config = LogPluginConfig { files };
    assert_eq!(
        logs_config.get_all_file_types(),
        vec!["type_one".to_string()]
    );
}
