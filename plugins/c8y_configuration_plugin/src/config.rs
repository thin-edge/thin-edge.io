use crate::{error::ConfigManagementError, DEFAULT_PLUGIN_CONFIG_TYPE};
use c8y_smartrest::topic::C8yTopic;
use mqtt_channel::Message;
use serde::Deserialize;
use std::borrow::Borrow;
use std::collections::HashSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::Path;
use tracing::{info, warn};

#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
struct RawPluginConfig {
    pub files: Vec<RawFileEntry>,
}

#[derive(Deserialize, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RawFileEntry {
    pub path: String,
    #[serde(rename = "type")]
    config_type: Option<String>,
}

#[derive(Debug, Eq, PartialEq, Default, Clone)]
pub struct PluginConfig {
    pub files: HashSet<FileEntry>,
}

#[derive(Debug, Eq, Default, Clone)]
pub struct FileEntry {
    pub path: String,
    config_type: String,
}

impl Hash for FileEntry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.config_type.hash(state);
    }
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

impl FileEntry {
    pub fn new(path: String, config_type: String) -> Self {
        Self { path, config_type }
    }
}

impl RawPluginConfig {
    fn new(config_file_path: &Path) -> Self {
        let path_str = config_file_path.display().to_string();
        info!("Reading the config file from {}", path_str);
        match fs::read_to_string(config_file_path) {
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
}

impl PluginConfig {
    pub fn new(config_file_path: &Path) -> Self {
        let plugin_config = Self::new_with_config_file_entry(config_file_path);
        let raw_config = RawPluginConfig::new(config_file_path);
        plugin_config.add_entries_from_raw_config(raw_config)
    }

    fn new_with_config_file_entry(config_file_path: &Path) -> Self {
        let c8y_configuration_plugin = FileEntry::new(
            config_file_path.display().to_string(),
            DEFAULT_PLUGIN_CONFIG_TYPE.into(),
        );
        Self {
            files: HashSet::from([c8y_configuration_plugin]),
        }
    }

    fn add_entries_from_raw_config(mut self, raw_config: RawPluginConfig) -> Self {
        let original_plugin_config = self.clone();
        for raw_entry in raw_config.files {
            let config_type = raw_entry
                .config_type
                .unwrap_or_else(|| raw_entry.path.clone());
            let entry = FileEntry::new(raw_entry.path, config_type.clone());
            if !self.files.insert(entry) {
                warn!("The config file has the duplicated type '{}'.", config_type);
                return original_plugin_config;
            }
        }
        self
    }

    pub fn to_supported_config_types_message(&self) -> Result<Message, anyhow::Error> {
        let topic = C8yTopic::SmartRestResponse.to_topic()?;
        Ok(Message::new(&topic, self.to_smartrest_payload()))
    }

    pub fn get_all_file_types(&self) -> Vec<String> {
        self.files
            .iter()
            .map(|x| x.config_type.to_string())
            .collect::<Vec<_>>()
    }

    pub fn get_path_from_type(&self, config_type: &str) -> Result<String, ConfigManagementError> {
        let file_entry = self
            .files
            .get(&config_type.to_string())
            .ok_or(ConfigManagementError::InvalidRequestedConfigType {
                config_type: config_type.to_owned(),
            })?
            .to_owned();
        Ok(file_entry.path)
    }

    // 119,typeA,typeB,...
    fn to_smartrest_payload(&self) -> String {
        let mut config_types = self.get_all_file_types();
        // Sort because hashset doesn't guarantee the order
        let () = config_types.sort();
        let supported_config_types = config_types.join(",");
        format!("119,{supported_config_types}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    use test_case::test_case;

    const PLUGIN_CONFIG_FILE: &str = "c8y-configuration-plugin.toml";

    #[test]
    fn deserialize_raw_plugin_config_array_of_tables() {
        let config: RawPluginConfig = toml::from_str(
            r#"
            [[files]]
                path = "/etc/tedge/tedge.toml"
                type = "tedge.toml"
            [[files]]
                type = "tedge.toml"
                path = "/etc/tedge/tedge.toml"
            [[files]]
                path = "/etc/tedge/mosquitto-conf/c8y-bridge.conf"
            [[files]]
                path = "/etc/tedge/mosquitto-conf/tedge-mosquitto.conf"
                type = '"double quotation"'
            [[files]]
                path = "/etc/mosquitto/mosquitto.conf"
                type = "'single quotation'"
             "#,
        )
        .unwrap();

        assert_eq!(
            config.files,
            vec![
                RawFileEntry::new(
                    "/etc/tedge/tedge.toml".to_string(),
                    Some("tedge.toml".to_string())
                ),
                RawFileEntry::new(
                    "/etc/tedge/tedge.toml".to_string(),
                    Some("tedge.toml".to_string())
                ),
                RawFileEntry::new(
                    "/etc/tedge/mosquitto-conf/c8y-bridge.conf".to_string(),
                    None
                ),
                RawFileEntry::new(
                    "/etc/tedge/mosquitto-conf/tedge-mosquitto.conf".to_string(),
                    Some("\"double quotation\"".to_string())
                ),
                RawFileEntry::new(
                    "/etc/mosquitto/mosquitto.conf".to_string(),
                    Some("'single quotation'".to_string())
                )
            ]
        );
    }

    #[test]
    fn deserialize_raw_plugin_config() {
        let config: RawPluginConfig = toml::from_str(
            r#"
                 files = [
                   { path = "/etc/tedge/tedge.toml", type = "tedge.toml" },
                   { type = "tedge.toml", path = "/etc/tedge/tedge.toml" },
                   { path = "/etc/tedge/mosquitto-conf/c8y-bridge.conf" },
                   { path = "/etc/tedge/mosquitto-conf/tedge-mosquitto.conf", type = '"double quotation"' },
                   { path = "/etc/mosquitto/mosquitto.conf", type = "'single quotation'" },
                 ]
             "#,
        )
        .unwrap();

        assert_eq!(
            config.files,
            vec![
                RawFileEntry::new(
                    "/etc/tedge/tedge.toml".to_string(),
                    Some("tedge.toml".to_string())
                ),
                RawFileEntry::new(
                    "/etc/tedge/tedge.toml".to_string(),
                    Some("tedge.toml".to_string())
                ),
                RawFileEntry::new(
                    "/etc/tedge/mosquitto-conf/c8y-bridge.conf".to_string(),
                    None
                ),
                RawFileEntry::new(
                    "/etc/tedge/mosquitto-conf/tedge-mosquitto.conf".to_string(),
                    Some("\"double quotation\"".to_string())
                ),
                RawFileEntry::new(
                    "/etc/mosquitto/mosquitto.conf".to_string(),
                    Some("'single quotation'".to_string())
                )
            ]
        );
    }

    #[test_case(
        r#"
        [[files]]
            path = "/etc/tedge/tedge.toml"
            type = "tedge"
        [[files]]
            path = "/etc/tedge/mosquitto-conf/c8y-bridge.conf"
        "#,
        PluginConfig {
            files: HashSet::from([
                FileEntry::new("/etc/tedge/tedge.toml".to_string(), "tedge".to_string()),
                FileEntry::new("/etc/tedge/mosquitto-conf/c8y-bridge.conf".to_string(), "/etc/tedge/mosquitto-conf/c8y-bridge.conf".to_string()),
            ])
        }; "standard case"
    )]
    #[test_case(
        r#"
        [[files]]
            path = "/etc/tedge/tedge.toml"
            type = "tedge"
        [[files]]
            path = "/etc/tedge/tedge.toml"
            type = "tedge2"
        "#,
        PluginConfig {
            files: HashSet::from([
                FileEntry::new("/etc/tedge/tedge.toml".to_string(), "tedge".to_string()),
                FileEntry::new("/etc/tedge/tedge.toml".to_string(), "tedge2".to_string()),
            ])
        }; "file path duplication"
    )]
    #[test_case(
    r#"
        [[files]]
            path = "/etc/tedge/tedge.toml"
            type = "tedge"
        [[files]]
            path = "/etc/tedge/tedge2.toml"
            type = "tedge"
        "#,
        PluginConfig {
        files: HashSet::new()
    }; "file type duplication"
    )]
    #[test_case(
        r#"files = []"#,
        PluginConfig {
            files: HashSet::new()
        }
        ;"empty case"
    )]
    #[test_case(
        r#"test"#,
        PluginConfig {
            files: HashSet::new()
        }
        ;"not toml"
    )]
    #[test_case(
        r#"
        [[files]]
            path = "/etc/tedge/tedge.toml"
            type = "tedge"
        [[unsupported_key]]
        "#,
        PluginConfig {
            files: HashSet::new()
        }
        ;"unexpected field"
    )]
    fn read_plugin_config_file(
        file_content: &str,
        expected_config: PluginConfig,
    ) -> anyhow::Result<()> {
        let (_dir, config_root_path) = create_temp_plugin_config(file_content)?;
        let tmp_path_to_plugin_config = config_root_path.join(PLUGIN_CONFIG_FILE);
        let tmp_path_to_plugin_config_str =
            tmp_path_to_plugin_config.as_path().display().to_string();

        let config = PluginConfig::new(tmp_path_to_plugin_config.clone());
        let expected_config = expected_config.add_file_entry(
            tmp_path_to_plugin_config_str,
            DEFAULT_PLUGIN_CONFIG_TYPE.into(),
        );

        assert_eq!(config, expected_config);

        Ok(())
    }

    impl RawFileEntry {
        pub fn new(path: String, config_type: Option<String>) -> Self {
            Self { path, config_type }
        }
    }

    // Use this to add a temporary file path of the plugin configuration file
    impl PluginConfig {
        fn add_file_entry(&self, path: String, config_type: String) -> Self {
            let mut files = self.files.clone();
            let _ = files.insert(FileEntry::new(path, config_type));
            Self { files }
        }
    }

    // Need to return TempDir, otherwise the dir will be deleted when this function ends.
    fn create_temp_plugin_config(content: &str) -> std::io::Result<(TempDir, PathBuf)> {
        let temp_dir = TempDir::new()?;
        let config_root = temp_dir.path().to_path_buf();
        let config_file_path = config_root.join(PLUGIN_CONFIG_FILE);
        let mut file = std::fs::File::create(config_file_path.as_path())?;
        file.write_all(content.as_bytes())?;
        Ok((temp_dir, config_root))
    }

    #[test]
    fn get_smartrest_single_type() {
        let plugin_config = PluginConfig {
            files: HashSet::from([FileEntry::new(
                "/path/to/file".to_string(),
                "typeA".to_string(),
            )]),
        };
        let output = plugin_config.to_smartrest_payload();
        assert_eq!(output, "119,typeA");
    }

    #[test]
    fn get_smartrest_multiple_types() {
        let plugin_config = PluginConfig {
            files: HashSet::from([
                FileEntry::new("path1".to_string(), "typeA".to_string()),
                FileEntry::new("path2".to_string(), "typeB".to_string()),
                FileEntry::new("path3".to_string(), "typeC".to_string()),
            ]),
        };
        let output = plugin_config.to_smartrest_payload();
        assert_eq!(output, ("119,typeA,typeB,typeC"));
    }
}
