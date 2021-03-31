use crate::*;
use std::path::PathBuf;

const DEVICE_KEY_FILE: &str = "tedge-private-key.pem";
const DEVICE_CERT_FILE: &str = "tedge-certificate.pem";
const TEDGE_HOME_DIR: &str = ".tedge";

// XXX: Move to TEdgeConfig
pub trait AssignDefaults: Sized {
    fn assign_defaults(self) -> Result<Self, ConfigError>;
}

impl AssignDefaults for TEdgeConfigDto {
    fn assign_defaults(self) -> Result<Self, ConfigError> {
        let device_config = self.device.assign_defaults()?;

        Ok(TEdgeConfigDto {
            device: device_config,
            ..self
        })
    }
}

impl AssignDefaults for DeviceConfigDto {
    fn assign_defaults(self) -> Result<Self, ConfigError> {
        let key_path = match self.key_path {
            None => default_device_key_path()?,
            Some(val) => val,
        };

        let cert_path = match self.cert_path {
            None => default_device_cert_path()?,
            Some(val) => val,
        };

        Ok(DeviceConfigDto {
            key_path: Some(key_path),
            cert_path: Some(cert_path),
            ..self
        })
    }
}

pub(crate) fn default_device_key_path() -> Result<String, ConfigError> {
    path_in_cert_directory(DEVICE_KEY_FILE)
}

pub(crate) fn default_device_cert_path() -> Result<String, ConfigError> {
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
