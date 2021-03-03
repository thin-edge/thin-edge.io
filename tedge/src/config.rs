use crate::command::{BuildCommand, Command};
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

pub const DEVICE_ID: &str = "device.id";
pub const DEVICE_CERT_PATH: &str = "device.cert.path";
pub const DEVICE_KEY_PATH: &str = "device.key.path";

pub const C8Y_CONNECT: &str = "c8y.connect";
pub const C8Y_URL: &str = "c8y.url";
pub const C8Y_ROOT_CERT_PATH: &str = "c8y.root.cert.path";

// CIT-221 will use them. Remove the prefix `_` later
pub const _AZURE_CONNECT: &str = "azure.connect";
pub const _AZURE_URL: &str = "azure.url";
pub const _AZURE_ROOT_CERT_PATH: &str = "azure.root.cert.path";

/// Wrapper type for Configuration keys.
#[derive(Debug, Clone)]
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
        #[structopt(help = TEdgeConfig::valid_keys_help_message_for_set())]
        key: ConfigKey,

        /// Configuration value.
        value: String,
    },

    /// Unset the provided configuration key
    Unset {
        /// Configuration key.
        #[structopt(help = TEdgeConfig::valid_keys_help_message_for_set())]
        key: ConfigKey,
    },

    /// Get the value of the provided configuration key
    Get {
        /// Configuration key.
        #[structopt(help = TEdgeConfig::valid_keys_help_message_for_get())]
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

impl BuildCommand for ConfigCmd {
    fn build_command(self, _config: TEdgeConfig) -> Result<Box<dyn Command>, ConfigError> {
        // Temporary implementation
        // - should return a specific command, not self.
        // - see certificate.rs for an example
        Ok(self.into_boxed())
    }
}

impl Command for ConfigCmd {
    fn description(&self) -> String {
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

    fn execute(&self, _verbose: u8) -> Result<(), anyhow::Error> {
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
            ConfigCmd::Set { key, value } => match TEdgeConfig::can_set_by_cli(key.as_str()) {
                true => {
                    config.set_config_value(key.as_str(), value.to_string())?;
                    config_updated = true;
                }
                false => Err(ConfigError::ForbiddenToSetByCLI {
                    key: key.as_str().parse()?,
                })?,
            },
            ConfigCmd::Unset { key } => match TEdgeConfig::can_set_by_cli(key.as_str()) {
                true => {
                    config.unset_config_value(key.as_str())?;
                    config_updated = true;
                }
                false => Err(ConfigError::ForbiddenToSetByCLI {
                    key: key.as_str().parse()?,
                })?,
            },
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

/// Represents the complete configuration of a thin edge device.
/// This configuration is a wrapper over the device specific configurations
/// as well as the IoT cloud provider specific configurations.
///
/// The following example showcases how the thin edge configuration can be read
/// and how individual configuration values can be retrieved out of it:
///
/// # Examples
/// ```
/// /// Read the default tedge.toml file into a TEdgeConfig object
/// let config: TEdgeConfig = TEdgeConfig::from_default_config().unwrap();
///
/// /// Fetch the device config from the TEdgeConfig object
/// let device_config: DeviceConfig = config.device;
/// /// Fetch the device id from the DeviceConfig object
/// let device_id = device_config.id.unwrap();
///
/// /// Fetch the Cumulocity config from the TEdgeConfig object
/// let cumulocity_config: CumulocityConfig = config.c8y;
/// /// Fetch the Cumulocity URL from the CumulocityConfig object
/// let cumulocity_url = cumulocity_config.url.unwrap();
/// ```
///
#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct TEdgeConfig {
    /// Captures the device specific configurations
    #[serde(default)]
    pub device: DeviceConfig,

    /// Captures the configurations required to connect to Cumulocity
    #[serde(default)]
    pub c8y: CumulocityConfig,
    #[serde(default)]
    pub azure: AzureConfig,
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
macro_rules! hide_key {
    ($str:literal, Public) => {
        concat!(" ", $str)
    };
    ($str:literal, Protected) => {
        ""
    };
    ($str:literal, Computed) => {
        ""
    };
}

macro_rules! set_keys_by_cli {
    ($str:literal, Public) => {
        true
    };
    ($str:literal, Protected) => {
        false
    };
    ($str:literal, Computed) => {
        false
    };
}

macro_rules! set_keys_by_api {
    ($str:literal, Public) => {
        true
    };
    ($str:literal, Protected) => {
        true
    };
    ($str:literal, Computed) => {
        false
    };
}

macro_rules! config_keys {
    ($ty:ty { $( $str:literal => ( $( $key:ident ).* , $type:tt, $desc:literal ) )* }) => {
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

            fn valid_keys_help_message_for_get() -> &'static str {
                concat!("[", $( " ", $str ),*, " ]")
            }

            fn valid_keys_help_message_for_set() -> &'static str {
                concat!("[", $( hide_key!($str, $type) ),*, " ]")
            }

            fn get_description_of_key(key: &str) -> &'static str {
                match key {
                    $( $str => $desc, )*
                    _ => "Undefined key",
                }
            }

            fn can_set_by_cli(key: &str) -> bool {
                match key {
                    $( $str => set_keys_by_cli!($str, $type), )*
                    _ => false,
                }
            }

            fn can_set_by_api(key: &str) -> bool {
                match key {
                    $( $str => set_keys_by_api!($str, $type), )*
                    _ => false,
                }
            }
        }
    }
}

config_keys! {
    TEdgeConfig {
        // external key => (internal key, type description)
        "device.id"            => (device.id, Computed, "Identifier of the device within the fleet. It must be globally unique. Example: Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665")
        "device.key.path"      => (device.key_path, Public, "Path to the private key file. Example: /home/user/certificate/tedge-private-key.pem")
        "device.cert.path"     => (device.cert_path, Public, "Path to the certificate file. Example: /home/user/certificate/tedge-certificate.crt")
        "c8y.url"              => (c8y.url, Public, "Tenant endpoint URL of Cumulocity tenant. Example: your-tenant.cumulocity.com")
        "c8y.root.cert.path"   => (c8y.root_cert_path, Public, "Path where Cumulocity root certificate(s) are located. Example: /home/user/certificate/c8y-trusted-root-certificates.pem")
        "c8y.connect"          => (c8y.connect, Protected, "Connection status to the provided Cumulocity tenant. Example: true")
        "azure.url"            => (azure.url, Public, "Tenant endpoint URL of Azure IoT tenant. Example:  MyAzure.azure-devices.net")
        "azure.root.cert.path" => (azure.root_cert_path, Public, "Path where Azure IoT root certificate(s) are located. Example: /home/user/certificate/azure-trusted-root-certificates.pem")
        "azure.connect"        => (azure.connect, Public, "Connection status to the provided Azure IoT tenant. Example: true")
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

/// Represents the device specific configurations defined in the [device] section
/// of the thin edge configuration TOML file
#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct DeviceConfig {
    /// The unique id of the device
    pub id: Option<String>,

    /// Path where the device's private key is stored.
    /// Defaults to $HOME/.tedge/tedge-private.pem
    pub key_path: Option<String>,

    /// Path where the device's certificate is stored.
    /// Defaults to $HOME/.tedge/tedge-certificate.crt
    pub cert_path: Option<String>,
}

impl DeviceConfig {
    fn default_cert_path() -> Result<String, ConfigError> {
        Self::path_in_cert_directory(DEVICE_CERT_FILE)
    }

    fn default_key_path() -> Result<String, ConfigError> {
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

/// Represents the Cumulocity specific configurations defined in the
/// [c8y] section of the thin edge configuration TOML file
#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CumulocityConfig {
    /// Preserves the current status of the connection
    connect: Option<String>,

    /// Endpoint URL of the Cumulocity tenant
    url: Option<String>,

    /// The path where Cumulocity root certificate(s) are stored.
    /// The value can be a directory path as well as the path of the direct certificate file.
    root_cert_path: Option<String>,
}

#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct AzureConfig {
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

    #[error(
        r#"A value for `{key}` is missing.
    A value can be set with `tedge config set {key} <value>`"#
    )]
    ConfigNotSet { key: String },

    #[error("The provided config key: {key} is forbidden to change the value by user")]
    ForbiddenToSetByCLI { key: String },

    #[error("The provided config key: {key} is forbidden to change the value by API")]
    ForbiddenToSetByAPI { key: String },
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
    /// Parse the configuration file at `$HOME/.tedge/tedge.toml` and create a `TEdgeConfig` out of it
    /// The retrieved configuration will have default values applied to any unconfigured field
    /// for which a default value is available.
    pub fn from_default_config() -> Result<TEdgeConfig, ConfigError> {
        Self::from_custom_config(tedge_config_path()?.as_path())
    }

    /// Parse the configuration file at the provided `path` and create a `TEdgeConfig` out of it
    /// The retrieved configuration will have default values applied to any unconfigured field
    /// for which a default value is available.
    ///
    /// #Arguments
    ///
    /// * `path` - Path to a thin edge configuration TOML file
    ///
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

    /// Persists this `TEdgeConfig` to $HOME/.tedge/tedge.toml
    pub fn write_to_default_config(&self) -> Result<(), ConfigError> {
        self.write_to_custom_config(tedge_config_path()?.as_path())
    }

    /// Persists this `TEdgeConfig` to the `path` provided
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

    /// Get the value of the provided `key` from this configuration
    pub fn get_config_value(&self, key: &str) -> Result<Option<String>, ConfigError> {
        self._get_config_value(key)
            .map(|opt_str| opt_str.map(Into::into))
    }

    /// Associate the provided key with the given value in this configuration.
    /// If the key exists already with some value, it will be replaced by the new value.
    pub fn set_config_value(&mut self, key: &str, value: String) -> Result<(), ConfigError> {
        match TEdgeConfig::can_set_by_api(key) {
            true => self._set_config_value(key, Some(value)),
            false => Err(ConfigError::ForbiddenToSetByAPI{key: key.to_string()}),
        }
    }

    /// Remove the mapping for the provided `key` from this configuration
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
        assert_eq!(TEdgeConfig::valid_keys().contains(&"device.id"), true);
        assert_eq!(TEdgeConfig::valid_keys().contains(&"device-id"), false);
    }

    #[test]
    fn test_macro_creates_can_set_by_cli_correctly() {
        assert_eq!(TEdgeConfig::can_set_by_cli("device.id"), false);
        assert_eq!(TEdgeConfig::can_set_by_cli("device.cert.path"), true);
        assert_eq!(TEdgeConfig::can_set_by_cli("c8y.connect"), false);
    }

    #[test]
    fn test_macro_creates_can_set_by_api_correctly() {
        assert_eq!(TEdgeConfig::can_set_by_api("device.id"), false);
        assert_eq!(TEdgeConfig::can_set_by_api("device.cert.path"), true);
        assert_eq!(TEdgeConfig::can_set_by_api("c8y.connect"), true);
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
root_cert_path = "/path/to/c8y/root/cert"
connect = "true"

[azure]
url = "MyAzure.azure-devices.net"
root_cert_path = "/path/to/azure/root/cert"
connect = "false"
"#;

        let config_file = temp_file_with_content(toml_conf);
        let config = TEdgeConfig::from_custom_config(config_file.path()).unwrap();

        assert_eq!(config.device.id.unwrap(), "ABCD1234");
        assert_eq!(config.device.key_path.unwrap(), "/path/to/key");
        assert_eq!(config.device.cert_path.unwrap(), "/path/to/cert");

        assert_eq!(config.c8y.url.unwrap(), "your-tenant.cumulocity.com");
        assert_eq!(config.c8y.root_cert_path.unwrap(), "/path/to/c8y/root/cert");
        assert_eq!(config.c8y.connect.unwrap(), "true");

        assert_eq!(config.azure.url.unwrap(), "MyAzure.azure-devices.net");
        assert_eq!(
            config.azure.root_cert_path.unwrap(),
            "/path/to/azure/root/cert"
        );
        assert_eq!(config.azure.connect.unwrap(), "false");
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
root_cert_path = "/path/to/c8y/root/cert"
connect = "true"

[azure]
url = "MyAzure.azure-devices.net"
root_cert_path = "/path/to/azure/root/cert"
connect = "false"
"#;

        // Using a TempPath let's close the file (this is required on Windows for that test to work).
        let config_file_path = temp_file_with_content(toml_conf).into_temp_path();

        let mut config = TEdgeConfig::from_custom_config(config_file_path.as_ref()).unwrap();
        assert_eq!(config.device.id.as_ref().unwrap(), "ABCD1234");
        assert_eq!(config.device.key_path.as_ref().unwrap(), "/path/to/key");
        assert_eq!(config.device.cert_path.as_ref().unwrap(), "/path/to/cert");

        assert_eq!(
            config.c8y.url.as_ref().unwrap(),
            "your-tenant.cumulocity.com"
        );
        assert_eq!(
            config.c8y.root_cert_path.as_ref().unwrap(),
            "/path/to/c8y/root/cert"
        );
        assert_eq!(config.c8y.connect.as_ref().unwrap(), "true");

        assert_eq!(
            config.azure.url.as_ref().unwrap(),
            "MyAzure.azure-devices.net"
        );
        assert_eq!(
            config.azure.root_cert_path.as_ref().unwrap(),
            "/path/to/azure/root/cert"
        );
        assert_eq!(config.azure.connect.as_ref().unwrap(), "false");

        let updated_device_id = "XYZ1234";
        let updated_c8y_url = "other-tenant.cumulocity.com";
        let updated_c8y_connect = "false";
        let updated_azure_url = "OtherAzure.azure-devices.net";
        let updated_azure_connect = "true";

        config.device.id = Some(updated_device_id.to_string());
        config.c8y.url = Some(updated_c8y_url.to_string());
        config.c8y.root_cert_path = None;
        config.c8y.connect = Some(updated_c8y_connect.to_string());
        config.azure.url = Some(updated_azure_url.to_string());
        config.azure.root_cert_path = None;
        config.azure.connect = Some(updated_azure_connect.to_string());

        config
            .write_to_custom_config(config_file_path.as_ref())
            .unwrap();
        let config = TEdgeConfig::from_custom_config(config_file_path.as_ref()).unwrap();

        assert_eq!(config.device.id.as_ref().unwrap(), updated_device_id);
        assert_eq!(config.device.key_path.as_ref().unwrap(), "/path/to/key");
        assert_eq!(config.device.cert_path.as_ref().unwrap(), "/path/to/cert");

        assert_eq!(config.c8y.url.as_ref().unwrap(), updated_c8y_url);
        assert!(config.c8y.root_cert_path.is_none());
        assert_eq!(config.c8y.connect.as_ref().unwrap(), updated_c8y_connect);

        assert_eq!(config.azure.url.as_ref().unwrap(), updated_azure_url);
        assert!(config.azure.root_cert_path.is_none());
        assert_eq!(
            config.azure.connect.as_ref().unwrap(),
            updated_azure_connect
        );
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
        assert!(config.c8y.connect.is_none());
    }

    #[test]
    fn test_parse_config_missing_azure_configuration() {
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

        assert!(config.azure.url.is_none());
        assert!(config.azure.root_cert_path.is_none());
        assert!(config.azure.connect.is_none());
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
        assert!(config.c8y.connect.is_none());
        assert!(config.azure.url.is_none());
        assert!(config.azure.root_cert_path.is_none());
        assert!(config.azure.connect.is_none());
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
                .set_config_value("invalid.key", "dummy-value".into())
                .unwrap_err(),
            ConfigError::InvalidConfigKey { .. }
        );
    }

    #[test]
    fn test_get_config_key_invalid_key() {
        let config = TEdgeConfig::from_default_config().unwrap();
        assert_matches!(
            config.get_config_value("invalid.key").unwrap_err(),
            ConfigError::InvalidConfigKey { .. }
        );
    }

    #[test]
    fn test_unset_config_key_invalid_key() {
        let mut config = TEdgeConfig::from_default_config().unwrap();
        assert_matches!(
            config.unset_config_value("invalid.key").unwrap_err(),
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
root_cert_path = "/path/to/c8y/root/cert"
connect = "true"

[azure]
url = "MyAzure.azure-devices.net"
root_cert_path = "/path/to/azure/root/cert"
connect = "false"
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
        let original_c8y_root_cert_path = "/path/to/c8y/root/cert".to_string();
        let original_c8y_connect = "true".to_string();
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
        assert_eq!(
            config.get_config_value(C8Y_CONNECT).unwrap().unwrap(),
            original_c8y_connect
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
        assert_eq!(
            config.get_config_value(C8Y_CONNECT).unwrap().unwrap(),
            original_c8y_connect
        );
    }

    #[test]
    fn test_crud_config_value_azure() {
        let toml_conf = r#"
[device]
id = "ABCD1234"
key_path = "/path/to/key"
cert_path = "/path/to/cert"

[c8y]
url = "your-tenant.cumulocity.com"
root_cert_path = "/path/to/c8y/root/cert"
connect = "true"

[azure]
url = "MyAzure.azure-devices.net"
root_cert_path = "/path/to/azure/root/cert"
connect = "false"
"#;

        let config_file = temp_file_with_content(toml_conf);
        let mut config = TEdgeConfig::from_custom_config(config_file.path()).unwrap();

        let original_azure_url = "MyAzure.azure-devices.net".to_string();
        let original_azure_root_cert_path = "/path/to/azure/root/cert".to_string();
        let original_azure_connect = "false".to_string();

        // read
        assert_eq!(
            config.get_config_value(_AZURE_URL).unwrap().unwrap(),
            original_azure_url
        );
        assert_eq!(
            config
                .get_config_value(_AZURE_ROOT_CERT_PATH)
                .unwrap()
                .unwrap(),
            original_azure_root_cert_path
        );
        assert_eq!(
            config.get_config_value(_AZURE_CONNECT).unwrap().unwrap(),
            original_azure_connect
        );

        // set
        let updated_azure_url = "OtherAzure.azure-devices.net".to_string();
        config
            .set_config_value(_AZURE_URL, updated_azure_url.clone())
            .unwrap();
        assert_eq!(
            config.get_config_value(_AZURE_URL).unwrap().unwrap(),
            updated_azure_url
        );

        // unset
        config.unset_config_value(_AZURE_ROOT_CERT_PATH).unwrap();
        assert!(config
            .get_config_value(_AZURE_ROOT_CERT_PATH)
            .unwrap()
            .is_none());

        // no change
        assert_eq!(
            config.get_config_value(_AZURE_CONNECT).unwrap().unwrap(),
            original_azure_connect
        );
    }

    fn temp_file_with_content(content: &str) -> NamedTempFile {
        let file = NamedTempFile::new().unwrap();
        file.as_file().write_all(content.as_bytes()).unwrap();
        file
    }
}
