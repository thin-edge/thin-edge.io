use crate::config::ConfigError::{HomeDirectoryNotFound, InvalidCharacterInHomeDirectoryPath};
use std::fs::{create_dir_all, read_to_string};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

mod error;
mod tedge_config;

pub use error::*;
pub use tedge_config::*;

pub const TEDGE_HOME_DIR: &str = ".tedge";
const TEDGE_CONFIG_FILE: &str = "tedge.toml";
const DEVICE_KEY_FILE: &str = "tedge-private-key.pem";
const DEVICE_CERT_FILE: &str = "tedge-certificate.pem";

impl TEdgeConfig {
    fn with_defaults(self) -> Result<Self, ConfigError> {
        let device_config = self.device.with_defaults()?;

        Ok(TEdgeConfig {
            device: device_config,
            ..self
        })
    }
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
            .join(TEDGE_HOME_DIR)
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

pub fn home_dir() -> Result<PathBuf, ConfigError> {
    // The usage of this deprecated method is temporary as this whole function will be replaced with the util function being added in CIT-137.
    #![allow(deprecated)]
    std::env::home_dir().ok_or(HomeDirectoryNotFound)
}

pub fn tedge_config_path() -> Result<PathBuf, ConfigError> {
    Ok(home_dir()?.join(TEDGE_HOME_DIR).join(TEDGE_CONFIG_FILE))
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{settings::*, types::*};
    use assert_matches::assert_matches;
    use std::convert::TryFrom;

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

        assert_eq!(
            config.c8y.url.unwrap().as_str(),
            "your-tenant.cumulocity.com"
        );
        assert_eq!(config.c8y.root_cert_path.unwrap(), "/path/to/c8y/root/cert");

        assert_eq!(
            config.azure.url.unwrap().as_str(),
            "MyAzure.azure-devices.net"
        );
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
            config.c8y.url.as_ref().unwrap().as_str(),
            "your-tenant.cumulocity.com"
        );
        assert_eq!(
            config.c8y.root_cert_path.as_ref().unwrap(),
            "/path/to/c8y/root/cert"
        );

        assert_eq!(
            config.azure.url.as_ref().unwrap().as_str(),
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
        config.c8y.url = Some(ConnectUrl::try_from(updated_c8y_url.to_string()).unwrap());
        config.c8y.root_cert_path = None;
        config.azure.url = Some(ConnectUrl::try_from(updated_azure_url.to_string()).unwrap());
        config.azure.root_cert_path = None;

        config
            .write_to_custom_config(config_file_path.as_ref())
            .unwrap();
        let config = TEdgeConfig::from_custom_config(config_file_path.as_ref()).unwrap();

        assert_eq!(config.device.id.as_ref().unwrap(), updated_device_id);
        assert_eq!(config.device.key_path.as_ref().unwrap(), "/path/to/key");
        assert_eq!(config.device.cert_path.as_ref().unwrap(), "/path/to/cert");

        assert_eq!(config.c8y.url.as_ref().unwrap().as_str(), updated_c8y_url);
        assert!(config.c8y.root_cert_path.is_none());

        assert_eq!(
            config.azure.url.as_ref().unwrap().as_str(),
            updated_azure_url
        );
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

        assert_eq!(
            config.c8y.url.unwrap().as_str(),
            "your-tenant.cumulocity.com"
        );

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
            DeviceIdSetting.get_string(&config).unwrap(),
            original_device_id
        );
        assert_eq!(
            DeviceKeyPathSetting.get_string(&config).unwrap(),
            original_device_key_path
        );
        assert_eq!(
            DeviceCertPathSetting.get_string(&config).unwrap(),
            original_device_cert_path
        );

        let original_c8y_url = "your-tenant.cumulocity.com".to_string();
        let original_c8y_root_cert_path = "/path/to/c8y/root/cert".to_string();
        assert_eq!(
            C8yUrlSetting.get(&config).unwrap().as_str(),
            original_c8y_url
        );
        assert_eq!(
            C8yRootCertPathSetting.get_string(&config).unwrap(),
            original_c8y_root_cert_path
        );

        // let updated_device_id = "XYZ1234".to_string();
        let updated_c8y_url = "other-tenant.cumulocity.com".to_string();

        // DeviceIdSetting.set_string(&mut config, updated_device_id.clone()).unwrap();
        C8yUrlSetting
            .set_string(&mut config, updated_c8y_url.clone())
            .unwrap();
        C8yRootCertPathSetting.unset(&mut config).unwrap();

        /*
        assert_eq!(
            config.get_config_value(DEVICE_ID).unwrap().unwrap(),
            updated_device_id
        );
        */
        assert_eq!(
            DeviceKeyPathSetting.get_string(&config).unwrap(),
            original_device_key_path
        );
        assert_eq!(
            DeviceCertPathSetting.get_string(&config).unwrap(),
            original_device_cert_path
        );

        assert_eq!(
            C8yUrlSetting.get(&config).unwrap().as_str(),
            updated_c8y_url
        );
        assert!(C8yRootCertPathSetting.get_string(&config).is_err());
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
            AzureUrlSetting.get(&config).unwrap().as_str(),
            original_azure_url
        );
        assert_eq!(
            AzureRootCertPathSetting.get_string(&config).unwrap(),
            original_azure_root_cert_path
        );

        // set
        let updated_azure_url = "OtherAzure.azure-devices.net".to_string();
        AzureUrlSetting
            .set_string(&mut config, updated_azure_url.clone())
            .unwrap();
        assert_eq!(
            AzureUrlSetting.get(&config).unwrap().as_str(),
            updated_azure_url
        );

        // unset
        AzureRootCertPathSetting.unset(&mut config).unwrap();
        assert!(AzureRootCertPathSetting.get_string(&config).is_err());
    }

    fn temp_file_with_content(content: &str) -> NamedTempFile {
        let file = NamedTempFile::new().unwrap();
        file.as_file().write_all(content.as_bytes()).unwrap();
        file
    }
}
