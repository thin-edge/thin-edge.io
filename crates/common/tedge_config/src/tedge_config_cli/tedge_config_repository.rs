use crate::new::TomlMigrationStep;
use crate::*;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use tedge_utils::fs::atomically_write_file_sync;
use toml::Table;

use super::figment::ConfigSources;
use super::figment::FileAndEnvironment;
use super::figment::FileOnly;
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
        let config = self.make_tedge_config(self.load_dto()?)?;
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

        self.store(&config.data)
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

    pub fn load_new(&self) -> Result<new::TEdgeConfig, TEdgeConfigError> {
        let dto = self.load_dto()?;
        Ok(new::TEdgeConfig::from_dto(&dto, &self.config_location))
    }

    fn load_dto(&self) -> Result<new::TEdgeConfigDto, TEdgeConfigError> {
        let (mut dto, mut warnings) = super::figment::extract_data::<
            new::TEdgeConfigDto,
            FileAndEnvironment,
        >(self.config_location.tedge_config_file_path())?;

        if let Some(migrations) = dto.config.version.unwrap_or_default().migrations() {
            tracing::info!("Migrating tedge.toml configuration to version 2");
            let config = std::fs::read_to_string(self.config_location.tedge_config_file_path())?;
            let mut toml: toml::Value = toml::de::from_str(&config)?;
            'migration: for migration in migrations {
                match migration {
                    TomlMigrationStep::MoveKey { original, target } => {
                        let mut doc = &mut toml;
                        let (tables, field) = original.rsplit_once('.').unwrap();
                        for key in tables.split('.') {
                            if doc.as_table().map(|table| table.contains_key(key)) == Some(true) {
                                doc = &mut doc[key];
                            } else {
                                continue 'migration;
                            }
                        }
                        let value = doc.as_table_mut().unwrap().remove(field);

                        if let Some(value) = value {
                            let mut doc = &mut toml;
                            let (tables, field) = target.rsplit_once('.').unwrap();
                            for key in tables.split('.') {
                                let table = doc.as_table_mut().unwrap();
                                if !table.contains_key(key) {
                                    table.insert(key.to_owned(), toml::Value::Table(Table::new()));
                                }
                                doc = &mut doc[key];
                            }
                            let table = doc.as_table_mut().unwrap();
                            // TODO if this returns Some, something is going wrong? Maybe this could be an error, or maybe it doesn't matter
                            table.insert(field.to_owned(), value);
                        }
                    }
                    TomlMigrationStep::UpdateFieldValue { key, value } => {
                        let mut doc = &mut toml;
                        let (tables, field) = key.rsplit_once('.').unwrap();
                        for key in tables.split('.') {
                            let table = doc.as_table_mut().unwrap();
                            if !table.contains_key(key) {
                                table.insert(key.to_owned(), toml::Value::Table(Table::new()));
                            }
                            doc = &mut doc[key];
                        }
                        let table = doc.as_table_mut().unwrap();
                        // TODO if this returns Some, something is going wrong? Maybe this could be an error, or maybe it doesn't matter
                        table.insert(field.to_owned(), value);
                    }
                }
            }

            self.store(&toml)?;

            // Reload DTO to get the settings in the right place
            (dto, warnings) = super::figment::extract_data::<_, FileAndEnvironment>(
                self.config_location.tedge_config_file_path(),
            )?;
        }

        warnings.emit();

        Ok(dto)
    }

    pub fn get_config_location(&self) -> &TEdgeConfigLocation {
        &self.config_location
    }

    fn read_file_or_default<Sources: ConfigSources>(
        &self,
        path: PathBuf,
    ) -> Result<TEdgeConfig, TEdgeConfigError> {
        let (data, warnings) = super::figment::extract_data::<new::TEdgeConfigDto, Sources>(path)?;

        warnings.emit();

        self.make_tedge_config(data)
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

        atomically_write_file_sync(
            self.config_location.tedge_config_file_path(),
            toml.as_bytes(),
        )?;
        Ok(())
    }
}
