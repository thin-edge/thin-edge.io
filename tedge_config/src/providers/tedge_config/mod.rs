use crate::*;
use std::path::Path;

const DEFAULT_ROOT_CERT_PATH: &str = "/etc/ssl/certs";

/// Represents the complete configuration of a thin edge device.
/// This configuration is a wrapper over the device specific configurations
/// as well as the IoT cloud provider specific configurations.
///
#[derive(Debug)]
pub struct TEdgeConfig {
    file: TomlConfigFile<TEdgeConfigDto>,
}

// For now, just proxy settings to the underlying TEdgeConfigDto.
impl<T: ConfigSetting> QuerySetting<T> for TEdgeConfig
where
    TomlConfigFile<TEdgeConfigDto>: QuerySetting<T>,
{
    fn query(&self, setting: T) -> ConfigSettingResult<T::Value> {
        self.file.query(setting)
    }
}

// For now, just proxy settings to the underlying TEdgeConfigDto.
impl<T: ConfigSetting> UpdateSetting<T> for TEdgeConfig
where
    TomlConfigFile<TEdgeConfigDto>: UpdateSetting<T>,
{
    fn update(&mut self, setting: T, value: T::Value) -> ConfigSettingResult<()> {
        self.file.update(setting, value)
    }
}

// For now, just proxy settings to the underlying TEdgeConfigDto.
impl<T: ConfigSetting> UnsetSetting<T> for TEdgeConfig
where
    TomlConfigFile<TEdgeConfigDto>: UnsetSetting<T>,
{
    fn unset(&mut self, setting: T) -> ConfigSettingResult<()> {
        self.file.unset(setting)
    }
}

impl QuerySettingWithDefault<AzureRootCertPathSetting> for TEdgeConfig {
    fn query_with_default(&self, setting: AzureRootCertPathSetting) -> ConfigSettingResult<String> {
        match self.file.query(setting) {
            Ok(value) => Ok(value),
            Err(ConfigSettingError::ConfigNotSet { .. }) => Ok(DEFAULT_ROOT_CERT_PATH.into()),
            Err(other) => Err(other),
        }
    }
}

impl QuerySettingWithDefault<C8yRootCertPathSetting> for TEdgeConfig {
    fn query_with_default(&self, setting: C8yRootCertPathSetting) -> ConfigSettingResult<String> {
        match self.file.query(setting) {
            Ok(value) => Ok(value),
            Err(ConfigSettingError::ConfigNotSet { .. }) => Ok(DEFAULT_ROOT_CERT_PATH.into()),
            Err(other) => Err(other),
        }
    }
}

impl TEdgeConfig {
    /// Parse the configuration file at the provided `path` and create a `TEdgeConfig` out of it
    ///
    /// #Arguments
    ///
    /// * `path` - Path to a thin edge configuration TOML file
    ///
    pub(crate) fn from_file(path: &Path) -> Result<TEdgeConfig, ConfigError> {
        Ok(Self {
            file: TomlConfigFile::from_file_or_default(path.into())?,
        })
    }

    pub fn persist(&mut self) -> Result<(), ConfigError> {
        Ok(self.file.persist()?)
    }

    pub fn update_if_not_set<T: ConfigSetting + Copy>(
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
}
