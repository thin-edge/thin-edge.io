use crate::*;
use std::path::PathBuf;
use std::{fs, marker::PhantomData};
use tedge_utils::fs::atomically_write_file_sync;

use super::figment::{ConfigSources, FileAndEnvironment, FileOnly};

/// TEdgeConfigRepository is responsible for loading and storing TEdgeConfig entities.
#[derive(Debug, Clone)]
pub struct TEdgeConfigRepository<Sources: ConfigSources> {
    config_location: TEdgeConfigLocation,
    config_defaults: TEdgeConfigDefaults,
    _sources: PhantomData<Sources>,
}

/// A repository to read the tedge config from both tedge.toml and environment variables
pub type ReadOnlyTEdgeConfigRepository = TEdgeConfigRepository<FileAndEnvironment>;

/// A repository intended specifically for updating tedge.toml
///
/// This does not read environment variables so as to stop an environment variable
/// being persisted to tedge.toml (e.g. when calling `tedge config set ...`)
///
/// To create this type, call [ReadOnlyTEdgeConfigRepository::without_environment]
pub type ReadWriteTEdgeConfigRepository = TEdgeConfigRepository<FileOnly>;

pub trait ConfigRepository<T> {
    type Error;
    fn load(&self) -> Result<T, Self::Error>;
}

pub trait PersistentConfigRepository<T>: ConfigRepository<T> {
    fn store(&self, config: &T) -> Result<(), Self::Error>;
}

impl<Sources: ConfigSources> ConfigRepository<TEdgeConfig> for TEdgeConfigRepository<Sources> {
    type Error = TEdgeConfigError;

    fn load(&self) -> Result<TEdgeConfig, TEdgeConfigError> {
        let config =
            self.read_file_or_default(self.config_location.tedge_config_file_path().into())?;
        Ok(config)
    }
}

impl PersistentConfigRepository<TEdgeConfig> for ReadWriteTEdgeConfigRepository {
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

impl ReadOnlyTEdgeConfigRepository {
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
            _sources: <_>::default(),
        }
    }

    #[must_use]
    /// Enable writing to this repository by preventing environment variables from being read for configuration
    pub fn skip_environment_variables(&self) -> ReadWriteTEdgeConfigRepository {
        TEdgeConfigRepository {
            config_location: self.config_location.clone(),
            config_defaults: self.config_defaults.clone(),
            _sources: <_>::default(),
        }
    }
}

impl<Sources: ConfigSources> TEdgeConfigRepository<Sources> {
    pub fn get_config_location(&self) -> &TEdgeConfigLocation {
        &self.config_location
    }

    fn read_file_or_default(&self, path: PathBuf) -> Result<TEdgeConfig, TEdgeConfigError> {
        let data: TEdgeConfigDto = super::figment::extract_data::<_, Sources>(path)?;

        self.make_tedge_config(data)
    }

    fn make_tedge_config(&self, data: TEdgeConfigDto) -> Result<TEdgeConfig, TEdgeConfigError> {
        Ok(TEdgeConfig {
            data,
            config_defaults: self.config_defaults.clone(),
        })
    }
}
