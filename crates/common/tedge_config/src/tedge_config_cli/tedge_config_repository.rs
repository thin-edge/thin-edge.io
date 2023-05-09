use crate::*;
use camino::Utf8Path;
use serde::Serialize;
use std::fs;
use tedge_utils::fs::atomically_write_file_sync;

use super::figment::ConfigSources;
use super::figment::FileAndEnvironment;
use super::figment::FileOnly;
use super::figment::UnusedValueWarnings;
use super::new;

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
        let config =
            self.make_tedge_config(self.load_dto::<FileAndEnvironment>(self.toml_path())?)?;
        Ok(config)
    }

    fn update_toml(
        &self,
        update: &impl Fn(&mut TEdgeConfig) -> ConfigSettingResult<()>,
    ) -> Result<(), Self::Error> {
        let mut config = self.read_file_or_default::<FileOnly>(self.toml_path())?;
        update(&mut config)?;

        self.store(&config.data)
    }
}

impl TEdgeConfigRepository {
    fn toml_path(&self) -> &Utf8Path {
        self.config_location.tedge_config_file_path()
    }

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

    pub fn load_new(&self) -> Result<new::TEdgeConfig, TEdgeConfigError> {
        let dto = self.load_dto::<FileAndEnvironment>(self.toml_path())?;
        Ok(new::TEdgeConfig::from_dto(&dto, &self.config_location))
    }

    fn load_dto<Sources: ConfigSources>(
        &self,
        path: &Utf8Path,
    ) -> Result<new::TEdgeConfigDto, TEdgeConfigError> {
        let (mut dto, mut warnings): (new::TEdgeConfigDto, UnusedValueWarnings) =
            super::figment::extract_data::<_, Sources>(path)?;

        if let Some(migrations) = dto.config.version.unwrap_or_default().migrations() {
            'migrate_toml: {
                tracing::info!("Migrating tedge.toml configuration to version 2");
                let Ok(config) = std::fs::read_to_string(self.toml_path()) else { break 'migrate_toml };

                let toml = toml::de::from_str(&config)?;
                let migrated_toml = migrations
                    .into_iter()
                    .fold(toml, |toml, migration| migration.apply_to(toml));

                self.store(&migrated_toml)?;

                // Reload DTO to get the settings in the right place
                (dto, warnings) = super::figment::extract_data::<_, Sources>(self.toml_path())?;
            }
        }

        warnings.emit();

        Ok(dto)
    }

    pub fn get_config_location(&self) -> &TEdgeConfigLocation {
        &self.config_location
    }

    fn read_file_or_default<Sources: ConfigSources>(
        &self,
        path: &Utf8Path,
    ) -> Result<TEdgeConfig, TEdgeConfigError> {
        let dto = self.load_dto::<Sources>(path)?;

        self.make_tedge_config(dto)
    }

    fn make_tedge_config(
        &self,
        data: new::TEdgeConfigDto,
    ) -> Result<TEdgeConfig, TEdgeConfigError> {
        Ok(TEdgeConfig {
            data,
            config_defaults: self.config_defaults.clone(),
        })
    }

    // TODO: Explicitly set the file permissions in this function and file ownership!
    fn store<S: Serialize>(&self, config: &S) -> Result<(), TEdgeConfigError> {
        let toml = toml::to_string_pretty(&config)?;

        // Create `$HOME/.tedge` or `/etc/tedge` directory in case it does not exist yet
        if !self.config_location.tedge_config_root_path.exists() {
            fs::create_dir(self.config_location.tedge_config_root_path())?;
        }

        atomically_write_file_sync(self.toml_path(), toml.as_bytes())?;
        Ok(())
    }
}
