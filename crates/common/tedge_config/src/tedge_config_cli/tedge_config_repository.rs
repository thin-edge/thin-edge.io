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
    // TODO don't make this part of the trait. Do we even need the trait?
    fn update_toml(
        &self,
        update: impl FnOnce(&mut T) -> ConfigSettingResult<()>,
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
        update: impl FnOnce(&mut TEdgeConfig) -> ConfigSettingResult<()>,
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
            self.config_location.temporary_tedge_config_file_path(),
            self.config_location.tedge_config_file_path(),
            toml.as_bytes(),
        )?;
        Ok(())
    }

    /// Deletes a key from the current configuration
    ///
    /// # Errors
    /// This returns an error if persisting the updated configuration in the TOML file
    /// fails, but unsetting the value in the underlying configuration is guaranteed to
    /// succeed as [WritableKey] only includes valid keys.
    ///
    /// ```
    /// # use tedge_test_utils::fs::TempTedgeDir;
    /// use tedge_config::*;
    ///
    /// let location = TEdgeConfigLocation::default();
    /// # let tmp_dir = TempTedgeDir::new();
    /// # let location = TEdgeConfigLocation::from_custom_root(tmp_dir.path());
    /// let repo = TEdgeConfigRepository::new(location);
    ///
    /// assert!(repo.update(ConfigurationUpdate::C8yUrl(ConnectUrl::try_from("test.cumulocity.com")?)).is_ok());
    ///
    /// assert!(repo.unset(WritableKey::C8yUrl).is_ok());
    ///
    /// // Unsetting a value that isn't set will succeed
    /// assert!(repo.unset(WritableKey::C8yUrl).is_ok());
    /// # Ok::<(), TEdgeConfigError>(())
    /// ```
    pub fn unset(&self, key: WritableKey) -> Result<(), TEdgeConfigError> {
        self.update_toml(|config| {
            typed_unset(&mut config.data, key);
            Ok(())
        })
    }

    /// Updates a value for a particular key in the current configuration
    ///
    /// # Errors
    /// This returns an error if persisting the updated configuration in the TOML file
    /// fails, but the update to the underlying configuration is guaranteed to succeed
    /// as [ConfigurationUpdate] only accepts valid configuration updates.
    ///
    /// ```
    /// # use tedge_test_utils::fs::TempTedgeDir;
    /// use tedge_config::*;
    ///
    /// let location = TEdgeConfigLocation::default();
    /// # let tmp_dir = TempTedgeDir::new();
    /// # let location = TEdgeConfigLocation::from_custom_root(tmp_dir.path());
    /// let repo = TEdgeConfigRepository::new(location);
    ///
    /// repo.update(ConfigurationUpdate::MqttPort(2345))?;
    ///
    /// let config = repo.load_new()?;
    /// assert_eq!(config.mqtt_port(), 2345);
    ///
    /// # Ok::<_, TEdgeConfigError>(())
    /// ```
    pub fn update(&self, update: ConfigurationUpdate) -> Result<(), TEdgeConfigError> {
        self.update_toml(|config| {
            typed_update(&mut config.data, update);
            Ok(())
        })
    }

    /// Updates the value for a particular key in the current configuration
    ///
    /// This takes value as `&str` to accept arbitrary user input. This is intended for
    /// the `tedge config set` subcommand.
    ///
    /// # Errors
    /// This returns an error if the value cannot be deserialised to the appropriate type
    /// for the provided key.
    ///
    /// Like [TEdgeConfigRepository::update], this also returns an error if persisting the
    /// updated configuration in the TOML file fails.
    ///
    /// ```
    /// # use tedge_test_utils::fs::TempTedgeDir;
    /// use tedge_config::*;
    ///
    /// let location = TEdgeConfigLocation::default();
    /// # let tmp_dir = TempTedgeDir::new();
    /// # let location = TEdgeConfigLocation::from_custom_root(tmp_dir.path());
    /// let repo = TEdgeConfigRepository::new(location);
    ///
    /// assert!(repo.update_string(WritableKey::MqttPort, "not a port").is_err());
    /// assert!(repo.update_string(WritableKey::C8yUrl, "test.cumulocity.com").is_ok());
    ///
    /// let config = repo.load_new()?;
    /// assert_eq!(config.mqtt_port(), 1883);
    /// assert_eq!(config.c8y_url()?, ConnectUrl::try_from("test.cumulocity.com")?);
    ///
    /// # Ok::<_, TEdgeConfigError>(())
    /// ```
    pub fn update_string(&self, key: WritableKey, value: &str) -> Result<(), TEdgeConfigError> {
        self.update_toml(|config| {
            config.data = TEdgeConfigUpdate::new(key, value).apply_to(&config.data)?;
            Ok(())
        })
    }

    pub fn load_new(&self) -> Result<super::new_tedge_config::NewTEdgeConfig, TEdgeConfigError> {
        Ok(super::new_tedge_config::NewTEdgeConfig::new(
            self.load()?,
            &self.config_location,
        ))
    }
}
