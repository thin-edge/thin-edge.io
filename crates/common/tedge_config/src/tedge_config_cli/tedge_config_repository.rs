use crate::*;
use std::fs;
use std::path::PathBuf;
use tedge_utils::fs::atomically_write_file_sync;
use tracing::warn;

/// Displays warning messages for any unknown toml **fields and or keys**.
///
/// It does **not** display values.
macro_rules! display_unknown_for {
    ($x:ident, $y:ident) => {
        let other = &$x.other;

        if !other.is_empty() {
            let mut vec = vec![];
            for key in other.keys() {
                vec.push(key);
            }
            let message = format!("Unknown field/s: {:?} in file {:?}", vec, &$y);
            warn!("{}", message);
        };
    };
    ($x:expr, $y:ident, $z:expr) => {
        let other = &$x.other;

        if !other.is_empty() {
            let mut vec = vec![];
            for key in other.keys() {
                vec.push(key);
            }

            let message = format!(
                "Unknown key/s: {:?} for field: {:?} in file {:?}",
                vec, &$z, &$y
            );
            warn!("{}", message);
        }
    };
}
//use tracing::warn;

/// TEdgeConfigRepository is responsible for loading and storing TEdgeConfig entities.
///
#[derive(Debug)]
pub struct TEdgeConfigRepository {
    config_location: TEdgeConfigLocation,
    config_defaults: TEdgeConfigDefaults,
}

pub trait ConfigRepository<T> {
    type Error;
    fn load(&self) -> Result<T, Self::Error>;
    fn store(&self, config: &T) -> Result<(), Self::Error>;
}

impl ConfigRepository<TEdgeConfig> for TEdgeConfigRepository {
    type Error = TEdgeConfigError;

    fn load(&self) -> Result<TEdgeConfig, TEdgeConfigError> {
        let config =
            self.read_file_or_default(self.config_location.tedge_config_file_path().into())?;
        Ok(config)
    }

    // TODO: Explicitly set the file permissions in this function and file ownership!
    fn store(&self, config: &TEdgeConfig) -> Result<(), TEdgeConfigError> {
        let toml = toml::to_string_pretty(&config.data)?;

        // Create `$HOME/.tedge` or `/etc/tedge` directory in case it does not exist yet
        if !self.config_location.tedge_config_root_path.exists() {
            fs::create_dir(self.config_location.tedge_config_root_path())?;
        }

        atomically_write_file_sync(
            self.config_location.temporary_tedge_config_file_path(),
            self.config_location.tedge_config_file_path(),
            toml.as_bytes(),
        )?;
        Ok(())
    }
}

impl TEdgeConfigRepository {
    pub fn new(config_location: TEdgeConfigLocation) -> Self {
        let config_defaults = TEdgeConfigDefaults::from(&config_location);
        Self::new_with_defaults(config_location, config_defaults)
    }

    pub fn new_with_defaults(
        config_location: TEdgeConfigLocation,
        config_defaults: TEdgeConfigDefaults,
    ) -> Self {
        Self {
            config_location,
            config_defaults,
        }
    }

    pub fn get_config_location(&self) -> &TEdgeConfigLocation {
        &self.config_location
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

                display_unknown_for!(data, path);
                display_unknown_for!(data.device, path, "device");
                display_unknown_for!(data.c8y, path, "c8y");
                display_unknown_for!(data.az, path, "az");
                display_unknown_for!(data.mqtt, path, "mqtt");
                display_unknown_for!(data.http, path, "http");
                display_unknown_for!(data.software, path, "software");
                display_unknown_for!(data.tmp, path, "tmp");
                display_unknown_for!(data.logs, path, "logs");
                display_unknown_for!(data.run, path, "run");

                self.make_tedge_config(data)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Err(TEdgeConfigError::ConfigFileNotFound(path))
            }
            Err(err) => Err(TEdgeConfigError::FromIo(err)),
        }
    }

    fn read_file_or_default(&self, path: PathBuf) -> Result<TEdgeConfig, TEdgeConfigError> {
        match self.read_file(path) {
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
            config_defaults: self.config_defaults.clone(),
        })
    }
}
