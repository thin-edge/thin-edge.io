use crate::system_services::SystemConfigError::{ConfigFileNotFound, InvalidSyntax};
use crate::system_services::*;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

const SERVICE_CONFIG_FILE: &str = "system.toml";

#[derive(Deserialize, Debug, PartialEq)]
pub struct SystemConfig {
    pub name: String,
    pub is_available: Vec<String>,
    pub restart: Vec<String>,
    pub stop: Vec<String>,
    pub enable: Vec<String>,
    pub disable: Vec<String>,
    pub is_active: Vec<String>,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            name: "systemd".to_string(),
            is_available: vec!["/bin/systemctl".into(), "--version".into()],
            restart: vec!["/bin/systemctl".into(), "restart".into(), "{}".into()],
            stop: vec!["/bin/systemctl".into(), "stop".into(), "{}".into()],
            enable: vec!["/bin/systemctl".into(), "enable".into(), "{}".into()],
            disable: vec!["/bin/systemctl".into(), "disable".into(), "{}".into()],
            is_active: vec!["/bin/systemctl".into(), "is-active".into(), "{}".into()]
        }
    }
}

impl SystemConfig {
    pub fn new(config_root: PathBuf) -> Self {
        match Self::try_new(config_root) {
            Ok(config) => config,
            Err(err) => {
                eprintln!("{:?}", err);
                Self::default()
            }
        }
    }

    pub fn try_new(config_root: PathBuf) -> Result<Self, SystemConfigError> {
        let config_path = config_root.join(SERVICE_CONFIG_FILE);

        let contents =
            fs::read_to_string(config_path.clone()).map_err(|_| ConfigFileNotFound(config_path))?;

        let config: SystemConfig =
            toml::from_str(contents.as_str()).map_err(|e| InvalidSyntax {
                reason: format!("{}", e),
            })?;

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn deserialize_system_config() {
        let config: SystemConfig = toml::from_str(
            r#"
            name = "systemd"
            is_available = ["/bin/systemctl", "--version"]
            restart = ["/bin/systemctl", "restart", "{}"]
            stop =  ["/bin/systemctl", "stop", "{}"]
            enable =  ["/bin/systemctl", "enable", "{}"]
            disable =  ["/bin/systemctl", "disable", "{}"]
            is_active = ["/bin/systemctl", "is-active", "{}"]
        "#,
        )
        .unwrap();

        assert_eq!(config.name, "systemd");
        assert_eq!(config.is_available, vec!["/bin/systemctl", "--version"]);
        assert_eq!(config.restart, vec!["/bin/systemctl", "restart", "{}"]);
        assert_eq!(config.stop, vec!["/bin/systemctl", "stop", "{}"]);
        assert_eq!(config.enable, vec!["/bin/systemctl", "enable", "{}"]);
        assert_eq!(config.disable, vec!["/bin/systemctl", "disable", "{}"]);
        assert_eq!(config.is_active, vec!["/bin/systemctl", "is-active", "{}"]);
    }

    #[test]
    fn read_system_config_file() -> anyhow::Result<()> {
        let toml_conf = r#"
            name = "systemd"
            is_available = ["/bin/systemctl", "--version"]
            restart = ["/bin/systemctl", "restart", "{}"]
            stop =  ["/bin/systemctl", "stop", "{}"]
            enable =  ["/bin/systemctl", "enable", "{}"]
            disable =  ["/bin/systemctl", "disable", "{}"]
            is_active = ["/bin/systemctl", "is-active", "{}"]
        "#;
        let expected_config: SystemConfig = toml::from_str(toml_conf)?;

        let (_dir, config_root_path) = create_temp_tedge_config(toml_conf)?;
        let config = SystemConfig::new(config_root_path);

        assert_eq!(config, expected_config);

        Ok(())
    }

    // Need to return TempDir, otherwise the dir will be deleted when this function ends.
    fn create_temp_tedge_config(content: &str) -> std::io::Result<(TempDir, PathBuf)> {
        let temp_dir = TempDir::new()?;
        let config_root = temp_dir.path().to_path_buf();
        let config_file_path = config_root.join(SERVICE_CONFIG_FILE);
        let mut file = std::fs::File::create(config_file_path.as_path())?;
        file.write_all(content.as_bytes())?;
        Ok((temp_dir, config_root))
    }
}
