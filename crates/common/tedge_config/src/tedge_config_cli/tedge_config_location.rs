use std::fs;
use std::path::Path;

use crate::tedge_config_cli::figment::ConfigSources;
use crate::tedge_config_cli::figment::FileAndEnvironment;
use crate::tedge_config_cli::figment::FileOnly;
use crate::tedge_config_cli::figment::UnusedValueWarnings;
use crate::ConfigSettingResult;
use crate::TEdgeConfig;
use crate::TEdgeConfigDto;
use crate::TEdgeConfigError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use serde::Serialize;
use tedge_utils::fs::atomically_write_file_sync;
use tracing::debug;

pub const DEFAULT_TEDGE_CONFIG_PATH: &str = "/etc/tedge";
const TEDGE_CONFIG_FILE: &str = "tedge.toml";
/// Information about where `tedge.toml` is located.
///
/// Broadly speaking, we distinguish two different locations:
///
/// - System-wide locations under `/etc/tedge` or `/usr/local/etc/tedge`.
/// - User-local locations under `$HOME/.tedge`
///
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TEdgeConfigLocation {
    /// Root directory where `tedge.toml` and other tedge related configuration files are located.
    pub tedge_config_root_path: Utf8PathBuf,

    /// Full path to the `tedge.toml` file.
    pub tedge_config_file_path: Utf8PathBuf,
}

impl Default for TEdgeConfigLocation {
    /// `tedge.toml` is located in `/etc/tedge`.
    fn default() -> Self {
        Self::from_custom_root(DEFAULT_TEDGE_CONFIG_PATH)
    }
}

impl TEdgeConfigLocation {
    pub fn from_custom_root(tedge_config_root_path: impl AsRef<Path>) -> Self {
        Self {
            tedge_config_root_path: Utf8Path::from_path(tedge_config_root_path.as_ref())
                .unwrap()
                .to_owned(),
            tedge_config_file_path: Utf8Path::from_path(tedge_config_root_path.as_ref())
                .unwrap()
                .join(TEDGE_CONFIG_FILE),
        }
    }

    pub fn tedge_config_root_path(&self) -> &Utf8Path {
        &self.tedge_config_root_path
    }

    pub fn tedge_config_file_path(&self) -> &Utf8Path {
        &self.tedge_config_file_path
    }

    pub fn update_toml(
        &self,
        update: &impl Fn(&mut TEdgeConfigDto) -> ConfigSettingResult<()>,
    ) -> Result<(), TEdgeConfigError> {
        let mut config = self.load_dto::<FileOnly>(self.toml_path())?;
        update(&mut config)?;

        self.store(&config)
    }

    fn toml_path(&self) -> &Utf8Path {
        self.tedge_config_file_path()
    }

    pub fn load(&self) -> Result<TEdgeConfig, TEdgeConfigError> {
        let dto = self.load_dto::<FileAndEnvironment>(self.toml_path())?;
        debug!(
            "Loading configuration from {:?}",
            self.tedge_config_file_path
        );
        Ok(TEdgeConfig::from_dto(&dto, self))
    }

    fn load_dto<Sources: ConfigSources>(
        &self,
        path: &Utf8Path,
    ) -> Result<TEdgeConfigDto, TEdgeConfigError> {
        let (dto, warnings) = self.load_dto_with_warnings::<Sources>(path)?;

        warnings.emit();

        Ok(dto)
    }

    #[cfg(feature = "test")]
    /// A test only method designed for injecting configuration into tests
    ///
    /// ```
    /// use tedge_config::TEdgeConfigLocation;
    /// let config = TEdgeConfigLocation::load_toml_str("service.ty = \"service\"");
    ///
    /// assert_eq!(&config.service.ty, "service");
    /// // Defaults are preserved
    /// assert_eq!(config.sudo.enable, true);
    /// ```
    pub fn load_toml_str(toml: &str) -> TEdgeConfig {
        let dto = super::figment::extract_from_toml_str(toml).unwrap();
        TEdgeConfig::from_dto(&dto, &TEdgeConfigLocation::default())
    }

    fn load_dto_with_warnings<Sources: ConfigSources>(
        &self,
        path: &Utf8Path,
    ) -> Result<(TEdgeConfigDto, UnusedValueWarnings), TEdgeConfigError> {
        let (mut dto, mut warnings): (TEdgeConfigDto, _) =
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

    // TODO: Explicitly set the file permissions in this function and file ownership!
    fn store<S: Serialize>(&self, config: &S) -> Result<(), TEdgeConfigError> {
        let toml = toml::to_string_pretty(&config)?;

        // Create `$HOME/.tedge` or `/etc/tedge` directory in case it does not exist yet
        if !self.tedge_config_root_path.exists() {
            fs::create_dir(self.tedge_config_root_path())?;
        }

        atomically_write_file_sync(self.toml_path(), toml.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tedge_test_utils::fs::TempTedgeDir;

    use crate::TEdgeConfigReader;

    use super::*;

    #[test]
    fn test_from_custom_root() {
        let config_location = TEdgeConfigLocation::from_custom_root("/opt/etc/tedge");
        assert_eq!(
            config_location.tedge_config_root_path,
            Utf8Path::new("/opt/etc/tedge")
        );
        assert_eq!(
            config_location.tedge_config_file_path,
            Utf8Path::new("/opt/etc/tedge/tedge.toml")
        );
    }

    #[test]
    fn test_from_default_system_location() {
        let config_location = TEdgeConfigLocation::default();
        assert_eq!(
            config_location.tedge_config_root_path,
            Utf8Path::new("/etc/tedge")
        );
        assert_eq!(
            config_location.tedge_config_file_path,
            Utf8Path::new("/etc/tedge/tedge.toml")
        );
    }

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
        let (dto, warnings) = config_location
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
