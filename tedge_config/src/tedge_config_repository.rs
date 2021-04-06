use crate::*;
use std::io::Write;
use std::path::PathBuf;
use tempfile::NamedTempFile;

const TEDGE_DEFAULT_LOCATION: &str = "/etc/tedge";
const TEDGE_CONFIG_FILE: &str = "tedge.toml";
const DEVICE_KEY_FILE: &str = "tedge-private-key.pem";
const DEVICE_CERT_FILE: &str = "tedge-certificate.pem";

/// TEdgeConfigRepository is resposible for loading and storing TEdgeConfig entities.
///
pub struct TEdgeConfigRepository {
    tedge_home: PathBuf,
}

pub trait ConfigRepository<T> {
    type Error;
    fn load(&self) -> Result<T, Self::Error>;
    fn store(&self, config: T) -> Result<(), Self::Error>;
}

impl ConfigRepository<TEdgeConfig> for TEdgeConfigRepository {
    type Error = TEdgeConfigError;

    fn load(&self) -> Result<TEdgeConfig, TEdgeConfigError> {
        let config = self.read_file_or_default(self.config_file_name().into())?;
        Ok(self.fixup_config(config)?)
    }

    // XXX: Explicitly set the file permissions in this function and file ownership!
    fn store(&self, config: TEdgeConfig) -> Result<(), TEdgeConfigError> {
        let toml = toml::to_string_pretty(&config.data)?;
        let mut file = NamedTempFile::new()?;
        file.write_all(toml.as_bytes())?;
        match file.persist(self.config_file_name()) {
            Ok(_) => Ok(()),
            Err(err) => Err(err.error.into()),
        }
    }
}

impl TEdgeConfigRepository {
    pub fn from_default_location() -> Result<Self, TEdgeConfigError> {
        Ok(Self::from_dir(TEDGE_DEFAULT_LOCATION))
    }

    pub fn from_dir(tedge_home: impl Into<PathBuf>) -> Self {
        Self {
            tedge_home: tedge_home.into(),
        }
    }

    fn config_file_name(&self) -> PathBuf {
        self.tedge_home.join(TEDGE_CONFIG_FILE)
    }

    fn fixup_config(&self, mut config: TEdgeConfig) -> Result<TEdgeConfig, TEdgeConfigError> {
        config.update_if_not_set(DeviceKeyPathSetting, self.default_device_key_path()?)?;
        config.update_if_not_set(DeviceCertPathSetting, self.default_device_cert_path()?)?;
        Ok(config)
    }

    /// Parse the configuration file at the provided `path` and create a `TEdgeConfig` out of it
    ///
    /// #Arguments
    ///
    /// * `path` - Path to a thin edge configuration TOML file
    ///
    fn read_file(&self, path: PathBuf) -> Result<TEdgeConfig, TEdgeConfigError> {
        match std::fs::read(&path) {
            Ok(bytes) => {
                let data = toml::from_slice::<TEdgeConfigDto>(bytes.as_slice())?;
                Ok(TEdgeConfig { data })
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Err(TEdgeConfigError::ConfigFileNotFound(path))
            }
            Err(err) => Err(TEdgeConfigError::IOError(err)),
        }
    }

    fn read_file_or_default(&self, path: PathBuf) -> Result<TEdgeConfig, TEdgeConfigError> {
        match self.read_file(path.clone()) {
            Ok(file) => Ok(file),
            Err(TEdgeConfigError::ConfigFileNotFound(..)) => Ok(TEdgeConfig {
                data: TEdgeConfigDto::default(),
            }),
            Err(err) => Err(err),
        }
    }

    fn default_device_key_path(&self) -> Result<String, TEdgeConfigError> {
        self.path_in_cert_directory(DEVICE_KEY_FILE)
    }

    fn default_device_cert_path(&self) -> Result<String, TEdgeConfigError> {
        self.path_in_cert_directory(DEVICE_CERT_FILE)
    }

    fn path_in_cert_directory(&self, file_name: &str) -> Result<String, TEdgeConfigError> {
        self.tedge_home
            .join(file_name)
            .to_str()
            .map(|s| s.into())
            .ok_or(TEdgeConfigError::InvalidCharacterInHomeDirectoryPath)
    }
}
