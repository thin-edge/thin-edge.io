use crate::command::Command;
use crate::config::ConfigError::{HomeDirectoryNotFound, InvalidCharacterInHomeDirectoryPath};
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, read_to_string};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use structopt::StructOpt;
use tempfile::NamedTempFile;

pub const TEDGE_HOME_DIR: &str = ".tedge";
const TEDGE_CONFIG_FILE: &str = "tedge.toml";
const DEVICE_CERT_DIR: &str = "certificate";
const DEVICE_KEY_FILE: &str = "tedge-private-key.pem";
const DEVICE_CERT_FILE: &str = "tedge-certificate.pem";

pub const DEVICE_ID: &str = "device-id";
pub const DEVICE_CERT_PATH: &str = "device-cert-path";
pub const DEVICE_KEY_PATH: &str = "device-key-path";

pub const C8Y_CONNECT: &str = "c8y-connect";
pub const C8Y_URL: &str = "c8y-url";
pub const C8Y_ROOT_CERT_PATH: &str = "c8y-root-cert-path";

/// Wrapper type for Configuration keys.
#[derive(Debug)]
pub struct ConfigKey(pub String);

impl ConfigKey {
    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl std::str::FromStr for ConfigKey {
    type Err = String;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        if TEdgeConfig::is_valid_key(key) {
            Ok(ConfigKey(key.into()))
        } else {
            Err(format!(
                "Invalid key `{}'. Valid keys are: [{}].",
                key,
                TEdgeConfig::valid_keys().join(", ")
            ))
        }
    }
}

#[derive(StructOpt, Debug)]
pub enum ConfigCmd {
    /// Set or update the provided configuration key with the given value
    Set {
        /// Configuration key.
        #[structopt(help = TEdgeConfig::valid_keys_help_message())]
        key: ConfigKey,

        /// Configuration value.
        value: String,
    },

    /// Unset the provided configuration key
    Unset {
        /// Configuration key.
        #[structopt(help = TEdgeConfig::valid_keys_help_message())]
        key: ConfigKey,
    },

    /// Get the value of the provided configuration key
    Get {
        /// Configuration key.
        #[structopt(help = TEdgeConfig::valid_keys_help_message())]
        key: ConfigKey,
    },

    /// Print the configuration keys and their values
    List {
        /// Prints all the configuration keys, even those without a configured value
        #[structopt(long)]
        all: bool,

        /// Prints all keys and descriptions with example values
        #[structopt(long)]
        doc: bool,
    },
}

impl Command for ConfigCmd {
    fn to_string(&self) -> String {
        match self {
            ConfigCmd::Set { key, value } => format!(
                "set the configuration key: {} with value: {}.",
                key.as_str(),
                value
            ),
            ConfigCmd::Get { key } => {
                format!("get the configuration value for key: {}", key.as_str())
            }
            ConfigCmd::Unset { key } => {
                format!("unset the configuration value for key: {}", key.as_str())
            }
            ConfigCmd::List { .. } => String::from("list the configuration keys and values"),
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
            ConfigCmd::List { all, doc } => match doc {
                true => print_config_doc(),
                false => print_config_list(&config, *all)?,
            },
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

///
/// This macro creates accessor functions for a `struct` to set and get the value of (possibly
/// nested) fields given a string as key. It also creates functions to test if a key is valid or
/// not, a utility function returning the list of all valid keys, as well as a function returning a
/// static string that includes all keys. The latter three are useful for StructOpt integration.
///
/// All fields accessed through this macro have to be of type `Option<String>`.
///
/// # Generated functions
///
/// - _get_config_value (get a value given a key)
/// - _set_config_value (set a value)
/// - is_valid_key (test if a key is valid)
/// - valid_keys (list of valid keys)
/// - valid_keys_help_message (create a help message for structopt when `-h` is specified)
/// - get_description_of_key (get a description of a given key)
///
/// # Basic Usage
///
/// ```rust,ignore
/// struct MyType { field_of_my_type: ... };
///
/// config_keys!{
///   MyType {
///     "key1" => field_of_my_type.nested1,
///     "key2" => path_to_field_2,
///     ...
///     "keyn" => path_to_field_n,
///   }
/// }
/// ```
///
/// # Example
///
/// ```
/// struct MyStruct {
///   a: Option<String>,
///   b: Nested,
/// }
///
/// struct Nested {
///     c: Option<String>,
/// }
///
/// config_keys! {
///   MyStruct {
///     "a" => a,
///     "b.c" => b.c,
///   }
/// }
///
/// let my = MyStruct { a: Some("test".into()), b: Nested { c: None } };
/// assert_eq!(my._get_config_value("a").unwrap().unwrap(), "test");
/// assert_eq!(my._get_config_value("b.c").unwrap(), None);
/// assert_eq!(my.is_valid_key("b.c"), true);
/// assert_eq!(my.is_valid_key("c"), false);
/// ```
///
macro_rules! config_keys {
    ($ty:ty { $( $str:literal => ( $( $key:ident ).* , $desc:literal ) )* }) => {
        impl $ty {
            fn _get_config_value<'a>(&'a self, key: &str) -> Result<Option<&'a str>, ConfigError> {
                match key {
                    $( $str => Ok(self . $( $key ).* .as_ref().map(String::as_str)), )*
                    _ => Err(ConfigError::InvalidConfigKey { key: key.into() }),
                }
            }

            fn _set_config_value(&mut self, key: &str, value: Option<String>) -> Result<(), ConfigError> {
                match key {
                    $(
                        $str => {
                            self . $( $key ).* = value;
                            Ok(())
                        }
                     )*
                     _ => Err(ConfigError::InvalidConfigKey { key: key.into() }),
                }
            }

            fn is_valid_key(key: &str) -> bool {
                match key {
                    $( $str => true, )*
                    _ => false,
                }
            }

            fn valid_keys() -> Vec<&'static str> {
                vec![
                    $( $str , )*
                ]
            }

            fn valid_keys_help_message() -> &'static str {
                concat!("[", $( " ", $str ),*, " ]")
            }

            fn get_description_of_key(key: &str) -> &'static str {
                match key {
                    $( $str => $desc, )*
                    _ => "Undefined key",
                }
            }
        }
    }
}

config_keys! {
    TEdgeConfig {
        "device-id"          => (device.id, "Identifier of the device within the fleet. It must be globally unique. Example: Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665")
        "device-key-path"    => (device.key_path, "Path to the private key file. Example: /home/user/certificate/tedge-private-key.pem")
        "device-cert-path"   => (device.cert_path, "Path to the certificate file. Example: /home/user/certificate/tedge-certificate.crt")
        "c8y-url"            => (c8y.url, "Tenant endpoint URL of Cumulocity tenant. Example: your-tenant.cumulocity.com")
        "c8y-root-cert-path" => (c8y.root_cert_path, "Path where Cumulocity root certificate(s) are located. Example: /home/user/certificate/c8y-trusted-root-certificates.pem")
        "c8y-connect"        => (c8y.connect, "Connection status to the provided Cumulocity tenant. Example: true")
    }
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
    connect: Option<String>,
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

fn print_config_list(config: &TEdgeConfig, all: bool) -> Result<(), ConfigError> {
    let mut keys_without_values: Vec<&str> = Vec::new();
    for key in TEdgeConfig::valid_keys() {
        let opt = config.get_config_value(key)?;
        match opt {
            Some(value) => println!("{}={}", key, value),
            None => keys_without_values.push(key),
        }
    }
    if all && !keys_without_values.is_empty() {
        println!();
        for key in keys_without_values {
            println!("{}=", key);
        }
    }
    Ok(())
}

fn print_config_doc() {
    for key in TEdgeConfig::valid_keys() {
        let desc = TEdgeConfig::get_description_of_key(key);
        println!("{:<30} {}", key, desc);
    }
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
                _ => Err(ConfigError::IOError(err)),
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
            Err(err) => Err(err.error.into()),
        }
    }

    pub fn get_config_value(&self, key: &str) -> Result<Option<String>, ConfigError> {
        self._get_config_value(key)
            .map(|opt_str| opt_str.map(Into::into))
    }

    pub fn set_config_value(&mut self, key: &str, value: String) -> Result<(), ConfigError> {
        self._set_config_value(key, Some(value))
    }

    pub fn unset_config_value(&mut self, key: &str) -> Result<(), ConfigError> {
        self._set_config_value(key, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn test_macro_creates_valid_keys_correctly() {
        assert_eq!(TEdgeConfig::valid_keys().contains(&"device-id"), true);
        assert_eq!(TEdgeConfig::valid_keys().contains(&"device.id"), false);
    }

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
            config.device.cert_path.unwrap(),
            DeviceConfig::default_cert_path().unwrap()
        );
        assert_eq!(
            config.device.key_path.unwrap(),
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
            config
                .set_config_value("invalid-key", "dummy-value".into())
                .unwrap_err(),
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
