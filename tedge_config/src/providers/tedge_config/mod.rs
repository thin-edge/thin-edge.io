use crate::*;
use std::fs::{create_dir_all, read_to_string};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

pub const TEDGE_HOME_DIR: &str = ".tedge";
const TEDGE_CONFIG_FILE: &str = "tedge.toml";
const DEVICE_KEY_FILE: &str = "tedge-private-key.pem";
const DEVICE_CERT_FILE: &str = "tedge-certificate.pem";
const DEFAULT_ROOT_CERT_PATH: &str = "/etc/ssl/certs";

#[derive(Debug)]
pub struct TEdgeConfig {
    data: TEdgeConfigDto,
}

// For now, just proxy settings to the underlying TEdgeConfigDto.
impl<T: ConfigSetting> QuerySetting<T> for TEdgeConfig
where
    TEdgeConfigDto: QuerySetting<T>,
{
    fn query(&self, setting: T) -> ConfigSettingResult<T::Value> {
        self.data.query(setting)
    }
}

// For now, just proxy settings to the underlying TEdgeConfigDto.
impl<T: ConfigSetting> UpdateSetting<T> for TEdgeConfig
where
    TEdgeConfigDto: UpdateSetting<T>,
{
    fn update(&mut self, setting: T, value: T::Value) -> ConfigSettingResult<()> {
        self.data.update(setting, value)
    }
}

// For now, just proxy settings to the underlying TEdgeConfigDto.
impl<T: ConfigSetting> UnsetSetting<T> for TEdgeConfig
where
    TEdgeConfigDto: UnsetSetting<T>,
{
    fn unset(&mut self, setting: T) -> ConfigSettingResult<()> {
        self.data.unset(setting)
    }
}

impl QuerySettingWithDefault<AzureRootCertPathSetting> for TEdgeConfig {
    fn query_with_default(&self, setting: AzureRootCertPathSetting) -> ConfigSettingResult<String> {
        match self.data.query(setting) {
            Ok(value) => Ok(value),
            Err(ConfigSettingError::ConfigNotSet { .. }) => Ok(DEFAULT_ROOT_CERT_PATH.into()),
            Err(other) => Err(other),
        }
    }
}

impl QuerySettingWithDefault<C8yRootCertPathSetting> for TEdgeConfig {
    fn query_with_default(&self, setting: C8yRootCertPathSetting) -> ConfigSettingResult<String> {
        match self.data.query(setting) {
            Ok(value) => Ok(value),
            Err(ConfigSettingError::ConfigNotSet { .. }) => Ok(DEFAULT_ROOT_CERT_PATH.into()),
            Err(other) => Err(other),
        }
    }
}

impl TEdgeConfig {
    /// Parse the configuration file at `$HOME/.tedge/tedge.toml` and create a `TEdgeConfigDto` out of it
    /// The retrieved configuration will have default values applied to any unconfigured field
    /// for which a default value is available.
    pub fn from_default_config() -> Result<TEdgeConfig, ConfigError> {
        Self::from_custom_config(tedge_config_path()?.as_path())
    }

    /// Parse the configuration file at the provided `path` and create a `TEdgeConfigDto` out of it
    /// The retrieved configuration will have default values applied to any unconfigured field
    /// for which a default value is available.
    ///
    /// #Arguments
    ///
    /// * `path` - Path to a thin edge configuration TOML file
    ///
    pub fn from_custom_config(path: &Path) -> Result<TEdgeConfig, ConfigError> {
        let mut config = Self::load_from(path)?;
        config.update_if_not_set(DeviceKeyPathSetting, default_device_key_path()?)?;
        config.update_if_not_set(DeviceCertPathSetting, default_device_cert_path()?)?;
        Ok(config)
    }

    fn update_if_not_set<T: ConfigSetting + Copy>(
        &mut self,
        setting: T,
        value: T::Value,
    ) -> Result<(), ConfigError>
    where
        Self: QuerySetting<T> + UpdateSetting<T>,
    {
        match self.query(setting) {
            Err(ConfigSettingError::ConfigNotSet { .. }) => {
                self.update(setting, value)?;
                Ok(())
            }
            Err(other) => Err(other.into()),
            Ok(_ok) => Ok(()),
        }
    }

    fn load_from(path: &Path) -> Result<TEdgeConfig, ConfigError> {
        let data = match read_to_string(path) {
            Ok(content) => toml::from_str::<TEdgeConfigDto>(content.as_str())?,
            Err(err) => match err.kind() {
                ErrorKind::NotFound => TEdgeConfigDto::default(),
                _ => Err(ConfigError::IOError(err))?,
            },
        };
        Ok(Self { data })
    }

    /// Persists this `TEdgeConfigDto` to $HOME/.tedge/tedge.toml
    pub fn write_to_default_config(&self) -> Result<(), ConfigError> {
        self.write_to_custom_config(tedge_config_path()?.as_path())
    }

    #[cfg(test)]
    fn data(&self) -> &TEdgeConfigDto {
        &self.data
    }

    #[cfg(test)]
    fn data_mut(&mut self) -> &mut TEdgeConfigDto {
        &mut self.data
    }

    /// Persists this `TEdgeConfigDto` to the `path` provided
    fn write_to_custom_config(&self, path: &Path) -> Result<(), ConfigError> {
        let toml = toml::to_string_pretty(&self.data)?;
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

fn default_device_key_path() -> Result<String, ConfigError> {
    path_in_cert_directory(DEVICE_KEY_FILE)
}

fn default_device_cert_path() -> Result<String, ConfigError> {
    path_in_cert_directory(DEVICE_CERT_FILE)
}

fn path_in_cert_directory(file_name: &str) -> Result<String, ConfigError> {
    home_dir()?
        .join(TEDGE_HOME_DIR)
        .join(file_name)
        .to_str()
        .map(|s| s.into())
        .ok_or(ConfigError::InvalidCharacterInHomeDirectoryPath)
}

fn home_dir() -> Result<PathBuf, ConfigError> {
    // The usage of this deprecated method is temporary as this whole function will be replaced with the util function being added in CIT-137.
    #![allow(deprecated)]
    std::env::home_dir().ok_or(ConfigError::HomeDirectoryNotFound)
}

fn tedge_config_path() -> Result<PathBuf, ConfigError> {
    Ok(home_dir()?.join(TEDGE_HOME_DIR).join(TEDGE_CONFIG_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let cfg = TEdgeConfig::from_custom_config(config_file.path()).unwrap();
        let config = cfg.data();

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

        let updated_device_id = "XYZ1234";
        let updated_c8y_url = "other-tenant.cumulocity.com";
        let updated_azure_url = "OtherAzure.azure-devices.net";

        {
            let mut cfg = TEdgeConfig::from_custom_config(config_file_path.as_ref()).unwrap();
            let config = cfg.data_mut();
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

            config.device.id = Some(updated_device_id.to_string());
            config.c8y.url = Some(ConnectUrl::try_from(updated_c8y_url.to_string()).unwrap());
            config.c8y.root_cert_path = None;
            config.azure.url = Some(ConnectUrl::try_from(updated_azure_url.to_string()).unwrap());
            config.azure.root_cert_path = None;
            cfg.write_to_custom_config(config_file_path.as_ref())
                .unwrap();
        }

        {
            let cfg = TEdgeConfig::from_custom_config(config_file_path.as_ref()).unwrap();
            let config = cfg.data();

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
    }

    #[test]
    fn test_parse_config_missing_c8y_configuration() {
        let toml_conf = r#"
[device]
id = "ABCD1234"
"#;

        let config_file = temp_file_with_content(toml_conf);
        let cfg = TEdgeConfig::from_custom_config(config_file.path()).unwrap();
        let config = cfg.data();

        assert_eq!(config.device.id.as_ref().unwrap(), "ABCD1234");
        assert_eq!(
            config.device.cert_path.clone().unwrap(),
            default_device_cert_path().unwrap()
        );
        assert_eq!(
            config.device.key_path.clone().unwrap(),
            default_device_key_path().unwrap()
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
        let cfg = TEdgeConfig::from_custom_config(config_file.path()).unwrap();
        let config = cfg.data();

        assert_eq!(config.device.id.as_ref().unwrap(), "ABCD1234");
        assert_eq!(
            config.device.cert_path.clone().unwrap(),
            default_device_cert_path().unwrap()
        );
        assert_eq!(
            config.device.key_path.clone().unwrap(),
            default_device_key_path().unwrap()
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
        let cfg = TEdgeConfig::from_custom_config(config_file.path()).unwrap();
        let config = cfg.data();

        assert_eq!(
            config.c8y.url.as_ref().unwrap().as_str(),
            "your-tenant.cumulocity.com"
        );

        assert!(config.device.id.is_none());
        assert_eq!(
            config.device.cert_path.clone().unwrap(),
            default_device_cert_path().unwrap()
        );
        assert_eq!(
            config.device.key_path.clone().unwrap(),
            default_device_key_path().unwrap()
        );
    }

    #[test]
    fn test_parse_config_empty_file() {
        let config_file = NamedTempFile::new().unwrap();
        let cfg = TEdgeConfig::from_custom_config(config_file.path()).unwrap();
        let config = cfg.data();

        assert!(config.device.id.is_none());
        assert_eq!(
            config.device.cert_path.clone().unwrap(),
            default_device_cert_path().unwrap()
        );
        assert_eq!(
            config.device.key_path.clone().unwrap(),
            default_device_key_path().unwrap()
        );

        assert!(config.c8y.url.is_none());
        assert!(config.c8y.root_cert_path.is_none());
        assert!(config.azure.url.is_none());
        assert!(config.azure.root_cert_path.is_none());
    }

    #[test]
    fn test_parse_config_no_config_file() {
        let cfg = TEdgeConfig::from_custom_config(Path::new("/non/existent/path")).unwrap();
        let config = cfg.data();

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
        assert_eq!(config.query(DeviceIdSetting).unwrap(), original_device_id);
        assert_eq!(
            config.query(DeviceKeyPathSetting).unwrap(),
            original_device_key_path
        );
        assert_eq!(
            config.query(DeviceCertPathSetting).unwrap(),
            original_device_cert_path
        );

        let original_c8y_url = "your-tenant.cumulocity.com".to_string();
        let original_c8y_root_cert_path = "/path/to/c8y/root/cert".to_string();
        assert_eq!(
            config.query_string(C8yUrlSetting).unwrap(),
            original_c8y_url
        );
        assert_eq!(
            config.query_string(C8yRootCertPathSetting).unwrap(),
            original_c8y_root_cert_path
        );

        // let updated_device_id = "XYZ1234".to_string();
        let updated_c8y_url =
            ConnectUrl::try_from("other-tenant.cumulocity.com".to_string()).unwrap();

        // DeviceIdSetting.set_string(&mut config, updated_device_id.clone()).unwrap();
        config
            .update(C8yUrlSetting, updated_c8y_url.clone())
            .unwrap();

        config.unset(C8yRootCertPathSetting).unwrap();

        /*
        assert_eq!(
            config.get_config_value(DEVICE_ID).unwrap().unwrap(),
            updated_device_id
        );
        */
        assert_eq!(
            config.query(DeviceKeyPathSetting).unwrap(),
            original_device_key_path
        );
        assert_eq!(
            config.query(DeviceCertPathSetting).unwrap(),
            original_device_cert_path
        );

        assert_eq!(config.query(C8yUrlSetting).unwrap(), updated_c8y_url);
        assert!(config.query(C8yRootCertPathSetting).is_err());
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
            config.query_string(AzureUrlSetting).unwrap(),
            original_azure_url
        );
        assert_eq!(
            config.query_string(AzureRootCertPathSetting).unwrap(),
            original_azure_root_cert_path
        );

        // set
        let updated_azure_url =
            ConnectUrl::try_from("OtherAzure.azure-devices.net".to_string()).unwrap();
        config
            .update(AzureUrlSetting, updated_azure_url.clone())
            .unwrap();

        assert_eq!(config.query(AzureUrlSetting).unwrap(), updated_azure_url);

        // unset
        config.unset(AzureRootCertPathSetting).unwrap();
        assert!(config.query_string(AzureRootCertPathSetting).is_err());
    }

    fn temp_file_with_content(content: &str) -> NamedTempFile {
        let file = NamedTempFile::new().unwrap();
        file.as_file().write_all(content.as_bytes()).unwrap();
        file
    }
}
