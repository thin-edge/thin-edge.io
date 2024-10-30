use log::info;
use log::warn;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

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

    pub fn get_all_file_types(&self) -> Vec<String> {
        self.files
            .iter()
            .map(|x| x.config_type.to_string())
            .collect::<HashSet<_>>()
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
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
