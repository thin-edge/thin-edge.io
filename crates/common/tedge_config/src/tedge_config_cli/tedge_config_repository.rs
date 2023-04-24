use crate::*;
use std::fs;
use std::path::PathBuf;
use tedge_utils::fs::atomically_write_file_sync;

use super::figment::ConfigSources;
use super::figment::FileAndEnvironment;
use super::figment::FileOnly;

/// TEdgeConfigRepository is responsible for loading and storing TEdgeConfig entities.
#[derive(Debug, Clone)]
pub struct TEdgeConfigRepository {
    config_location: TEdgeConfigLocation,
    config_defaults: TEdgeConfigDefaults,
}

pub trait ConfigRepository<T> {
    type Error;
    fn load(&self) -> Result<T, Self::Error>;
    fn update_toml(
        &self,
        update: &impl Fn(&mut T) -> ConfigSettingResult<()>,
    ) -> Result<(), Self::Error>;
}

impl ConfigRepository<TEdgeConfig> for TEdgeConfigRepository {
    type Error = TEdgeConfigError;

    fn load(&self) -> Result<TEdgeConfig, TEdgeConfigError> {
        let config = self.read_file_or_default::<FileAndEnvironment>(
            self.config_location.tedge_config_file_path().into(),
        )?;
        Ok(config)
    }

    fn update_toml(
        &self,
        update: &impl Fn(&mut TEdgeConfig) -> ConfigSettingResult<()>,
    ) -> Result<(), Self::Error> {
        let mut config = self.read_file_or_default::<FileOnly>(
            self.config_location.tedge_config_file_path().into(),
        )?;
        update(&mut config)?;

        self.store(&config)
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

    fn read_file_or_default<Sources: ConfigSources>(
        &self,
        path: PathBuf,
    ) -> Result<TEdgeConfig, TEdgeConfigError> {
        let data: TEdgeConfigDto = super::figment::extract_data::<_, Sources>(path)?;

        self.make_tedge_config(data)
    }

    fn make_tedge_config(&self, data: TEdgeConfigDto) -> Result<TEdgeConfig, TEdgeConfigError> {
        Ok(TEdgeConfig {
            data,
            config_defaults: self.config_defaults.clone(),
        })
    }

    // TODO: Explicitly set the file permissions in this function and file ownership!
    fn store(&self, config: &TEdgeConfig) -> Result<(), TEdgeConfigError> {
        let toml = toml::to_string_pretty(&config.data)?;

        // Create `$HOME/.tedge` or `/etc/tedge` directory in case it does not exist yet
        if !self.config_location.tedge_config_root_path.exists() {
            fs::create_dir(self.config_location.tedge_config_root_path())?;
        }

        atomically_write_file_sync(
            self.config_location.tedge_config_file_path(),
            toml.as_bytes(),
        )?;
        Ok(())
    }
}
