use crate::system_services::SystemServiceError;
use camino::Utf8Path;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::time::Duration;

pub const SERVICE_CONFIG_FILE: &str = "system.toml";
const REBOOT_COMMAND: &[&str] = &["init", "6"];

#[derive(Deserialize, Debug, Default, Eq, PartialEq)]
pub struct SystemConfig {
    #[serde(default)]
    pub init: InitConfig,
    #[serde(default)]
    pub log: HashMap<String, String>,
    #[serde(default)]
    pub system: SystemSpecificCommands,
}

#[derive(Deserialize, Debug, Eq, PartialEq)]
#[serde(from = "InitConfigToml")]
pub struct InitConfig {
    pub name: String,
    pub is_available: Vec<String>,
    pub restart: Vec<String>,
    pub stop: Vec<String>,
    pub start: Vec<String>,
    pub enable: Vec<String>,
    pub disable: Vec<String>,
    pub is_active: Vec<String>,
}

#[derive(Deserialize, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct InitConfigToml {
    name: String,
    is_available: Vec<String>,
    restart: Vec<String>,
    stop: Vec<String>,
    start: Option<Vec<String>>,
    enable: Vec<String>,
    disable: Vec<String>,
    is_active: Vec<String>,
}

impl From<InitConfigToml> for InitConfig {
    fn from(value: InitConfigToml) -> Self {
        Self {
            name: value.name,
            is_available: value.is_available,
            start: value.start.unwrap_or(value.restart.clone()),
            restart: value.restart,
            stop: value.stop,
            enable: value.enable,
            disable: value.disable,
            is_active: value.is_active,
        }
    }
}

#[derive(Deserialize, Debug, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SystemSpecificCommands {
    pub reboot: Vec<String>,
    #[serde(default = "SystemSpecificCommands::default_reboot_timeout_seconds")]
    pub reboot_timeout_seconds: u64,
}

impl SystemSpecificCommands {
    pub fn default_reboot_timeout_seconds() -> u64 {
        // The linux shutdown command only supports triggering the shutdown immediately
        // or in minutes, a delay in seconds is not supported. Using a shell script to delay
        // the call to shutdown is generally not very reliable.
        // Choose a sensible default that won't timeout if 'shutdown -r' is used
        // (with some buffer), e.g. 2 x default interval (60 seconds)
        120
    }

    pub fn reboot_timeout(&self) -> Duration {
        Duration::from_secs(self.reboot_timeout_seconds)
    }
}

impl Default for SystemSpecificCommands {
    fn default() -> Self {
        Self {
            reboot: REBOOT_COMMAND
                .iter()
                .map(|value| String::from(*value))
                .collect::<Vec<String>>(),
            reboot_timeout_seconds: SystemSpecificCommands::default_reboot_timeout_seconds(),
        }
    }
}

impl Default for InitConfig {
    fn default() -> Self {
        Self {
            name: "systemd".to_string(),
            is_available: vec!["/bin/systemctl".into(), "--version".into()],
            restart: vec!["/bin/systemctl".into(), "restart".into(), "{}".into()],
            stop: vec!["/bin/systemctl".into(), "stop".into(), "{}".into()],
            start: vec!["/bin/systemctl".into(), "start".into(), "{}".into()],
            enable: vec!["/bin/systemctl".into(), "enable".into(), "{}".into()],
            disable: vec!["/bin/systemctl".into(), "disable".into(), "{}".into()],
            is_active: vec!["/bin/systemctl".into(), "is-active".into(), "{}".into()],
        }
    }
}

impl SystemConfig {
    pub fn try_new(config_root: &Utf8Path) -> Result<Self, SystemServiceError> {
        let config_path = config_root.join(SERVICE_CONFIG_FILE);

        match fs::read_to_string(config_path.clone()) {
            Ok(contents) => {
                let config: SystemConfig = toml::from_str(contents.as_str()).map_err(|e| {
                    SystemServiceError::SystemConfigInvalidToml {
                        path: config_path,
                        reason: e.to_string(),
                    }
                })?;
                Ok(config)
            }
            Err(_) => {
                println!("The system config file '{}' doesn't exist. Use '/bin/systemctl' as a service manager.\n", config_path);
                Ok(Self::default())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn deserialize_system_config() {
        let config: SystemConfig = toml::from_str(
            r#"
            [init]
            name = "systemd"
            is_available = ["/bin/systemctl", "--version"]
            restart = ["/bin/systemctl", "restart", "{}"]
            stop =  ["/bin/systemctl", "stop", "{}"]
            start =  ["/bin/systemctl", "start", "{}"]
            enable =  ["/bin/systemctl", "enable", "{}"]
            disable =  ["/bin/systemctl", "disable", "{}"]
            is_active = ["/bin/systemctl", "is-active", "{}"]

            [system]
            reboot = ["init", "6"]
            
            [log]
            tedge_mapper = "Debug"
            tedge_agent = "Info"
            tedge_watchdog = "Warn"
        "#,
        )
        .unwrap();

        assert_eq!(config.init.name, "systemd");
        assert_eq!(
            config.init.is_available,
            vec!["/bin/systemctl", "--version"]
        );
        assert_eq!(config.init.restart, vec!["/bin/systemctl", "restart", "{}"]);
        assert_eq!(config.init.stop, vec!["/bin/systemctl", "stop", "{}"]);
        assert_eq!(config.init.start, vec!["/bin/systemctl", "start", "{}"]);
        assert_eq!(config.init.enable, vec!["/bin/systemctl", "enable", "{}"]);
        assert_eq!(config.init.disable, vec!["/bin/systemctl", "disable", "{}"]);
        assert_eq!(
            config.init.is_active,
            vec!["/bin/systemctl", "is-active", "{}"]
        );
        assert_eq!(
            config.system.reboot,
            Vec::from([String::from("init"), String::from("6")])
        );
        assert_eq!(config.log.get("tedge_mapper").unwrap(), "Debug");
        assert_eq!(config.log.get("tedge_agent").unwrap(), "Info");
        assert_eq!(config.log.get("tedge_watchdog").unwrap(), "Warn");
    }

    #[test]
    fn deserialize_init_config_without_start_field() {
        let config: SystemConfig = toml::from_str(
            r#"
            [init]
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

        assert_eq!(config.init.start, vec!["/bin/systemctl", "restart", "{}"]);
    }

    #[test]
    fn read_system_log_config_file() -> anyhow::Result<()> {
        let toml_conf = r#"            
        [log]
        tedge_mapper = "Debug"
        tedge_agent = "Info"
        tedge_watchdog = "Warn"
    "#;
        let expected_config: SystemConfig = toml::from_str(toml_conf)?;
        let (_dir, config_root_path) = create_temp_system_config(toml_conf)?;
        let config = SystemConfig::try_new(&config_root_path).unwrap();
        assert_eq!(config, expected_config);

        Ok(())
    }

    #[test]
    fn read_system_config_file() -> anyhow::Result<()> {
        let toml_conf = r#"
            [system]
            reboot = ["init", "6"]
        "#;
        let expected_config: SystemConfig = toml::from_str(toml_conf)?;

        let (_dir, config_root_path) = create_temp_system_config(toml_conf)?;
        let config = SystemConfig::try_new(&config_root_path).unwrap();

        assert_eq!(config, expected_config);

        Ok(())
    }

    // Need to return TempDir, otherwise the dir will be deleted when this function ends.
    fn create_temp_system_config(content: &str) -> std::io::Result<(TempDir, Utf8PathBuf)> {
        let temp_dir = TempDir::new()?;
        let config_root = Utf8Path::from_path(temp_dir.path()).unwrap().to_owned();
        let config_file_path = config_root.join(SERVICE_CONFIG_FILE);
        let mut file = std::fs::File::create(config_file_path.as_path())?;
        file.write_all(content.as_bytes())?;
        Ok((temp_dir, config_root))
    }
}
