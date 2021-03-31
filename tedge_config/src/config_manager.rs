use crate::*;
use std::path::{Path, PathBuf};

const TEDGE_CONFIG_FILE: &str = "tedge.toml";
const DEVICE_KEY_FILE: &str = "tedge-private-key.pem";
const DEVICE_CERT_FILE: &str = "tedge-certificate.pem";

pub struct TEdgeConfigManager {
    tedge_home: PathBuf,
}

impl TEdgeConfigManager {
    // XXX: Remove
    pub fn try_default() -> Result<Self, ConfigError> {
        Ok(Self::new(
            home_dir()?
                .join(crate::TEDGE_HOME_DIR)
                .join(TEDGE_CONFIG_FILE),
        ))
    }

    pub fn new(tedge_home: PathBuf) -> Self {
        Self { tedge_home }
    }

    /// Parse the configuration file at `tedge_home` + `/.tedge/tedge.toml` and create a `TEdgeConfig` out of it
    /// The retrieved configuration will have default values applied to any unconfigured field
    /// for which a default value is available.
    pub fn from_default_config(&self) -> Result<TEdgeConfig, ConfigError> {
        self.from_custom_config(self.tedge_home.join(TEDGE_CONFIG_FILE).as_path())
    }

    pub fn from_custom_config(&self, path: &Path) -> Result<TEdgeConfig, ConfigError> {
        let mut config = TEdgeConfig::from_file(path)?;
        config.update_if_not_set(DeviceKeyPathSetting, self.default_device_key_path()?)?;
        config.update_if_not_set(DeviceCertPathSetting, self.default_device_cert_path()?)?;
        Ok(config)
    }

    fn default_device_key_path(&self) -> Result<String, ConfigError> {
        self.path_in_cert_directory(DEVICE_KEY_FILE)
    }

    fn default_device_cert_path(&self) -> Result<String, ConfigError> {
        self.path_in_cert_directory(DEVICE_CERT_FILE)
    }

    fn path_in_cert_directory(&self, file_name: &str) -> Result<String, ConfigError> {
        self.tedge_home
            .join(file_name)
            .to_str()
            .map(|s| s.into())
            .ok_or(ConfigError::InvalidCharacterInHomeDirectoryPath)
    }
}

fn home_dir() -> Result<PathBuf, ConfigError> {
    // The usage of this deprecated method is temporary as this whole function will be replaced with the util function being added in CIT-137.
    #![allow(deprecated)]
    std::env::home_dir().ok_or(ConfigError::HomeDirectoryNotFound)
}
