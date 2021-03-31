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

/// Represents the complete configuration of a thin edge device.
/// This configuration is a wrapper over the device specific configurations
/// as well as the IoT cloud provider specific configurations.
///
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

    /// Persists this `TEdgeConfigDto` to the `path` provided
    pub fn write_to_custom_config(&self, path: &Path) -> Result<(), ConfigError> {
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
