use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

pub const PLUGIN_CONFIG_FILE: &str = "c8y-configuration-plugin.toml";

#[derive(Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PluginConfig {
    pub files: Vec<String>,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self { files: vec![] }
    }
}

impl PluginConfig {
    pub fn new(config_root: PathBuf) -> Self {
        let config_path = config_root.join(PLUGIN_CONFIG_FILE);
        let config_path_str = config_path.to_str().unwrap_or(PLUGIN_CONFIG_FILE);
        Self::read_config(config_path.clone()).add_file(config_path_str.into())
    }

    fn read_config(path: PathBuf) -> Self {
        let path_str = path.to_str().unwrap_or(PLUGIN_CONFIG_FILE);
        info!("Reading the config file from {}", path_str);
        match fs::read_to_string(path.clone()) {
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
        let () = files.push(file);
        Self { files }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    use test_case::test_case;

    #[test]
    fn deserialize_plugin_config() {
        let config: PluginConfig = toml::from_str(
            r#"
             files = [
                '/etc/tedge/tedge.toml',
                '/etc/tedge/mosquitto-conf/c8y-bridge.conf',
                '/etc/tedge/mosquitto-conf/tedge-mosquitto.conf',
                '/etc/mosquitto/mosquitto.conf'
            ]"#,
        )
        .unwrap();

        assert_eq!(
            config.files,
            vec![
                "/etc/tedge/tedge.toml".to_string(),
                "/etc/tedge/mosquitto-conf/c8y-bridge.conf".to_string(),
                "/etc/tedge/mosquitto-conf/tedge-mosquitto.conf".to_string(),
                "/etc/mosquitto/mosquitto.conf".to_string(),
            ]
        );
    }

    #[test_case(
        r#"files = [
            '/etc/tedge/tedge.toml',
            '/etc/tedge/mosquitto-conf/c8y-bridge.conf',
            '/etc/tedge/mosquitto-conf/tedge-mosquitto.conf',
            '/etc/mosquitto/mosquitto.conf'
        ]"#,
        PluginConfig {
            files: vec![
                "/etc/tedge/tedge.toml".to_string(),
                "/etc/tedge/mosquitto-conf/c8y-bridge.conf".to_string(),
                "/etc/tedge/mosquitto-conf/tedge-mosquitto.conf".to_string(),
                "/etc/mosquitto/mosquitto.conf".to_string(),
            ]
        }
    )]
    #[test_case(
        r#"files = []"#,
        PluginConfig {
            files: vec![]
        }
        ;"empty case"
    )]
    #[test_case(
        r#"test"#,
        PluginConfig {
            files: vec![]
        }
        ;"not toml"
    )]
    #[test_case(
        r#"files = [
            '/etc/tedge/tedge.toml',
            '/etc/tedge/mosquitto-conf/c8y-bridge.conf',
            '/etc/tedge/mosquitto-conf/tedge-mosquitto.conf',
            '/etc/mosquitto/mosquitto.conf'
        ]
        unsupported_key = false
        "#,
        PluginConfig {
            files: vec![]
        }
        ;"unexpected field"
    )]
    fn read_plugin_config_file(file_content: &str, raw_config: PluginConfig) -> anyhow::Result<()> {
        let (_dir, config_root_path) = create_temp_plugin_config(file_content)?;
        let tmp_path_to_plugin_config = config_root_path.join(PLUGIN_CONFIG_FILE);
        let tmp_path_to_plugin_config_str = tmp_path_to_plugin_config
            .to_str()
            .unwrap_or(PLUGIN_CONFIG_FILE);

        let config = PluginConfig::new(config_root_path.clone());

        // The expected output should contain /tmp/<random>/c8y_configuration_plugin.toml
        let expected_config = raw_config.add_file(tmp_path_to_plugin_config_str.into());

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
        assert_eq!(config.files, vec!["/test/path/file".to_string()])
    }
}
