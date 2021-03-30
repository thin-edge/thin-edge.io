use crate::ConfigError;
use serde::de::DeserializeOwned;
use std::path::PathBuf;

pub struct ConfigFileManager<T> {
    path: PathBuf,
    config: T,
    dirty: bool,
}

impl<T> ConfigFileManager<T>
where
    T: DeserializeOwned,
{
    pub fn load_toml(path: PathBuf) -> Result<Self, ConfigError> {
        match std::fs::read(&path) {
            Ok(data) => {
                let config = toml::from_slice::<T>(data.as_slice())?;
                Ok(Self {
                    path,
                    config,
                    dirty: false,
                })
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Err(ConfigError::ConfigFileNotFound(path))
            }
            Err(err) => Err(ConfigError::IOError(err)),
        }
    }
}
