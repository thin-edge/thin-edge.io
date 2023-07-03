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
    pub fn update_toml_new(
        &self,
        update: &impl Fn(&mut new::TEdgeConfigDto) -> ConfigSettingResult<()>,
    ) -> Result<(), TEdgeConfigError> {
        let mut config = self.load_dto::<FileOnly>(self.toml_path())?;
        update(&mut config)?;

        self.store(&config)
    }

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
        let (dto, warnings) = self.load_dto_with_warnings::<Sources>(path)?;

        warnings.emit();

        Ok(dto)
    }

    fn load_dto_with_warnings<Sources: ConfigSources>(
        &self,
        path: &Utf8Path,
    ) -> Result<(new::TEdgeConfigDto, UnusedValueWarnings), TEdgeConfigError> {
        let (mut dto, mut warnings): (new::TEdgeConfigDto, _) =
            super::figment::extract_data::<_, Sources>(path)?;

        if let Some(migrations) = dto.config.version.unwrap_or_default().migrations() {
            'migrate_toml: {
                let Ok(config) = std::fs::read_to_string(self.toml_path()) else {
                    break 'migrate_toml;
                };

                tracing::info!("Migrating tedge.toml configuration to version 2");

                let toml = toml::de::from_str(&config)?;
                let migrated_toml = migrations
                    .into_iter()
                    .fold(toml, |toml, migration| migration.apply_to(toml));

                self.store(&migrated_toml)?;

                // Reload DTO to get the settings in the right place
                (dto, warnings) = super::figment::extract_data::<_, Sources>(self.toml_path())?;
            }
        }

        Ok((dto, warnings))
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

#[cfg(test)]
mod tests {
    use tedge_test_utils::fs::TempTedgeDir;

    use crate::new::TEdgeConfigReader;

    use super::*;

    #[test]
    fn old_toml_can_be_read_in_its_entirety() {
        let toml = r#"[device]
key_path = "/tedge/device-key.pem"
cert_path = "/tedge/device-cert.pem"
type = "a-device"

[c8y]
url = "something.latest.stage.c8y.io"
root_cert_path = "/c8y/root-cert.pem"
smartrest_templates = [
    "id1",
    "id2",
]

[az]
url = "something.azure.com"
root_cert_path = "/az/root-cert.pem"
mapper_timestamp = true

[aws]
url = "something.amazonaws.com"
root_cert_path = "/aws/root-cert.pem"
mapper_timestamp = false

[mqtt]
bind_address = "192.168.0.1"
port = 1886
client_host = "192.168.0.1"
client_port = 1885
client_ca_file = "/mqtt/ca.crt"
client_ca_path = "/mqtt/ca"
external_port = 8765
external_bind_address = "0.0.0.0"
external_bind_interface = "wlan0"
external_capath = "/mqtt/external/ca.pem"
external_certfile = "/mqtt/external/cert.pem"
external_keyfile = "/mqtt/external/key.pem"

[mqtt.client_auth]
cert_file = "/mqtt/auth/cert.pem"
key_file = "/mqtt/auth/key.pem"

[http]
port = 1234

[software]
default_plugin_type = "my-plugin"

[tmp]
path = "/tmp-path"

[logs]
path = "/logs-path"

[run]
path = "/run-path"
lock_files = false

[data]
path = "/data-path"

[firmware]
child_update_timeout = 3429

[service]
type = "a-service-type""#;
        let (_tempdir, config_location) = create_temp_tedge_config(toml).unwrap();
        let toml_path = config_location.tedge_config_file_path();
        let (dto, warnings) = TEdgeConfigRepository::new(config_location.clone())
            .load_dto_with_warnings::<FileOnly>(toml_path)
            .unwrap();

        // Figment will warn us if we're not using a field. If we've migrated
        // everything successfully, then no warnings will be emitted
        assert_eq!(warnings, UnusedValueWarnings::default());

        let reader = TEdgeConfigReader::from_dto(&dto, &config_location);

        assert_eq!(reader.device.cert_path, "/tedge/device-cert.pem");
        assert_eq!(reader.device.key_path, "/tedge/device-key.pem");
        assert_eq!(reader.device.ty, "a-device");
        assert_eq!(u16::from(reader.mqtt.bind.port), 1886);
        assert_eq!(u16::from(reader.mqtt.client.port), 1885);
    }

    fn create_temp_tedge_config(
        content: &str,
    ) -> std::io::Result<(TempTedgeDir, TEdgeConfigLocation)> {
        let dir = TempTedgeDir::new();
        dir.file("tedge.toml").with_raw_content(content);
        let config_location = TEdgeConfigLocation::from_custom_root(dir.path());
        Ok((dir, config_location))
    }
}
