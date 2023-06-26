use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use tedge_config::ReadError;
use tedge_config::TEdgeConfig;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;

pub const DEFAULT_PLUGIN_CONFIG_FILE_NAME: &str = "tedge-log-plugin.toml";
pub const DEFAULT_PLUGIN_CONFIG_DIR_NAME: &str = "plugins/";

/// Configuration of the Configuration Manager
#[derive(Clone, Debug)]
pub struct LogManagerConfig {
    pub config_dir: PathBuf,
    pub topic_root: String,
    pub topic_identifier: String,
    pub plugin_config_dir: PathBuf,
    pub plugin_config_path: PathBuf,
    pub logtype_reload_topic: Topic,
    pub logfile_request_topic: TopicFilter,
    pub current_operations: HashSet<String>,
}

impl LogManagerConfig {
    pub fn from_tedge_config(
        config_dir: impl AsRef<Path>,
        _tedge_config: &TEdgeConfig,
        topic_root: String,
        topic_identifier: String,
    ) -> Result<Self, ReadError> {
        let config_dir: PathBuf = config_dir.as_ref().into();

        let plugin_config_dir = config_dir.join(DEFAULT_PLUGIN_CONFIG_DIR_NAME);
        let plugin_config_path = plugin_config_dir.join(DEFAULT_PLUGIN_CONFIG_FILE_NAME);

        let logtype_reload_topic = Topic::new_unchecked(
            format!("{}/{}/cmd/log_upload", topic_root, topic_identifier).as_str(),
        );
        let logfile_request_topic = TopicFilter::new_unchecked(
            format!("{}/{}/cmd/log_upload/+", topic_root, topic_identifier).as_str(),
        );
        let current_operations = HashSet::new();

        Ok(Self {
            config_dir,
            topic_root,
            topic_identifier,
            plugin_config_dir,
            plugin_config_path,
            logtype_reload_topic,
            logfile_request_topic,
            current_operations,
        })
    }
}

#[derive(Clone, Deserialize, Debug, Eq, PartialEq, Default)]
pub struct LogPluginConfig {
    pub files: Vec<FileEntry>,
}

#[derive(Deserialize, Debug, Eq, Default, Clone)]
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

    pub fn to_supported_config_types_message(
        &self,
        topic: &Topic,
    ) -> Result<MqttMessage, anyhow::Error> {
        Ok(MqttMessage::new(topic, self.to_payload()).with_retain())
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

    fn to_payload(&self) -> String {
        let mut config_types = self.get_all_file_types();
        config_types.sort();
        json!({ "types": config_types }).to_string()
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
