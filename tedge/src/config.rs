use crate::command::Command;
use crate::config::ConfigError::{HomeDirectoryNotFound, InvalidCharacterInHomeDirectoryPath};
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, read_to_string};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use structopt::StructOpt;
use tempfile::NamedTempFile;

const TEDGE_HOME_DIR: &str = ".tedge";
const TEDGE_CONFIG_FILE: &str = "tedge.toml";
const DEVICE_CERT_DIR: &str = "certificate";
const DEVICE_KEY_FILE: &str = "tedge-private-key.pem";
const DEVICE_CERT_FILE: &str = "tedge-certificate.pem";

const DEVICE_ID: &str = "device-id";
const DEVICE_CERT_PATH: &str = "device-cert-path";
const DEVICE_KEY_PATH: &str = "device-key-path";

const C8Y_URL: &str = "c8y-url";
const C8Y_ROOT_CERT_PATH: &str = "c8y-root-cert-path";

#[derive(StructOpt, Debug)]
pub enum ConfigCmd {
    /// Set or update the provided configuration key with the given value
    Set {
        /// Configuration key.
        key: String,

        /// Configuration value.
        value: String,
    },

    /// Unset the provided configuration key
    Unset {
        /// Configuration key.
        key: String,
    },

    /// Get the value of the provided configuration key
    Get {
        /// Configuration key.
        key: String,
    },
}

impl Command for ConfigCmd {
    fn to_string(&self) -> String {
        match self {
            ConfigCmd::Set { key, value } => {
                format!("set the configuration key: {} with value: {}.", key, value)
            }
            ConfigCmd::Get { key } => format!("get the configuration value for key: {}", key),
            ConfigCmd::Unset { key } => format!("unset the configuration value for key: {}", key),
        }
    }

    fn run(&self, _verbose: u8) -> Result<(), anyhow::Error> {
        let mut config = TEdgeConfig::from_default_config()?;
        let mut config_updated = false;

        match self {
            ConfigCmd::Get { key } => {
                let value =
                    config
                        .get_config_value(key.as_str())?
                        .ok_or(ConfigError::ConfigNotSet {
                            key: key.as_str().to_string(),
                        })?;
                println!("{}", value)
            }
            ConfigCmd::Set { key, value } => {
                config.set_config_value(key.as_str(), value.to_string())?;
                config_updated = true;
            }
            ConfigCmd::Unset { key } => {
                config.unset_config_value(key.as_str())?;
                config_updated = true;
            }
        }

        if config_updated {
            config.write_to_default_config()?;
        }
        Ok(())
    }
}

#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct TEdgeConfig {
    #[serde(default)]
    pub device: DeviceConfig,
    #[serde(default)]
    pub c8y: CumulocityConfig,
}

impl TEdgeConfig {
    fn with_defaults(self) -> Result<Self, ConfigError> {
        let device_config = self.device.with_defaults()?;

        Ok(TEdgeConfig {
            device: device_config,
            ..self
        })
    }
}

#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct DeviceConfig {
    pub id: Option<String>,
    pub key_path: Option<String>,
    pub cert_path: Option<String>,
}

impl DeviceConfig {
    pub fn default_cert_path() -> Result<String, ConfigError> {
        Self::path_in_cert_directory(DEVICE_CERT_FILE)
    }

    pub fn default_key_path() -> Result<String, ConfigError> {
        Self::path_in_cert_directory(DEVICE_KEY_FILE)
    }

    fn path_in_cert_directory(file_name: &str) -> Result<String, ConfigError> {
        home_dir()?
            .join(DEVICE_CERT_DIR)
            .join(file_name)
            .to_str()
            .map(|s| s.into())
            .ok_or(InvalidCharacterInHomeDirectoryPath)
    }

    fn with_defaults(self) -> Result<Self, ConfigError> {
        let key_path = match self.key_path {
            None => Self::default_key_path()?,
            Some(val) => val,
        };

        let cert_path = match self.cert_path {
            None => Self::default_cert_path()?,
            Some(val) => val,
        };

        Ok(DeviceConfig {
            key_path: Some(key_path),
            cert_path: Some(cert_path),
            ..self
        })
    }
}

#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CumulocityConfig {
    url: Option<String>,
    root_cert_path: Option<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("TOML parse error")]
    TOMLParseError(#[from] toml::de::Error),

    #[error("TOML serialization error")]
    InvalidTOMLError(#[from] toml::ser::Error),

    #[error("I/O error")]
    IOError(#[from] std::io::Error),

    #[error("Home directory not found")]
    HomeDirectoryNotFound,

    #[error("Invalid characters found in home directory path")]
    InvalidCharacterInHomeDirectoryPath,

    #[error("The provided config key: {key} is not a valid Thin Edge configuration key")]
    InvalidConfigKey { key: String },

    #[error("The provided config key: {key} is not set")]
    ConfigNotSet { key: String },
}

pub fn home_dir() -> Result<PathBuf, ConfigError> {
    // The usage of this deprecated method is temporary as this whole function will be replaced with the util function being added in CIT-137.
    #![allow(deprecated)]
    std::env::home_dir().ok_or(HomeDirectoryNotFound)
}

pub fn tedge_config_path() -> Result<PathBuf, ConfigError> {
    Ok(home_dir()?.join(TEDGE_HOME_DIR).join(TEDGE_CONFIG_FILE))
}

impl TEdgeConfig {
    ///Parse the configuration file at `$HOME/.tedge/tedge.toml` and create a `TEdgeConfig` out of it
    pub fn from_default_config() -> Result<TEdgeConfig, ConfigError> {
        Self::from_custom_config(tedge_config_path()?.as_path())
    }

    ///Parse the configuration file at the provided `path` and create a `TEdgeConfig` out of it
    fn from_custom_config(path: &Path) -> Result<TEdgeConfig, ConfigError> {
        match read_to_string(path) {
            Ok(content) => {
                let mut tedge_config = toml::from_str::<TEdgeConfig>(content.as_str())?;
                tedge_config.device = tedge_config.device.with_defaults()?;
                Ok(tedge_config)
            }
            Err(err) => match err.kind() {
                ErrorKind::NotFound => {
                    let default: TEdgeConfig = Default::default();
                    Ok(default.with_defaults()?)
                }
                _ => return Err(ConfigError::IOError(err)),
            },
        }
    }

    //Persists this `TEdgeConfig` to $HOME/.tedge/tedge.toml
    pub fn write_to_default_config(&self) -> Result<(), ConfigError> {
        self.write_to_custom_config(tedge_config_path()?.as_path())
    }

    //Persists this `TEdgeConfig` to the `path` provided
    fn write_to_custom_config(&self, path: &Path) -> Result<(), ConfigError> {
        let toml = toml::to_string_pretty(&self)?;
        let mut file = NamedTempFile::new()?;
        file.write_all(toml.as_bytes())?;
        if !path.exists() {
            create_dir_all(path.parent().unwrap())?;
        }
        match file.persist(path) {
            Ok(_) => Ok(()),
            Err(err) => Err(err.error)?,
        }
    }

    pub fn get_config_value(&self, key: &str) -> Result<Option<String>, ConfigError> {
        match key {
            DEVICE_ID => Ok(self.device.id.clone()),
            DEVICE_KEY_PATH => Ok(self.device.key_path.clone()),
            DEVICE_CERT_PATH => Ok(self.device.cert_path.clone()),
            C8Y_URL => Ok(self.c8y.url.clone()),
            C8Y_ROOT_CERT_PATH => Ok(self.c8y.root_cert_path.clone()),
            _ => Err(ConfigError::InvalidConfigKey { key: key.into() }),
        }
    }

    pub fn set_config_value(&mut self, key: &str, value: String) -> Result<(), ConfigError> {
        self.update_config_value(key, Some(value))
    }

    pub fn unset_config_value(&mut self, key: &str) -> Result<(), ConfigError> {
        self.update_config_value(key, None)
    }

    fn update_config_value(&mut self, key: &str, value: Option<String>) -> Result<(), ConfigError> {
        match key {
            DEVICE_ID => self.device.id = value,
            DEVICE_KEY_PATH => self.device.key_path = value,
            DEVICE_CERT_PATH => self.device.cert_path = value,
            C8Y_URL => self.c8y.url = value,
            C8Y_ROOT_CERT_PATH => self.c8y.root_cert_path = value,
            _ => return Err(ConfigError::InvalidConfigKey { key: key.into() }),
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn test_parse_config_with_all_values() {
        let toml_conf = r#"
[device]
id = "ABCD1234"
key_path = "/path/to/key"
cert_path = "/path/to/cert"

[c8y]
url = "your-tenant.cumulocity.com"
root_cert_path = "/path/to/root/cert"
"#;

        let config_file = temp_file_with_content(toml_conf);
        let config = TEdgeConfig::from_custom_config(config_file.path()).unwrap();

        assert_eq!(config.device.id.unwrap(), "ABCD1234");
        assert_eq!(config.device.key_path.unwrap(), "/path/to/key");
        assert_eq!(config.device.cert_path.unwrap(), "/path/to/cert");

        assert_eq!(config.c8y.url.unwrap(), "your-tenant.cumulocity.com");
        assert_eq!(config.c8y.root_cert_path.unwrap(), "/path/to/root/cert");
    }

    #[test]
    fn test_write_to_custom_config() {
        let toml_conf = r#"
[device]
id = "ABCD1234"
key_path = "/path/to/key"
cert_path = "/path/to/cert"

[c8y]
url = "your-tenant.cumulocity.com"
root_cert_path = "/path/to/root/cert"
"#;

        let config_file = temp_file_with_content(toml_conf);
        let mut config = TEdgeConfig::from_custom_config(config_file.path()).unwrap();
        assert_eq!(config.device.id.as_ref().unwrap(), "ABCD1234");
        assert_eq!(config.device.key_path.as_ref().unwrap(), "/path/to/key");
        assert_eq!(config.device.cert_path.as_ref().unwrap(), "/path/to/cert");

        assert_eq!(
            config.c8y.url.as_ref().unwrap(),
            "your-tenant.cumulocity.com"
        );
        assert_eq!(
            config.c8y.root_cert_path.as_ref().unwrap(),
            "/path/to/root/cert"
        );

        let updated_device_id = "XYZ1234";
        let updated_tenant_url = "other-tenant.cumulocity.com";

        config.device.id = Some(updated_device_id.to_string());
        config.c8y.url = Some(updated_tenant_url.to_string());
        config.c8y.root_cert_path = None;

        config.write_to_custom_config(config_file.path()).unwrap();
        let config = TEdgeConfig::from_custom_config(config_file.path()).unwrap();

        assert_eq!(config.device.id.as_ref().unwrap(), updated_device_id);
        assert_eq!(config.device.key_path.as_ref().unwrap(), "/path/to/key");
        assert_eq!(config.device.cert_path.as_ref().unwrap(), "/path/to/cert");

        assert_eq!(config.c8y.url.as_ref().unwrap(), updated_tenant_url);
        assert!(config.c8y.root_cert_path.is_none());
    }

    #[test]
    fn test_parse_config_missing_c8y_configuration() {
        let toml_conf = r#"
[device]
id = "ABCD1234"
"#;

        let config_file = temp_file_with_content(toml_conf);
        let config = TEdgeConfig::from_custom_config(config_file.path()).unwrap();

        assert_eq!(config.device.id.as_ref().unwrap(), "ABCD1234");
        assert_eq!(
            config.device.cert_path.clone().unwrap(),
            DeviceConfig::default_cert_path().unwrap()
        );
        assert_eq!(
            config.device.key_path.clone().unwrap(),
            DeviceConfig::default_key_path().unwrap()
        );

        assert!(config.c8y.url.is_none());
        assert!(config.c8y.root_cert_path.is_none());
    }

    #[test]
    fn test_parse_config_missing_device_configuration() {
        let toml_conf = r#"
[c8y]
url = "your-tenant.cumulocity.com"
"#;

        let config_file = temp_file_with_content(toml_conf);
        let config = TEdgeConfig::from_custom_config(config_file.path()).unwrap();

        assert_eq!(config.c8y.url.unwrap(), "your-tenant.cumulocity.com");

        assert!(config.device.id.is_none());
        assert_eq!(
            config.device.cert_path.clone().unwrap(),
            DeviceConfig::default_cert_path().unwrap()
        );
        assert_eq!(
            config.device.key_path.clone().unwrap(),
            DeviceConfig::default_key_path().unwrap()
        );
    }

    #[test]
    fn test_parse_config_empty_file() {
        let config_file = NamedTempFile::new().unwrap();
        let config = TEdgeConfig::from_custom_config(config_file.path()).unwrap();

        assert!(config.device.id.is_none());
        assert_eq!(
            config.device.cert_path.clone().unwrap(),
            DeviceConfig::default_cert_path().unwrap()
        );
        assert_eq!(
            config.device.key_path.clone().unwrap(),
            DeviceConfig::default_key_path().unwrap()
        );

        assert!(config.c8y.url.is_none());
        assert!(config.c8y.root_cert_path.is_none());
    }

    #[test]
    fn test_parse_config_no_config_file() {
        let config = TEdgeConfig::from_custom_config(Path::new("/non/existent/path")).unwrap();

        assert!(config.device.id.is_none());
        assert!(config.c8y.url.is_none());
    }

    #[test]
    fn test_parse_unsupported_keys() {
        let toml_conf = r#"
hey="tedge"
[c8y]
hello="tedge"
"#;

        let config_file = temp_file_with_content(toml_conf);
        let result = TEdgeConfig::from_custom_config(config_file.path());
        assert_matches!(
            result.unwrap_err(),
            ConfigError::TOMLParseError(_),
            "Expected the parsing to fail with TOMLParseError"
        );
    }

    #[test]
    fn test_parse_invalid_toml_file() {
        let toml_conf = r#"
        <abcde>
        "#;

        let config_file = temp_file_with_content(toml_conf);
        let result = TEdgeConfig::from_custom_config(config_file.path());
        assert_matches!(
            result.unwrap_err(),
            ConfigError::TOMLParseError(_),
            "Expected the parsing to fail with TOMLParseError"
        );
    }

    #[test]
    fn test_set_config_key_invalid_key() {
        let mut config = TEdgeConfig::from_default_config().unwrap();
        assert_matches!(
            config.set_config_value("invalid-key", "dummy-value".into()).unwrap_err(),
            ConfigError::InvalidConfigKey { .. }
        );
    }

    #[test]
    fn test_get_config_key_invalid_key() {
        let config = TEdgeConfig::from_default_config().unwrap();
        assert_matches!(
            config.get_config_value("invalid-key").unwrap_err(),
            ConfigError::InvalidConfigKey { .. }
        );
    }

    #[test]
    fn test_unset_config_key_invalid_key() {
        let mut config = TEdgeConfig::from_default_config().unwrap();
        assert_matches!(
            config.unset_config_value("invalid-key").unwrap_err(),
            ConfigError::InvalidConfigKey { .. }
        );
    }

    #[test]
    fn test_crud_config_value() {
        let toml_conf = r#"
[device]
id = "ABCD1234"
key_path = "/path/to/key"
cert_path = "/path/to/cert"

[c8y]
url = "your-tenant.cumulocity.com"
root_cert_path = "/path/to/root/cert"
"#;

        let config_file = temp_file_with_content(toml_conf);
        let mut config = TEdgeConfig::from_custom_config(config_file.path()).unwrap();

        let original_device_id = "ABCD1234".to_string();
        let original_device_key_path = "/path/to/key".to_string();
        let original_device_cert_path = "/path/to/cert".to_string();
        assert_eq!(
            config.get_config_value(DEVICE_ID).unwrap().unwrap(),
            original_device_id
        );
        assert_eq!(
            config.get_config_value(DEVICE_KEY_PATH).unwrap().unwrap(),
            original_device_key_path
        );
        assert_eq!(
            config.get_config_value(DEVICE_CERT_PATH).unwrap().unwrap(),
            original_device_cert_path
        );

        let original_c8y_url = "your-tenant.cumulocity.com".to_string();
        let original_c8y_root_cert_path = "/path/to/root/cert".to_string();
        assert_eq!(
            config.get_config_value(C8Y_URL).unwrap().unwrap(),
            original_c8y_url
        );
        assert_eq!(
            config
                .get_config_value(C8Y_ROOT_CERT_PATH)
                .unwrap()
                .unwrap(),
            original_c8y_root_cert_path
        );

        let updated_device_id = "XYZ1234".to_string();
        let updated_c8y_url = "other-tenant.cumulocity.com".to_string();

        config
            .set_config_value(DEVICE_ID, updated_device_id.clone())
            .unwrap();
        config
            .set_config_value(C8Y_URL, updated_c8y_url.clone())
            .unwrap();
        config.unset_config_value(C8Y_ROOT_CERT_PATH).unwrap();

        assert_eq!(
            config.get_config_value(DEVICE_ID).unwrap().unwrap(),
            updated_device_id
        );
        assert_eq!(
            config.get_config_value(DEVICE_KEY_PATH).unwrap().unwrap(),
            original_device_key_path
        );
        assert_eq!(
            config.get_config_value(DEVICE_CERT_PATH).unwrap().unwrap(),
            original_device_cert_path
        );

        assert_eq!(
            config.get_config_value(C8Y_URL).unwrap().unwrap(),
            updated_c8y_url
        );
        assert!(config
            .get_config_value(C8Y_ROOT_CERT_PATH)
            .unwrap()
            .is_none());
    }

    fn temp_file_with_content(content: &str) -> NamedTempFile {
        let file = NamedTempFile::new().unwrap();
        file.as_file().write_all(content.as_bytes()).unwrap();
        file
    }
}
