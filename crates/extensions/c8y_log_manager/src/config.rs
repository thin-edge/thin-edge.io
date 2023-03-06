use c8y_api::smartrest::topic::C8yTopic;
use log::info;
use log::warn;
use serde::Deserialize;
use std::borrow::Borrow;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tedge_config::C8yUrlSetting;
use tedge_config::ConfigSettingAccessor;
use tedge_config::ConnectUrl;
use tedge_config::DeviceIdSetting;
use tedge_config::HttpBindAddressSetting;
use tedge_config::HttpPortSetting;
use tedge_config::IpAddress;
use tedge_config::MqttClientHostSetting;
use tedge_config::MqttClientPortSetting;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigError;
use tedge_config::TmpPathSetting;
use tedge_mqtt_ext::MqttMessage;

pub const DEFAULT_PLUGIN_CONFIG_FILE_NAME: &str = "c8y-log-plugin.toml";
pub const DEFAULT_OPERATION_DIR_NAME: &str = "c8y/";

/// Configuration of the Configuration Manager
#[derive(Clone, Debug)]
pub struct LogManagerConfig {
    pub config_dir: PathBuf,
    pub tmp_dir: PathBuf,
    pub device_id: String,
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub c8y_url: ConnectUrl,
    pub tedge_http_host: IpAddress,
    pub tedge_http_port: u16,
    pub plugin_config_path: PathBuf,
    pub plugin_config: LogPluginConfig,
}

impl LogManagerConfig {
    pub fn from_tedge_config(
        config_dir: impl AsRef<Path>,
        tedge_config: &TEdgeConfig,
    ) -> Result<Self, TEdgeConfigError> {
        let config_dir: PathBuf = config_dir.as_ref().into();

        let device_id = tedge_config.query(DeviceIdSetting)?;
        let tmp_dir = tedge_config.query(TmpPathSetting)?.into();
        let mqtt_host = tedge_config.query(MqttClientHostSetting)?;
        let mqtt_port = tedge_config.query(MqttClientPortSetting)?.into();

        let c8y_url = tedge_config.query(C8yUrlSetting)?;

        let tedge_http_host = tedge_config.query(HttpBindAddressSetting)?;
        let tedge_http_port: u16 = tedge_config.query(HttpPortSetting)?.into();

        let plugin_config_path = config_dir
            .join(DEFAULT_OPERATION_DIR_NAME)
            .join(DEFAULT_PLUGIN_CONFIG_FILE_NAME);

        let plugin_config = LogPluginConfig::new(&plugin_config_path);

        Ok(Self {
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
