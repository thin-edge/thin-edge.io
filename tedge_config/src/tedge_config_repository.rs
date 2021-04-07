use crate::*;
use std::io::Write;
use std::path::PathBuf;
use tempfile::NamedTempFile;

/// TEdgeConfigRepository is resposible for loading and storing TEdgeConfig entities.
///
pub struct TEdgeConfigRepository {
    config_location: TEdgeConfigLocation,
}

pub trait ConfigRepository<T> {
    type Error;
    fn load(&self) -> Result<T, Self::Error>;
    fn store(&self, config: T) -> Result<(), Self::Error>;
}

impl ConfigRepository<TEdgeConfig> for TEdgeConfigRepository {
    type Error = TEdgeConfigError;

    fn load(&self) -> Result<TEdgeConfig, TEdgeConfigError> {
        let config = self.read_file_or_default(self.config_location.tedge_config_path.clone())?;
        Ok(config)
    }

    // XXX: Explicitly set the file permissions in this function and file ownership!
    fn store(&self, config: TEdgeConfig) -> Result<(), TEdgeConfigError> {
        let toml = toml::to_string_pretty(&config.data)?;
        let mut file = NamedTempFile::new()?;
        file.write_all(toml.as_bytes())?;
        match file.persist(self.config_location.tedge_config_path.clone()) {
            Ok(_) => Ok(()),
            Err(err) => Err(err.error.into()),
        }
    }
}

impl TEdgeConfigRepository {
    pub fn new(config_location: TEdgeConfigLocation) -> Self {
        Self { config_location }
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
                self.make_tedge_config(data)
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
            Err(TEdgeConfigError::ConfigFileNotFound(..)) => {
                self.make_tedge_config(TEdgeConfigDto::default())
            }
            Err(err) => Err(err),
        }
    }

    fn make_tedge_config(&self, data: TEdgeConfigDto) -> Result<TEdgeConfig, TEdgeConfigError> {
        Ok(TEdgeConfig {
            data,
            config_location: self.config_location.clone(),
        })
    }
}
