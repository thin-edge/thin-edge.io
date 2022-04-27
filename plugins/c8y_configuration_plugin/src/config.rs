use c8y_smartrest::topic::C8yTopic;
use mqtt_channel::Message;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Deserialize, Debug, Eq, PartialEq, Default)]
#[serde(deny_unknown_fields)]
pub struct PluginConfig {
    pub files: HashSet<FileEntry>,
}

#[derive(Deserialize, Debug, Eq, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    path: String,
}

impl Hash for FileEntry {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state);
    }
}

impl PartialEq for FileEntry {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl FileEntry {
    pub(crate) fn new(path: String) -> Self {
        Self { path }
    }
}

impl PluginConfig {
    pub fn new(config_file_path: PathBuf) -> Self {
        let config_file_path_str = config_file_path.as_path().display().to_string();
        Self::read_config(config_file_path).add_file(config_file_path_str)
    }

    fn read_config(path: PathBuf) -> Self {
        let path_str = path.as_path().display().to_string();
        info!("Reading the config file from {}", path_str);
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

    fn add_file(&self, file: String) -> Self {
        let mut files = self.files.clone();
        let _ = files.insert(FileEntry::new(file));
        Self { files }
    }

    pub fn to_supported_config_types_message(&self) -> Result<Message, anyhow::Error> {
        let topic = C8yTopic::SmartRestResponse.to_topic()?;
        Ok(Message::new(&topic, self.to_smartrest_payload()))
    }

    pub fn get_all_file_paths(&self) -> Vec<String> {
        self.files
            .iter()
            .map(|x| x.path.to_string())
            .collect::<Vec<_>>()
    }

    // 119,typeA,typeB,...
    fn to_smartrest_payload(&self) -> String {
        let mut config_types = self.get_all_file_paths();
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
    fn deserialize_plugin_config() {
        let config: PluginConfig = toml::from_str(
            r#"
            [[files]]
                path =  "/etc/tedge/tedge.toml"
            [[files]]
                path =  "/etc/tedge/mosquitto-conf/c8y-bridge.conf"
            [[files]]
                path =  "/etc/tedge/mosquitto-conf/tedge-mosquitto.conf"
            [[files]]
              path =  "/etc/mosquitto/mosquitto.conf"
             "#,
        )
        .unwrap();

        assert_eq!(
            config.files,
            HashSet::from([
                FileEntry::new("/etc/tedge/tedge.toml".to_string()),
                FileEntry::new("/etc/tedge/mosquitto-conf/c8y-bridge.conf".to_string(),),
                FileEntry::new("/etc/tedge/mosquitto-conf/tedge-mosquitto.conf".to_string(),),
                FileEntry::new("/etc/mosquitto/mosquitto.conf".to_string())
            ])
        );
    }

    #[test_case(
        r#"
        [[files]]
            path =  "/etc/tedge/tedge.toml"
        [[files]]
            path =  "/etc/tedge/mosquitto-conf/c8y-bridge.conf"
        [[files]]
            path =  "/etc/tedge/mosquitto-conf/tedge-mosquitto.conf"
        [[files]]
          path =  "/etc/mosquitto/mosquitto.conf"
        "#,
        PluginConfig {
            files: HashSet::from([
                FileEntry::new("/etc/tedge/tedge.toml".to_string()),
                FileEntry::new("/etc/tedge/mosquitto-conf/c8y-bridge.conf".to_string(),),
                FileEntry::new("/etc/tedge/mosquitto-conf/tedge-mosquitto.conf".to_string(),),
                FileEntry::new("/etc/mosquitto/mosquitto.conf".to_string())
            ])
        }; "standard case"
    )]
    #[test_case(
        r#"
        [[files]]
            path =  "/etc/tedge/tedge.toml"
        [[files]]
            path =  "/etc/tedge/mosquitto-conf/c8y-bridge.conf"
        [[files]]
            path =  "/etc/tedge/tedge.toml"
        [[files]]
            path =  "/etc/tedge/mosquitto-conf/c8y-bridge.conf"
        "#,
        PluginConfig {
            files: HashSet::from([
            FileEntry::new("/etc/tedge/tedge.toml".to_string()),
            FileEntry::new("/etc/tedge/mosquitto-conf/c8y-bridge.conf".to_string(),),
            ])
        }; "file path duplication"
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
            path =  "/etc/tedge/tedge.toml"
        [[files]]
            path =  "/etc/tedge/mosquitto-conf/c8y-bridge.conf"
        [[files]]
            path =  "/etc/tedge/tedge.toml"
        [[files]]
            path =  "/etc/tedge/mosquitto-conf/c8y-bridge.conf"
        [[unsupported_key]]
        "#,
        PluginConfig {
            files: HashSet::new()
        }
        ;"unexpected field"
    )]
    fn read_plugin_config_file(file_content: &str, raw_config: PluginConfig) -> anyhow::Result<()> {
        let (_dir, config_root_path) = create_temp_plugin_config(file_content)?;
        let tmp_path_to_plugin_config = config_root_path.join(PLUGIN_CONFIG_FILE);
        let tmp_path_to_plugin_config_str =
            tmp_path_to_plugin_config.as_path().display().to_string();

        let config = PluginConfig::new(tmp_path_to_plugin_config.clone());

        // The expected output should contain /tmp/<random>/c8y_configuration_plugin.toml
        let expected_config = raw_config.add_file(tmp_path_to_plugin_config_str);

        assert_eq!(config, expected_config);

        Ok(())
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
    fn add_file_to_plugin_config() {
        let config = PluginConfig::default().add_file("/test/path/file".into());
        assert_eq!(
            config.files,
            HashSet::from([FileEntry::new("/test/path/file".to_string())])
        )
    }

    #[test]
    fn add_file_to_plugin_config_with_duplication() {
        let config = PluginConfig::default()
            .add_file("/test/path/file".into())
            .add_file("/test/path/file".into());
        assert_eq!(
            config.files,
            HashSet::from([FileEntry::new("/test/path/file".to_string())])
        )
    }

    #[test]
    fn get_smartrest_single_type() {
        let plugin_config = PluginConfig {
            files: HashSet::from([FileEntry::new("typeA".to_string())]),
        };
        let output = plugin_config.to_smartrest_payload();
        assert_eq!(output, "119,typeA");
    }

    #[test]
    fn get_smartrest_multiple_types() {
        let plugin_config = PluginConfig {
            files: HashSet::from([
                FileEntry::new("typeA".to_string()),
                FileEntry::new("typeB".to_string()),
                FileEntry::new("typeC".to_string()),
            ]),
        };
        let output = plugin_config.to_smartrest_payload();
        assert_eq!(output, ("119,typeA,typeB,typeC"));
        dbg!(output);
    }
}
