use crate::utils::paths;
use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, read_to_string};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

const TEDGE_CONFIG_FILE: &str = "tedge.toml";
const DEVICE_KEY_FILE: &str = "device-certs/tedge-private-key.pem";
const DEVICE_CERT_FILE: &str = "device-certs/tedge-certificate.pem";

pub const DEVICE_ID: &str = "device.id";
pub const DEVICE_CERT_PATH: &str = "device.cert.path";
pub const DEVICE_KEY_PATH: &str = "device.key.path";

pub const C8Y_URL: &str = "c8y.url";
pub const C8Y_ROOT_CERT_PATH: &str = "c8y.root.cert.path";

// CIT-221 will use them. Remove the prefix `_` later
pub const AZURE_URL: &str = "azure.url";
pub const AZURE_ROOT_CERT_PATH: &str = "azure.root.cert.path";

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
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
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
/// - get_key_properties (get ConfigKeyProperties of a key)
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

        }
    }
}

config_keys! {
    TEdgeConfig {
        // external key => (internal key, type description)
        "device.id"            => (device.id, ReadOnly, "Identifier of the device within the fleet. It must be globally unique and the same one used in the device certificate. Example: Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665")
        "device.key.path"      => (device.key_path, ReadWrite, "Path to the private key file. Example: /home/user/.tedge/tedge-private-key.pem")
        "device.cert.path"     => (device.cert_path, ReadWrite, "Path to the certificate file. Example: /home/user/.tedge/tedge-certificate.crt")
        "c8y.url"              => (c8y.url, ReadWrite, "Tenant endpoint URL of Cumulocity tenant. Example: your-tenant.cumulocity.com")
        "c8y.root.cert.path"   => (c8y.root_cert_path, ReadWrite, "Path where Cumulocity root certificate(s) are located. Example: /home/user/.tedge/c8y-trusted-root-certificates.pem")
        "azure.url"            => (azure.url, ReadWrite, "Tenant endpoint URL of Azure IoT tenant. Example:  MyAzure.azure-devices.net")
        "azure.root.cert.path" => (azure.root_cert_path, ReadWrite, "Path where Azure IoT root certificate(s) are located. Example: /home/user/.tedge/azure-trusted-root-certificates.pem")
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
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct DeviceConfig {
    /// The unique id of the device
    pub id: Option<String>,

    /// Path where the device's private key is stored.
    /// Defaults to $HOME/.tedge/tedge-private.pem for user or /etc/tedge/device-certs/tedge-private.pem for system wide configuration.
    pub key_path: Option<String>,

    /// Path where the device's certificate is stored.
    /// Defaults to $HOME/.tedge/tedge-certificate.crt for user or /etc/tedge/device-certs/tedge-certificate.crt for system wide configuration.
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
        Ok(paths::build_path_for_sudo_or_user(&[file_name])?)
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
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct CumulocityConfig {
    /// Preserves the current status of the connection
    connect: Option<String>,

    /// Endpoint URL of the Cumulocity tenant
    pub url: Option<String>,

    /// The path where Cumulocity root certificate(s) are stored.
    /// The value can be a directory path as well as the path of the direct certificate file.
    pub root_cert_path: Option<String>,
}

#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct AzureConfig {
    connect: Option<String>,
    url: Option<String>,
    pub root_cert_path: Option<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("TOML parse error")]
    TomlParseError(#[from] toml::de::Error),

    #[error("TOML serialization error")]
    InvalidTomlError(#[from] toml::ser::Error),

    #[error("I/O error")]
    IoError(#[from] std::io::Error),

    #[error(
        r#"A value for `{key}` is missing.
    A value can be set with `tedge config set {key} <value>`"#
    )]
    ConfigNotSet { key: String },

    #[error(r#"A value for `device.id` is missing. It can be set with 'tedge cert create --device-id <value>'"#)]
    DeviceIdNotSet,

    #[error("The provided config key: {key} is not a valid Thin Edge configuration key")]
    InvalidConfigKey { key: String },

    #[error(
        r#"Provided URL: '{0}' contains scheme or port.
    Provided URL should contain only domain, eg: 'subdomain.cumulocity.com'."#
    )]
    InvalidConfigUrl(String),

    #[error(transparent)]
    PathsError(#[from] paths::PathsError),

    #[error(transparent)]
    TEdgeConfigError(#[from] tedge_config::TEdgeConfigError),

    #[error(transparent)]
    TEdgeConfigSettingError(#[from] tedge_config::ConfigSettingError),
}

pub fn tedge_config_path() -> Result<PathBuf, ConfigError> {
    Ok(paths::build_path_for_sudo_or_user_as_path(&[
        TEDGE_CONFIG_FILE,
    ])?)
}

impl TEdgeConfig {
    /// Parse the configuration file at `/etc/tedge/tedge.toml` and create a `TEdgeConfig` out of it
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
    pub fn from_custom_config(path: &Path) -> Result<TEdgeConfig, ConfigError> {
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
                _ => Err(ConfigError::IoError(err)),
            },
        }
    }

    /// Persists this `TEdgeConfig` to $HOME/.tedge/tedge.toml for user or /etc/tedge/tedge.toml for system configuration.
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
        self._set_config_value(key, Some(value))
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

        assert_eq!(config.azure.url.unwrap(), "MyAzure.azure-devices.net");
        assert_eq!(
            config.azure.root_cert_path.unwrap(),
            "/path/to/azure/root/cert"
        );
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

[azure]
url = "MyAzure.azure-devices.net"
root_cert_path = "/path/to/azure/root/cert"
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

        assert_eq!(
            config.azure.url.as_ref().unwrap(),
            "MyAzure.azure-devices.net"
        );
        assert_eq!(
            config.azure.root_cert_path.as_ref().unwrap(),
            "/path/to/azure/root/cert"
        );

        let updated_device_id = "XYZ1234";
        let updated_c8y_url = "other-tenant.cumulocity.com";
        let updated_azure_url = "OtherAzure.azure-devices.net";

        config.device.id = Some(updated_device_id.to_string());
        config.c8y.url = Some(updated_c8y_url.to_string());
        config.c8y.root_cert_path = None;
        config.azure.url = Some(updated_azure_url.to_string());
        config.azure.root_cert_path = None;

        config
            .write_to_custom_config(config_file_path.as_ref())
            .unwrap();
        let config = TEdgeConfig::from_custom_config(config_file_path.as_ref()).unwrap();

        assert_eq!(config.device.id.as_ref().unwrap(), updated_device_id);
        assert_eq!(config.device.key_path.as_ref().unwrap(), "/path/to/key");
        assert_eq!(config.device.cert_path.as_ref().unwrap(), "/path/to/cert");

        assert_eq!(config.c8y.url.as_ref().unwrap(), updated_c8y_url);
        assert!(config.c8y.root_cert_path.is_none());

        assert_eq!(config.azure.url.as_ref().unwrap(), updated_azure_url);
        assert!(config.azure.root_cert_path.is_none());
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
        assert!(config.azure.url.is_none());
        assert!(config.azure.root_cert_path.is_none());
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
            ConfigError::TomlParseError(_),
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
            ConfigError::TomlParseError(_),
            "Expected the parsing to fail with TOMLParseError"
        );
    }

    #[test]
    fn test_set_config_key_invalid_key() {
        let toml_conf = "[device]";

        let config_file = temp_file_with_content(toml_conf);
        let mut config = TEdgeConfig::from_custom_config(config_file.path()).unwrap();
        assert_matches!(
            config
                .set_config_value("invalid.key", "dummy-value".into())
                .unwrap_err(),
            ConfigError::InvalidConfigKey { .. }
        );
    }

    #[test]
    fn test_get_config_key_invalid_key() {
        let toml_conf = "[device]";

        let config_file = temp_file_with_content(toml_conf);
        let config = TEdgeConfig::from_custom_config(config_file.path()).unwrap();
        assert_matches!(
            config.get_config_value("invalid.key").unwrap_err(),
            ConfigError::InvalidConfigKey { .. }
        );
    }

    #[test]
    fn test_unset_config_key_invalid_key() {
        let toml_conf = "[device]";

        let config_file = temp_file_with_content(toml_conf);
        let mut config = TEdgeConfig::from_custom_config(config_file.path()).unwrap();
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

[azure]
url = "MyAzure.azure-devices.net"
root_cert_path = "/path/to/azure/root/cert"
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

[azure]
url = "MyAzure.azure-devices.net"
root_cert_path = "/path/to/azure/root/cert"
"#;

        let config_file = temp_file_with_content(toml_conf);
        let mut config = TEdgeConfig::from_custom_config(config_file.path()).unwrap();

        let original_azure_url = "MyAzure.azure-devices.net".to_string();
        let original_azure_root_cert_path = "/path/to/azure/root/cert".to_string();

        // read
        assert_eq!(
            config.get_config_value(AZURE_URL).unwrap().unwrap(),
            original_azure_url
        );
        assert_eq!(
            config
                .get_config_value(AZURE_ROOT_CERT_PATH)
                .unwrap()
                .unwrap(),
            original_azure_root_cert_path
        );

        // set
        let updated_azure_url = "OtherAzure.azure-devices.net".to_string();
        config
            .set_config_value(AZURE_URL, updated_azure_url.clone())
            .unwrap();
        assert_eq!(
            config.get_config_value(AZURE_URL).unwrap().unwrap(),
            updated_azure_url
        );

        // unset
        config.unset_config_value(AZURE_ROOT_CERT_PATH).unwrap();
        assert!(config
            .get_config_value(AZURE_ROOT_CERT_PATH)
            .unwrap()
            .is_none());
    }

    fn temp_file_with_content(content: &str) -> NamedTempFile {
        let file = NamedTempFile::new().unwrap();
        file.as_file().write_all(content.as_bytes()).unwrap();
        file
    }
}
