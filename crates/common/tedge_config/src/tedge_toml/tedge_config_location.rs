use std::borrow::Cow;
use std::io::ErrorKind;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::models::CloudType;
use crate::tedge_toml::mapper_config::ExpectedCloudType;
use crate::tedge_toml::mapper_config::HasPath;
use crate::tedge_toml::mapper_config::MapperConfigPath;
use crate::tedge_toml::DtoKey;
use crate::ConfigSettingResult;
use crate::TEdgeConfig;
use crate::TEdgeConfigDto;
use crate::TEdgeConfigError;
use crate::TEdgeConfigReader;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use glob::GlobError;
use serde::Deserialize as _;
use serde::Serialize;
use std::path::PathBuf;
use strum::IntoEnumIterator as _;
use tedge_config_macros::MultiDto;
use tedge_config_macros::ProfileName;
use tedge_utils::file::change_mode;
use tedge_utils::file::change_user_and_group;
use tedge_utils::fs::atomically_write_file_async;
use tracing::debug;
use tracing::subscriber::NoSubscriber;
use tracing::warn;

use super::tedge_config;
use super::ParseKeyError;
use super::WritableKey;

const DEFAULT_TEDGE_CONFIG_PATH: &str = "/etc/tedge";
const ENV_TEDGE_CONFIG_DIR: &str = "TEDGE_CONFIG_DIR";
const TEDGE_CONFIG_FILE: &str = "tedge.toml";

/// Get the location of the configuration directory
///
/// Check if the TEDGE_CONFIG_DIR env variable is set and only
/// use the value if it is not empty, otherwise use the default
/// location, /etc/tedge
pub fn get_config_dir() -> PathBuf {
    match std::env::var(ENV_TEDGE_CONFIG_DIR) {
        Ok(s) if !s.is_empty() => PathBuf::from(s),
        _ => PathBuf::from(DEFAULT_TEDGE_CONFIG_PATH),
    }
}

/// Information about where `tedge.toml` is located.
///
/// Broadly speaking, we distinguish two different locations:
///
/// - System-wide locations under `/etc/tedge` or `/usr/local/etc/tedge`.
/// - User-local locations under `$HOME/.tedge`
///
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct TEdgeConfigLocation {
    /// Root directory where `tedge.toml` and other tedge related configuration files are located.
    tedge_config_root_path: Utf8PathBuf,

    /// Full path to the `tedge.toml` file.
    tedge_config_file_path: Utf8PathBuf,

    mapper_config_default_location: MapperConfigLocation,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum MapperConfigLocation {
    TedgeToml,
    SeparateFile,
}

impl Default for TEdgeConfigLocation {
    /// `tedge.toml` is located in `/etc/tedge`.
    fn default() -> Self {
        Self::from_custom_root(DEFAULT_TEDGE_CONFIG_PATH)
    }
}

impl TEdgeConfigLocation {
    pub(crate) fn from_custom_root(tedge_config_root_path: impl AsRef<Path>) -> Self {
        Self {
            tedge_config_root_path: Utf8Path::from_path(tedge_config_root_path.as_ref())
                .unwrap()
                .to_owned(),
            tedge_config_file_path: Utf8Path::from_path(tedge_config_root_path.as_ref())
                .unwrap()
                .join(TEDGE_CONFIG_FILE),
            mapper_config_default_location: MapperConfigLocation::TedgeToml,
        }
    }

    #[cfg(feature = "test")]
    pub(crate) fn default_to_mapper_config_dir(&mut self) {
        self.mapper_config_default_location = MapperConfigLocation::SeparateFile;
    }

    pub(crate) fn tedge_config_root_path(&self) -> &Utf8Path {
        &self.tedge_config_root_path
    }

    pub fn mappers_config_dir(&self) -> Utf8PathBuf {
        self.tedge_config_root_path.join("mappers")
    }

    pub async fn tedge_toml_contains_cloud_config(&self) -> bool {
        let toml = tokio::fs::read_to_string(self.toml_path())
            .await
            .unwrap_or_default();
        let tedge_toml: toml::Table = toml::from_str(&toml).unwrap();
        CloudType::iter().any(|key| tedge_toml.contains_key(key.as_ref()))
    }

    pub async fn tedge_toml_contains_cloud_config_for(&self, cloud_type: CloudType) -> bool {
        let toml = tokio::fs::read_to_string(self.toml_path())
            .await
            .unwrap_or_default();
        let tedge_toml: toml::Table = toml::from_str(&toml).unwrap_or_default();
        tedge_toml.contains_key(cloud_type.as_ref())
    }

    pub fn config_dir<T: ExpectedCloudType>(&self, profile: Option<&ProfileName>) -> Utf8PathBuf {
        self.config_path::<T>().dir_for(profile)
    }

    pub(crate) fn config_path<T: ExpectedCloudType>(&self) -> MapperConfigPath<'static> {
        MapperConfigPath {
            base_dir: Cow::Owned(self.mappers_config_dir()),
            cloud_type: T::expected_cloud_type(),
        }
    }

    /// Decide which configuration source to use for a given cloud and profile
    ///
    /// This function centralizes the decision logic for mapper configuration precedence:
    /// 1. New format (`mappers/[cloud]/tedge.toml` or `mappers/[cloud].[profile]/tedge.toml`) takes precedence
    /// 2. If new format exists for some profiles but not the requested one, returns NotFound
    /// 3. If the config directory is inaccessible due to a permissions error, return Error
    ///
    /// If no new format exists, we then look at the
    /// `mapper_config_default_location` field of [TEdgeConfigLocation]:
    /// 1. If this is set to [MapperConfigLocation::TedgeToml], we fall back to the root tedge.toml
    /// 2. If this is set to [MapperConfigLocation::SeparateFile] and no cloud configurations
    ///    exist in tedge.toml, we use the new mapper config format
    /// 3. If cloud configurations (for any cloud) do already exist in tedge.toml, we use
    ///    tedge.toml until the configuration is explicitly migrated
    pub async fn decide_config_source<T>(&self, profile: Option<&ProfileName>) -> ConfigDecision
    where
        T: ExpectedCloudType,
    {
        use tokio::fs::try_exists;

        let mapper_config_dir = self.mappers_config_dir();

        let config_paths = self.config_path::<T>();
        let filename = config_paths.toml_path_for(profile);
        let path = mapper_config_dir.join(&filename);

        let migrated_config_exists = tokio::task::spawn_blocking({
            let non_profiled_config = config_paths.toml_path_for(None::<&ProfileName>);
            let profiled_glob = config_paths.toml_path_for(Some("*"));
            move || {
                let non_profiled_configs =
                    glob::glob(non_profiled_config.as_str()).expect("pattern is valid");
                let profiled_configs =
                    glob::glob(profiled_glob.as_str()).expect("pattern is valid");
                let configs = non_profiled_configs
                    .into_iter()
                    .chain(profiled_configs)
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(!configs.is_empty())
            }
        });

        match (
            migrated_config_exists.await.unwrap(),
            try_exists(&path).await,
        ) {
            // The specific config we're looking for exists
            (_, Ok(true)) => ConfigDecision::LoadNew { path },

            // New format configs exist for this cloud, but not the specific profile requested
            (Ok(true), _) => ConfigDecision::NotFound { path },

            // No new format configs exist for this cloud, use legacy
            (Ok(false), _) => {
                if self.mapper_config_default_location == MapperConfigLocation::SeparateFile
                    && !self.tedge_toml_contains_cloud_config().await
                {
                    ConfigDecision::LoadNew { path }
                } else {
                    ConfigDecision::LoadLegacy
                }
            }

            // Permission error accessing mapper config directory
            (Err(err), _) => ConfigDecision::PermissionError {
                mapper_config_dir,
                error: err,
            },
        }
    }

    pub async fn update_toml(
        &self,
        update: &impl Fn(&mut TEdgeConfigDto, &TEdgeConfigReader) -> ConfigSettingResult<()>,
    ) -> Result<(), TEdgeConfigError> {
        let mut config = self.load_dto::<FileOnly>().await?;
        let reader = TEdgeConfigReader::from_dto(&config, self);
        update(&mut config, &reader)?;

        self.store(config).await
    }

    async fn cleanup_existing_mapper_configs(
        &self,
        cloud_type: CloudType,
    ) -> Result<(), TEdgeConfigError> {
        let mappers_dir = self.mappers_config_dir();
        let cloud_str = cloud_type.as_ref();

        // Clean up default profile: mappers/{cloud}/tedge.toml
        let default_config = mappers_dir.join(format!("{cloud_str}/tedge.toml"));
        self.cleanup_config_file_and_possibly_parent(&default_config)
            .await?;

        // Clean up profiled configs: mappers/{cloud}.*/tedge.toml
        let glob_pattern = format!("{mappers_dir}/{cloud_str}.*/tedge.toml");
        let paths = tokio::task::spawn_blocking(move || {
            glob::glob(&glob_pattern)
                .map(|entries| entries.collect::<Vec<_>>())
                .unwrap_or_default()
        })
        .await
        .unwrap();

        // Iterate over only the successfully collected paths
        for path in paths.into_iter().flatten() {
            let path = Utf8PathBuf::from_path_buf(path)
                .map_err(|p| anyhow::anyhow!("Invalid UTF-8 path: {}", p.display()))?;
            self.cleanup_config_file_and_possibly_parent(&path).await?;
        }

        Ok(())
    }

    pub async fn migrate_mapper_config(
        &self,
        cloud_type: CloudType,
    ) -> Result<(), TEdgeConfigError> {
        // Check if tedge.toml contains cloud config first
        if !self.tedge_toml_contains_cloud_config_for(cloud_type).await {
            // No cloud config in tedge.toml, nothing to migrate
            tracing::debug!("No {cloud_type} configuration in tedge.toml, skipping migration");
            return Ok(());
        }

        // Cloud config exists in tedge.toml - proceed with migration
        tracing::debug!(
            "Migrating {cloud_type} configuration from tedge.toml to separate mapper config files"
        );

        // Clean up any existing partial migration files before starting
        self.cleanup_existing_mapper_configs(cloud_type).await?;

        self.update_toml(&|dto, _rdr| {
            match cloud_type {
                CloudType::C8y => {
                    dto.c8y.non_profile.mapper_config_dir = Some(self.mappers_config_dir())
                }
                CloudType::Aws => {
                    dto.aws.non_profile.mapper_config_dir = Some(self.mappers_config_dir())
                }
                CloudType::Az => {
                    dto.az.non_profile.mapper_config_dir = Some(self.mappers_config_dir())
                }
            }
            Ok(())
        })
        .await
    }

    fn toml_path(&self) -> &Utf8Path {
        &self.tedge_config_file_path
    }

    pub(crate) async fn load(self) -> Result<TEdgeConfig, TEdgeConfigError> {
        let dto = self.load_dto_from_toml_and_env().await?;
        debug!(
            "Loading configuration from {:?}",
            self.tedge_config_file_path
        );
        Ok(TEdgeConfig::from_dto(dto, self.clone()))
    }

    async fn load_dto_from_toml_and_env(&self) -> Result<TEdgeConfigDto, TEdgeConfigError> {
        self.load_dto::<FileAndEnvironment>().await
    }

    async fn load_dto<Sources: ConfigSources>(&self) -> Result<TEdgeConfigDto, TEdgeConfigError> {
        let (dto, warnings) = self.load_dto_with_warnings::<Sources>().await?;

        warnings.emit();

        Ok(dto)
    }

    #[cfg(feature = "test")]
    #[track_caller]
    pub(crate) fn load_toml_str(toml: &str, location: TEdgeConfigLocation) -> TEdgeConfig {
        let (tedge_config, warnings) = Self::load_toml_str_with_warnings(toml, location);
        warnings.emit();
        tedge_config
    }

    #[cfg(feature = "test")]
    #[track_caller]
    pub(crate) fn load_toml_str_with_warnings(
        toml: &str,
        location: TEdgeConfigLocation,
    ) -> (TEdgeConfig, UnusedValueWarnings) {
        let toml_path = Utf8Path::new("/not/read/from/file/system");
        let toml_value: toml::Value = toml::from_str(toml).unwrap();
        let (mut dto, mut warnings) = deserialize_toml(toml_value.clone(), toml_path).unwrap();
        if let Some(migrations) = dto.config.version.unwrap_or_default().migrations() {
            let migrated_toml = migrations
                .into_iter()
                .fold(toml_value, |toml, migration| migration.apply_to(toml));
            (dto, warnings) = deserialize_toml(migrated_toml, toml_path).unwrap();
        }
        (TEdgeConfig::from_dto(dto, location), warnings)
    }

    pub(crate) async fn mapper_config_profiles<T>(
        &self,
    ) -> Option<impl Iterator<Item = Option<ProfileName>>>
    where
        T: ExpectedCloudType,
    {
        fn profile_name_from_filename(tedge_toml_path: &Path) -> Option<ProfileName> {
            let config_dir =
                std::str::from_utf8(tedge_toml_path.parent()?.file_name()?.as_bytes()).ok()?;
            config_dir.split_once(".")?.1.parse().ok()
        }

        match self.decide_config_source::<T>(None).await {
            ConfigDecision::LoadNew { .. }
            | ConfigDecision::NotFound { .. }
            | ConfigDecision::PermissionError { .. } => {
                let default_profile = std::iter::once(None);
                let config_paths = self.config_path::<T>();
                let glob_pattern = config_paths.toml_path_for(Some("*"));
                let profiles = tokio::task::spawn_blocking(move || {
                    glob::glob(glob_pattern.as_str())
                        .unwrap()
                        .flat_map(|path| Ok::<_, GlobError>(profile_name_from_filename(&path?)))
                })
                .await
                .unwrap();
                Some(default_profile.chain(profiles.filter(Option::is_some)))
            }
            ConfigDecision::LoadLegacy => None,
        }
    }

    async fn load_dto_with_warnings<Sources: ConfigSources>(
        &self,
    ) -> Result<(TEdgeConfigDto, UnusedValueWarnings), TEdgeConfigError> {
        let toml_path = self.toml_path();
        let mut tedge_toml_readable = true;
        let config = tokio::fs::read_to_string(toml_path)
            .await
            .unwrap_or_else(|_| {
                tedge_toml_readable = false;
                String::new()
            });
        let toml: toml::Value = toml::de::from_str(&config)?;
        let (mut dto, mut warnings) = deserialize_toml(toml, toml_path)?;

        if let Some(migrations) = dto.config.version.unwrap_or_default().migrations() {
            if tedge_toml_readable {
                tracing::info!("Migrating tedge.toml configuration to version 2");

                let toml: toml::Value = toml::de::from_str(&config)?;
                let migrated_toml = migrations
                    .into_iter()
                    .fold(toml, |toml, migration| migration.apply_to(toml));

                self.store_in(self.toml_path(), &migrated_toml, StoreEmptyConfig::Yes)
                    .await?;

                (dto, warnings) = deserialize_toml(migrated_toml, toml_path)?;
            }
        }

        dto.populate_mapper_configs(self).await?;

        if Sources::INCLUDE_ENVIRONMENT {
            update_with_environment_variables(&mut dto, &mut warnings)?;
        }

        Ok((dto, warnings))
    }

    async fn store(&self, mut config: TEdgeConfigDto) -> Result<(), TEdgeConfigError> {
        self.store_cloud(&mut config.c8y).await?;
        self.store_cloud(&mut config.az).await?;
        self.store_cloud(&mut config.aws).await?;
        self.store_in(self.toml_path(), &config, StoreEmptyConfig::Yes)
            .await
    }

    async fn store_in<S: Serialize>(
        &self,
        toml_path: &Utf8Path,
        config: &S,
        persist_if_empty: StoreEmptyConfig,
    ) -> Result<(), TEdgeConfigError> {
        let toml = toml::to_string_pretty(&config)?;

        if persist_if_empty == StoreEmptyConfig::No && toml.trim() == "" {
            return self
                .cleanup_config_file_and_possibly_parent(toml_path)
                .await;
        }

        // Create `$HOME/.tedge` or `/etc/tedge` directory in case it does not exist yet
        if !tokio::fs::try_exists(toml_path).await.unwrap_or(false) {
            let parent_dir = toml_path.parent().expect("provided path must have parent");
            tokio::fs::create_dir_all(parent_dir)
                .await
                .with_context(|| format!("Failed to create directory {parent_dir}"))?;
        }

        atomically_write_file_async(toml_path, toml.as_bytes()).await?;

        if let Err(err) = change_user_and_group(toml_path, "tedge", "tedge").await {
            warn!("failed to set file ownership for '{toml_path}': {err}");
        }

        if let Err(err) = change_mode(toml_path, 0o644).await {
            warn!("failed to set file permissions for '{toml_path}': {err}");
        }

        Ok(())
    }

    async fn store_cloud<S>(&self, cloud: &mut MultiDto<S>) -> Result<(), TEdgeConfigError>
    where
        S: Serialize + HasPath + Default + PartialEq,
    {
        if let Some(paths) = cloud.non_profile.config_path() {
            self.store_in(
                &paths.toml_path_for(None::<&ProfileName>),
                &cloud.non_profile,
                StoreEmptyConfig::No,
            )
            .await?;
            for (name, profile) in &mut cloud.profiles {
                self.store_in(
                    &paths.toml_path_for(Some(name)),
                    profile,
                    StoreEmptyConfig::No,
                )
                .await?;
            }
            std::mem::take(cloud);
        }
        Ok(())
    }

    async fn cleanup_config_file_and_possibly_parent(
        &self,
        config_file: &Utf8Path,
    ) -> Result<(), TEdgeConfigError> {
        match tokio::fs::remove_file(&config_file).await {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }

        match tokio::fs::remove_dir(config_file.parent().unwrap()).await {
            Ok(()) => Ok(()),
            // If the directory isn't empty, leave it, it may contain flows or something
            Err(e) if [ErrorKind::DirectoryNotEmpty, ErrorKind::NotFound].contains(&e.kind()) => {
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    }
}

pub trait ConfigSources {
    const INCLUDE_ENVIRONMENT: bool;
}

#[derive(Clone, Debug)]
pub struct FileAndEnvironment;
#[derive(Clone, Debug)]
pub struct FileOnly;

impl ConfigSources for FileAndEnvironment {
    const INCLUDE_ENVIRONMENT: bool = true;
}

impl ConfigSources for FileOnly {
    const INCLUDE_ENVIRONMENT: bool = false;
}

/// Decision about which configuration source to use
pub enum ConfigDecision {
    /// Load from new format file at the given path
    LoadNew { path: Utf8PathBuf },
    /// Load from tedge.toml via compatibility layer
    LoadLegacy,
    /// Configuration file not found
    NotFound { path: Utf8PathBuf },
    /// Permission error accessing mapper config directory
    PermissionError {
        mapper_config_dir: Utf8PathBuf,
        error: glob::GlobError,
    },
}

#[derive(Default, Debug, PartialEq, Eq)]
#[must_use]
pub struct UnusedValueWarnings(Vec<String>);

impl UnusedValueWarnings {
    pub fn emit(self) {
        for warning in self.0 {
            tracing::warn!("{warning}");
        }
    }

    pub fn push(&mut self, warning: String) {
        self.0.push(warning)
    }
}

fn update_with_environment_variables(
    dto: &mut TEdgeConfigDto,
    warnings: &mut UnusedValueWarnings,
) -> anyhow::Result<()> {
    for (key, value) in std::env::vars() {
        let tedge_key = match key.strip_prefix("TEDGE_") {
            Some("CONFIG_DIR") => continue,
            Some("CLOUD_PROFILE") => continue,
            Some(tedge_key) => match parse_key_without_warnings(tedge_key) {
                Ok(key) => key,
                Err(_) => {
                    warnings.push(format!(
                        "Unknown configuration field {:?} from environment variable {key}",
                        tedge_key.to_ascii_lowercase()
                    ));
                    continue;
                }
            },
            None => continue,
        };

        // TODO test these warnings are vaguely sensibly formatted
        if value.starts_with('"') || value.starts_with('[') {
            if let Ok(mut tmp_dto) =
                toml::from_str::<TEdgeConfigDto>(&format!("{tedge_key} = {value}"))
            {
                if let Err(e) = dto.take_value_from(&mut tmp_dto, &tedge_key) {
                    warnings.push(format!("Failed to process {key}: {e}"))
                }
                continue;
            }
        }
        if value.is_empty() {
            dto.try_unset_key(&tedge_key).with_context(|| {
                format!("Failed to reset value for {tedge_key} from environment variable {key}")
            })?;
        } else {
            dto.try_update_str(&tedge_key, &value).with_context(|| {
                format!("Failed to set value for {tedge_key} to {value:?} from environment variable {key}")
            })?;
        }
    }
    Ok(())
}

fn parse_key_without_warnings(tedge_key: &str) -> Result<WritableKey, ParseKeyError> {
    tracing::subscriber::with_default(NoSubscriber::new(), || {
        tedge_key
            .to_ascii_lowercase()
            .parse::<tedge_config::WritableKey>()
    })
}

fn deserialize_toml(
    toml: toml::Value,
    toml_path: &Utf8Path,
) -> Result<(TEdgeConfigDto, UnusedValueWarnings), TEdgeConfigError> {
    let mut warnings = UnusedValueWarnings(vec![]);
    let keys = keys_in(&toml);
    let dto: TEdgeConfigDto = TEdgeConfigDto::deserialize(toml)?;
    for key in keys {
        if key.parse::<DtoKey>().is_err() {
            warnings.push(format!(
                "Unknown configuration field {key:?} from toml file {toml_path}",
            ));
        }
    }

    Ok((dto, warnings))
}

fn keys_in(toml: &toml::Value) -> Vec<String> {
    let table = toml.as_table().unwrap();
    let mut keys = vec![];
    for (key, value) in table {
        if let Some(table) = value.as_table() {
            keys.append(&mut keys_in_inner(key, table))
        }
    }
    keys
}

fn keys_in_inner(prefix: &str, table: &toml::map::Map<String, toml::Value>) -> Vec<String> {
    let mut res = vec![];
    for (key, value) in table {
        if let Some(table) = value.as_table() {
            res.append(&mut keys_in_inner(&format!("{prefix}.{key}"), table));
        } else {
            res.push(format!("{prefix}.{key}"));
        }
    }
    res
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum StoreEmptyConfig {
    No,
    Yes,
}

#[cfg(test)]
mod tests {
    use crate::models::AbsolutePath;
    use crate::tedge_toml::mapper_config::AzMapperSpecificConfig;
    use crate::tedge_toml::mapper_config::C8yMapperSpecificConfig;
    use crate::tedge_toml::Cloud;
    use once_cell::sync::Lazy;
    use tedge_config_macros::ProfileName;
    use tedge_test_utils::fs::TempTedgeDir;
    use tokio::sync::Mutex;
    use tokio::sync::MutexGuard;

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

    #[tokio::test]
    async fn old_toml_can_be_read_in_its_entirety() {
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
        let (tedge_config, warnings) = TEdgeConfig::load_toml_str_with_warnings(toml);

        // No warnings should be emitted
        assert_eq!(warnings, UnusedValueWarnings::default());

        assert_eq!(
            tedge_config.device_cert_path(None::<Void>).unwrap(),
            "/tedge/device-cert.pem".parse().unwrap()
        );
        assert_eq!(
            tedge_config.device_key_path(None::<Void>).unwrap(),
            "/tedge/device-key.pem".parse().unwrap()
        );
        assert_eq!(tedge_config.device.ty, "a-device");
        assert_eq!(u16::from(tedge_config.mqtt.bind.port), 1886);
        assert_eq!(u16::from(tedge_config.mqtt.client.port), 1885);
    }

    #[tokio::test]
    async fn config_can_be_loaded_if_tedge_toml_does_not_exist() {
        let (dir, t) = create_temp_tedge_config("").unwrap();
        tokio::fs::remove_file(dir.path().join("tedge.toml"))
            .await
            .unwrap();
        let _env_lock = EnvSandbox::new().await;

        t.load().await.unwrap();
    }

    #[tokio::test]
    async fn toml_values_can_be_overridden_with_environment() {
        let (_dir, t) = create_temp_tedge_config("apt.name = \"tedge.*\"").unwrap();
        let mut env = EnvSandbox::new().await;
        env.set_var("TEDGE_APT_NAME", "apt.env.*");
        let config = t.load().await.unwrap();
        assert_eq!(config.apt.name.or_none().unwrap(), "apt.env.*");
    }

    #[tokio::test]
    async fn environment_variables_can_contain_toml_syntax_strings() {
        let (_dir, t) = create_temp_tedge_config("apt.name = \"tedge.*\"").unwrap();
        let mut env = EnvSandbox::new().await;
        env.set_var("TEDGE_APT_NAME", "\"apt.env.*\"");
        let config = t.load().await.unwrap();
        assert_eq!(config.apt.name.or_none().unwrap(), "apt.env.*");
    }

    #[tokio::test]
    async fn environment_variables_are_parsed_using_custom_fromstr_implementations() {
        let (_dir, t) = create_temp_tedge_config("").unwrap();
        let mut env = EnvSandbox::new().await;
        env.set_var("TEDGE_DIAG_PLUGIN_PATHS", "test,values");
        let config = t.load().await.unwrap();
        assert_eq!(config.diag.plugin_paths, ["test", "values"]);
    }

    #[tokio::test]
    async fn environment_variables_can_contain_toml_format_arrays() {
        let (_dir, t) = create_temp_tedge_config("").unwrap();
        let mut env = EnvSandbox::new().await;
        env.set_var("TEDGE_DIAG_PLUGIN_PATHS", "[\"test\",\"values\"]");
        let config = t.load().await.unwrap();
        assert_eq!(config.diag.plugin_paths, ["test", "values"]);
    }

    #[tokio::test]
    async fn environment_variables_are_read_with_migrated_mapper_config() {
        let (ttd, t) = create_temp_tedge_config("").unwrap();
        ttd.dir("mappers")
            .dir("c8y")
            .file("tedge.toml")
            .with_raw_content("url = \"example.com\"");
        let mut env = EnvSandbox::new().await;
        env.set_var("TEDGE_C8Y_ROOT_CERT_PATH", "/root/cert/path");
        let config = t.load().await.unwrap();
        let c8y_config = config
            .mapper_config::<C8yMapperSpecificConfig>(&None::<ProfileName>)
            .unwrap();
        assert_eq!(
            c8y_config.http().or_none().unwrap().host().to_string(),
            "example.com",
            "Verify that c8y/tedge.toml file has actually been read"
        );
        assert_eq!(c8y_config.root_cert_path.to_string(), "/root/cert/path");
    }

    #[tokio::test]
    async fn empty_environment_variables_reset_configuration_parameters() {
        let (_dir, t) = create_temp_tedge_config("apt.name = \"tedge.*\"").unwrap();
        let mut env = EnvSandbox::new().await;
        env.set_var("TEDGE_APT_NAME", "");
        let config = t.load().await.unwrap();
        assert_eq!(config.apt.name.or_none(), None);
    }

    #[tokio::test]
    async fn environment_variables_can_override_profiled_configurations() {
        let (_dir, t) =
            create_temp_tedge_config("az.profiles.test.root_cert_path = \"/toml/path\"").unwrap();
        let mut env = EnvSandbox::new().await;
        env.set_var("TEDGE_AZ_PROFILES_TEST_ROOT_CERT_PATH", "/env/path");
        let config = t.load().await.unwrap();
        let az_config = config
            .mapper_config::<AzMapperSpecificConfig>(&Some(
                ProfileName::try_from("test".to_owned()).unwrap(),
            ))
            .unwrap();
        assert_eq!(
            az_config.root_cert_path,
            AbsolutePath::try_new("/env/path").unwrap()
        );
    }

    #[tokio::test]
    async fn config_dir_environment_variable_does_not_generate_a_warning() {
        let (_dir, t) = create_temp_tedge_config("").unwrap();
        let mut env = EnvSandbox::new().await;
        env.set_var("TEDGE_CONFIG_DIR", "/home/tedge/config");
        let (_config, warnings) = t
            .load_dto_with_warnings::<FileAndEnvironment>()
            .await
            .unwrap();
        assert_eq!(warnings.0, &[] as &[&'static str]);
    }

    #[tokio::test]
    async fn specifies_file_name_and_variable_path_in_relevant_warnings_after_migrations() {
        let (dir, t) = create_temp_tedge_config(
            "config.version = \"2\"\nc8y.smartrest.unknown = \"test.c8y.io\"",
        )
        .unwrap();
        let _env_lock = EnvSandbox::new().await;
        let toml_path = dir.utf8_path().join("tedge.toml");
        let (_config, warnings) = t
            .load_dto_with_warnings::<FileAndEnvironment>()
            .await
            .unwrap();
        assert_eq!(
            warnings.0,
            [format!(
                "Unknown configuration field \"c8y.smartrest.unknown\" from toml file {toml_path}"
            )]
        );
    }

    #[tokio::test]
    async fn specifies_file_name_and_variable_path_in_relevant_warnings_before_migrations() {
        let (dir, t) = create_temp_tedge_config("c8y.smartrest.unknown = \"test.c8y.io\"").unwrap();
        let _env_lock = EnvSandbox::new().await;
        let toml_path = dir.utf8_path().join("tedge.toml");
        let (_config, warnings) = t
            .load_dto_with_warnings::<FileAndEnvironment>()
            .await
            .unwrap();
        assert_eq!(
            warnings.0,
            [format!(
                "Unknown configuration field \"c8y.smartrest.unknown\" from toml file {toml_path}"
            )]
        );
    }

    #[tokio::test]
    async fn specifies_environment_variable_name_in_relevant_warnings() {
        let (_dir, t) = create_temp_tedge_config("").unwrap();
        let mut env = EnvSandbox::new().await;
        env.set_var("TEDGE_UNKNOWN_VALUE", "should just warn");
        let (_config, warnings) = t
            .load_dto_with_warnings::<FileAndEnvironment>()
            .await
            .unwrap();
        assert_eq!(warnings.0, ["Unknown configuration field \"unknown_value\" from environment variable TEDGE_UNKNOWN_VALUE"]);
    }

    #[tokio::test]
    async fn unsetting_configuration_for_unknown_profile_does_not_warn_or_error() {
        let (_dir, t) = create_temp_tedge_config("").unwrap();
        let mut env = EnvSandbox::new().await;
        env.set_var("TEDGE_C8Y_PROFILES_TEST_URL", "");
        let (_config, warnings) = t
            .load_dto_with_warnings::<FileAndEnvironment>()
            .await
            .unwrap();
        assert_eq!(warnings.0, &[] as &[&'static str]);
    }

    #[tokio::test]
    async fn environment_variable_causes_error_if_its_value_cannot_be_parsed() {
        let (_dir, t) = create_temp_tedge_config("").unwrap();
        let mut env = EnvSandbox::new().await;
        env.set_var("TEDGE_SUDO_ENABLE", "yes");
        let err = t
            .load_dto_with_warnings::<FileAndEnvironment>()
            .await
            .unwrap_err();
        assert!(dbg!(err.to_string()).contains("TEDGE_SUDO_ENABLE"));
    }

    #[tokio::test]
    async fn environment_variables_are_ignored_in_file_only_mode() {
        let (_dir, t) = create_temp_tedge_config("sudo.enable = true").unwrap();
        let mut env = EnvSandbox::new().await;
        env.set_var("TEDGE_SUDO_ENABLE", "false");
        let (config, _) = t.load_dto_with_warnings::<FileOnly>().await.unwrap();
        assert_eq!(config.sudo.enable, Some(true));
    }

    #[tokio::test]
    async fn empty_environment_variables_are_ignored() {
        let (_dir, t) = create_temp_tedge_config("").unwrap();
        let mut env = EnvSandbox::new().await;
        env.set_var("TEDGE_SUDO_ENABLE", "");
        let (config, _) = t
            .load_dto_with_warnings::<FileAndEnvironment>()
            .await
            .unwrap();
        assert_eq!(config.sudo.enable, None);
    }

    #[tokio::test]
    async fn environment_variable_profile_warnings_use_key_with_correct_format() {
        let (_dir, t) = create_temp_tedge_config("").unwrap();
        let mut env = EnvSandbox::new().await;
        env.set_var("TEDGE_C8Y_PROFILES_TEST_UNKNOWN", "override.c8y.io");
        let (_, warnings) = t
            .load_dto_with_warnings::<FileAndEnvironment>()
            .await
            .unwrap();

        assert_eq!(
                warnings.0,
                ["Unknown configuration field \"c8y_profiles_test_unknown\" from environment variable TEDGE_C8Y_PROFILES_TEST_UNKNOWN"]
            );
    }

    #[tokio::test]
    async fn toml_profile_warnings_use_key_with_correct_format() {
        let (_dir, t) = create_temp_tedge_config(
            "
        [c8y.profiles.test]
        unknown = \"test.c8y.io\"",
        )
        .unwrap();
        let _env_lock = EnvSandbox::new().await;

        let (_, warnings) = t
            .load_dto_with_warnings::<FileAndEnvironment>()
            .await
            .unwrap();
        assert!(dbg!(warnings.0.first().unwrap()).contains("c8y.profiles.test.unknown"));
    }

    #[tokio::test]
    async fn c8y_config_can_be_migrated() {
        let (_dir, location) = create_temp_tedge_config("c8y.url = \"test.c8y.io\"").unwrap();
        let _env_lock = EnvSandbox::new().await;

        location
            .migrate_mapper_config(CloudType::C8y)
            .await
            .unwrap();

        let c8y_tedge_toml =
            tokio::fs::read_to_string(location.mappers_config_dir().join("c8y/tedge.toml"))
                .await
                .unwrap();
        assert_eq!(
            toml::from_str::<toml::Table>(&c8y_tedge_toml).unwrap(),
            toml::toml!(url = "test.c8y.io")
        );

        // tedge.toml may or may not exist, but the key thing is the url isn't defined there
        let tedge_toml = tokio::fs::read_to_string(location.tedge_config_file_path)
            .await
            .unwrap_or_default();
        assert!(!toml::from_str::<toml::Table>(&tedge_toml)
            .unwrap()
            .contains_key("c8y"));
    }

    #[tokio::test]
    async fn profiled_c8y_config_can_be_migrated() {
        let (_dir, location) =
            create_temp_tedge_config("c8y.profiles.test.url = \"test.c8y.io\"").unwrap();
        let _env_lock = EnvSandbox::new().await;

        location
            .migrate_mapper_config(CloudType::C8y)
            .await
            .unwrap();

        let test_tedge_toml =
            tokio::fs::read_to_string(location.mappers_config_dir().join("c8y.test/tedge.toml"))
                .await
                .unwrap();
        assert_eq!(
            toml::from_str::<toml::Table>(&test_tedge_toml).unwrap(),
            toml::toml!(url = "test.c8y.io")
        );

        let c8y_tedge_toml =
            tokio::fs::read_to_string(location.mappers_config_dir().join("c8y/tedge.toml"))
                .await
                .unwrap_or_default();
        assert_eq!(
            toml::from_str::<toml::Table>(&c8y_tedge_toml).unwrap(),
            toml::Table::new()
        );

        // tedge.toml may or may not exist, but the key thing is the url isn't defined there
        let tedge_toml = tokio::fs::read_to_string(location.tedge_config_file_path)
            .await
            .unwrap_or_default();
        assert!(!toml::from_str::<toml::Table>(&tedge_toml)
            .unwrap()
            .contains_key("c8y"));
    }

    #[tokio::test]
    async fn az_config_can_be_migrated() {
        let (_dir, location) = create_temp_tedge_config("az.url = \"example.com\"").unwrap();
        let _env_lock = EnvSandbox::new().await;

        location.migrate_mapper_config(CloudType::Az).await.unwrap();

        let az_tedge_toml =
            tokio::fs::read_to_string(location.mappers_config_dir().join("az/tedge.toml"))
                .await
                .unwrap();
        assert_eq!(
            toml::from_str::<toml::Table>(&az_tedge_toml).unwrap(),
            toml::toml!(url = "example.com")
        );

        // tedge.toml may or may not exist, but the key thing is the url isn't defined there
        let tedge_toml = tokio::fs::read_to_string(location.tedge_config_file_path)
            .await
            .unwrap_or_default();
        assert!(!toml::from_str::<toml::Table>(&tedge_toml)
            .unwrap()
            .contains_key("az"));
    }

    #[tokio::test]
    async fn aws_config_can_be_migrated() {
        let (_dir, location) = create_temp_tedge_config("aws.url = \"example.com\"").unwrap();
        let _env_lock = EnvSandbox::new().await;

        location
            .migrate_mapper_config(CloudType::Aws)
            .await
            .unwrap();

        let aws_tedge_toml =
            tokio::fs::read_to_string(location.mappers_config_dir().join("aws/tedge.toml"))
                .await
                .unwrap();
        assert_eq!(
            toml::from_str::<toml::Table>(&aws_tedge_toml).unwrap(),
            toml::toml!(url = "example.com")
        );

        // tedge.toml may or may not exist, but the key thing is the url isn't defined there
        let tedge_toml = tokio::fs::read_to_string(location.tedge_config_file_path)
            .await
            .unwrap_or_default();
        assert!(!toml::from_str::<toml::Table>(&tedge_toml)
            .unwrap()
            .contains_key("aws"));
    }

    #[tokio::test]
    async fn az_config_can_be_read_from_main_tedge_toml_even_with_flows_installed() {
        let (dir, location) = create_temp_tedge_config("az.url = \"example.com\"").unwrap();
        let _env_lock = EnvSandbox::new().await;

        dir.dir("mappers")
            .dir("az")
            .dir("flows")
            .file("flow.toml")
            .with_raw_content("# An example flow");

        let config = location.load().await.unwrap();
        assert_eq!(
            config
                .mapper_config::<AzMapperSpecificConfig>(&None::<&ProfileName>)
                .unwrap()
                .url()
                .or_none(),
            Some(&"example.com".parse().unwrap())
        );
    }

    #[tokio::test]
    async fn profiled_az_config_can_be_read_from_main_tedge_toml_even_with_flows_installed() {
        let (dir, location) =
            create_temp_tedge_config("az.profiles.test.url = \"example.com\"").unwrap();
        let _env_lock = EnvSandbox::new().await;

        dir.dir("mappers")
            .dir("az.test")
            .dir("flows")
            .file("flow.toml")
            .with_raw_content("# An example flow");

        let config = location.load().await.unwrap();
        assert_eq!(
            config
                .mapper_config::<AzMapperSpecificConfig>(&Some(
                    "test".parse::<ProfileName>().unwrap()
                ))
                .unwrap()
                .url()
                .or_none(),
            Some(&"example.com".parse().unwrap())
        );
    }

    mod cleanup_config_file_and_possibly_parent {
        use super::*;

        #[tokio::test]
        async fn removes_empty_config_file() {
            let (dir, location) = create_temp_tedge_config("").unwrap();

            dir.dir("mappers")
                .dir("c8y")
                .file("tedge.toml")
                .with_raw_content("url = \"example.com\"");

            let config_path = location.mappers_config_dir().join("c8y/tedge.toml");
            assert!(tokio::fs::try_exists(&config_path).await.unwrap());

            location
                .cleanup_config_file_and_possibly_parent(&config_path)
                .await
                .unwrap();

            assert!(!tokio::fs::try_exists(&config_path).await.unwrap());
        }

        #[tokio::test]
        async fn removes_parent_directory_when_empty() {
            let (dir, location) = create_temp_tedge_config("").unwrap();

            dir.dir("mappers")
                .dir("c8y")
                .file("tedge.toml")
                .with_raw_content("url = \"example.com\"");

            let config_path = location.mappers_config_dir().join("c8y/tedge.toml");
            let parent_dir = location.mappers_config_dir().join("c8y");

            assert!(tokio::fs::try_exists(&config_path).await.unwrap());
            assert!(tokio::fs::try_exists(&parent_dir).await.unwrap());

            location
                .cleanup_config_file_and_possibly_parent(&config_path)
                .await
                .unwrap();

            assert!(!tokio::fs::try_exists(&config_path).await.unwrap());
            assert!(!tokio::fs::try_exists(&parent_dir).await.unwrap());
        }

        #[tokio::test]
        async fn preserves_parent_directory_with_other_files() {
            let (dir, location) = create_temp_tedge_config("").unwrap();

            let mapper_dir = dir.dir("mappers").dir("c8y.test");
            mapper_dir
                .file("tedge.toml")
                .with_raw_content("url = \"example.com\"");
            mapper_dir
                .file("some-flow.toml")
                .with_raw_content("# flow content");

            let config_path = location.mappers_config_dir().join("c8y.test/tedge.toml");
            let flow_path = location
                .mappers_config_dir()
                .join("c8y.test/some-flow.toml");
            let parent_dir = location.mappers_config_dir().join("c8y.test");

            assert!(tokio::fs::try_exists(&config_path).await.unwrap());
            assert!(tokio::fs::try_exists(&flow_path).await.unwrap());

            location
                .cleanup_config_file_and_possibly_parent(&config_path)
                .await
                .unwrap();

            assert!(!tokio::fs::try_exists(&config_path).await.unwrap());
            assert!(tokio::fs::try_exists(&parent_dir).await.unwrap());
            assert!(tokio::fs::try_exists(&flow_path).await.unwrap());
        }

        #[tokio::test]
        async fn handles_nonexistent_file_gracefully() {
            let (_dir, location) = create_temp_tedge_config("").unwrap();

            let config_path = location.mappers_config_dir().join("c8y/tedge.toml");

            let result = location
                .cleanup_config_file_and_possibly_parent(&config_path)
                .await;

            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn handles_nonexistent_directory_gracefully() {
            let (_dir, location) = create_temp_tedge_config("").unwrap();

            let config_path = location.mappers_config_dir().join("nonexistent/tedge.toml");

            let result = location
                .cleanup_config_file_and_possibly_parent(&config_path)
                .await;

            assert!(result.is_ok());
        }
    }

    mod store_cloud_cleanup_integration {
        use super::*;

        #[tokio::test]
        async fn main_tedge_toml_stored_even_when_empty() {
            let (_dir, mut location) = create_temp_tedge_config("").unwrap();
            location.default_to_mapper_config_dir();
            let _env_lock = EnvSandbox::new().await;

            location.update_toml(&|_dto, _rdr| Ok(())).await.unwrap();

            let main_config_path = location.tedge_config_file_path.clone();
            assert!(tokio::fs::try_exists(&main_config_path).await.unwrap());
        }

        #[tokio::test]
        async fn empty_default_profile_config_is_removed() {
            let (_dir, mut location) = create_temp_tedge_config("").unwrap();
            location.default_to_mapper_config_dir();
            let _env_lock = EnvSandbox::new().await;

            location
                .update_toml(&|dto, _rdr| {
                    dto.try_update_str(&WritableKey::C8yUrl(None), "example.com")?;
                    Ok(())
                })
                .await
                .unwrap();

            let config_path = location.mappers_config_dir().join("c8y/tedge.toml");
            let parent_dir = location.mappers_config_dir().join("c8y");
            assert!(tokio::fs::try_exists(&config_path).await.unwrap());

            location
                .update_toml(&|dto, _rdr| {
                    dto.try_unset_key(&WritableKey::C8yUrl(None))?;
                    Ok(())
                })
                .await
                .unwrap();

            assert!(!tokio::fs::try_exists(&config_path).await.unwrap());
            assert!(!tokio::fs::try_exists(&parent_dir).await.unwrap());
        }

        #[tokio::test]
        async fn empty_named_profile_config_is_removed() {
            let (_dir, mut location) = create_temp_tedge_config("").unwrap();
            location.default_to_mapper_config_dir();
            let _env_lock = EnvSandbox::new().await;

            location
                .update_toml(&|dto, _rdr| {
                    dto.try_update_str(
                        &WritableKey::C8yUrl(Some("test".into())),
                        "test.example.com",
                    )?;
                    Ok(())
                })
                .await
                .unwrap();

            location
                .migrate_mapper_config(CloudType::C8y)
                .await
                .unwrap();

            let config_path = location.mappers_config_dir().join("c8y.test/tedge.toml");
            let parent_dir = location.mappers_config_dir().join("c8y.test");
            assert!(tokio::fs::try_exists(&config_path).await.unwrap());

            location
                .update_toml(&|dto, _rdr| {
                    dto.try_unset_key(&WritableKey::C8yUrl(Some("test".into())))?;
                    Ok(())
                })
                .await
                .unwrap();

            assert!(!tokio::fs::try_exists(&config_path).await.unwrap());
            assert!(!tokio::fs::try_exists(&parent_dir).await.unwrap());
        }

        #[tokio::test]
        async fn profile_directory_preserved_if_non_empty_after_deleting_config() {
            let (dir, mut location) = create_temp_tedge_config("").unwrap();
            location.default_to_mapper_config_dir();
            let _env_lock = EnvSandbox::new().await;

            location
                .update_toml(&|dto, _rdr| {
                    dto.try_update_str(
                        &WritableKey::C8yUrl(Some("test".into())),
                        "test.example.com",
                    )?;
                    Ok(())
                })
                .await
                .unwrap();

            dir.dir("mappers")
                .dir("c8y.test")
                .dir("flows")
                .file("some-flow.toml")
                .with_raw_content("# flow content");

            let config_path = location.mappers_config_dir().join("c8y.test/tedge.toml");
            let flow_path = location
                .mappers_config_dir()
                .join("c8y.test/flows/some-flow.toml");
            let parent_dir = location.mappers_config_dir().join("c8y.test");

            assert!(tokio::fs::try_exists(&config_path).await.unwrap());
            assert!(tokio::fs::try_exists(&flow_path).await.unwrap());

            location
                .update_toml(&|dto, _rdr| {
                    dto.try_unset_key(&WritableKey::C8yUrl(Some("test".into())))?;
                    Ok(())
                })
                .await
                .unwrap();

            assert!(!tokio::fs::try_exists(&config_path).await.unwrap());
            assert!(tokio::fs::try_exists(&parent_dir).await.unwrap());
            assert!(tokio::fs::try_exists(&flow_path).await.unwrap());
        }

        #[tokio::test]
        async fn multiple_profiles_cleaned_up_correctly() {
            let (_dir, mut location) = create_temp_tedge_config("").unwrap();
            location.default_to_mapper_config_dir();
            let _env_lock = EnvSandbox::new().await;

            location
                .update_toml(&|dto, _rdr| {
                    dto.try_update_str(&WritableKey::C8yUrl(None), "example.com")
                        .unwrap();
                    dto.try_update_str(
                        &WritableKey::C8yUrl(Some("test".into())),
                        "test.example.com",
                    )
                    .unwrap();
                    Ok(())
                })
                .await
                .unwrap();

            let default_config_path = location.mappers_config_dir().join("c8y/tedge.toml");
            let test_config_path = location.mappers_config_dir().join("c8y.test/tedge.toml");
            let default_dir = default_config_path.parent().unwrap();
            let test_dir = test_config_path.parent().unwrap();

            assert!(tokio::fs::try_exists(&default_config_path).await.unwrap());
            assert!(tokio::fs::try_exists(&test_config_path).await.unwrap());

            location
                .update_toml(&|dto, _rdr| {
                    dto.try_unset_key(&WritableKey::C8yUrl(None))?;
                    dto.try_unset_key(&WritableKey::C8yUrl(Some("test".into())))?;
                    Ok(())
                })
                .await
                .unwrap();

            assert!(!tokio::fs::try_exists(&default_config_path).await.unwrap());
            assert!(!tokio::fs::try_exists(&test_config_path).await.unwrap());
            assert!(!tokio::fs::try_exists(&default_dir).await.unwrap());
            assert!(!tokio::fs::try_exists(&test_dir).await.unwrap());
        }

        #[tokio::test]
        async fn multiple_clouds_cleaned_up_independently() {
            let (_dir, mut location) = create_temp_tedge_config("").unwrap();
            location.default_to_mapper_config_dir();
            let _env_lock = EnvSandbox::new().await;

            location
                .update_toml(&|dto, _rdr| {
                    dto.try_update_str(&WritableKey::C8yUrl(None), "c8y.example.com")
                        .unwrap();
                    dto.try_update_str(&WritableKey::AwsUrl(None), "aws.example.com")
                        .unwrap();
                    Ok(())
                })
                .await
                .unwrap();

            let c8y_config_path = location.mappers_config_dir().join("c8y/tedge.toml");
            let aws_config_path = location.mappers_config_dir().join("aws/tedge.toml");
            let c8y_dir = location.mappers_config_dir().join("c8y");
            let aws_dir = location.mappers_config_dir().join("aws");

            assert!(tokio::fs::try_exists(&c8y_config_path).await.unwrap());
            assert!(tokio::fs::try_exists(&aws_config_path).await.unwrap());

            location
                .update_toml(&|dto, _rdr| {
                    dto.try_unset_key(&WritableKey::C8yUrl(None)).unwrap();
                    Ok(())
                })
                .await
                .unwrap();

            assert!(!tokio::fs::try_exists(&c8y_config_path).await.unwrap());
            assert!(!tokio::fs::try_exists(&c8y_dir).await.unwrap());
            assert!(tokio::fs::try_exists(&aws_config_path).await.unwrap());
            assert!(tokio::fs::try_exists(&aws_dir).await.unwrap());
        }
    }

    mod migration_retry {
        use super::*;

        #[tokio::test]
        async fn migration_skips_when_no_config_in_tedge_toml() {
            let (dir, location) = create_temp_tedge_config("").unwrap();
            let _env_lock = EnvSandbox::new().await;

            // Create existing mapper files (from previous successful migration)
            dir.dir("mappers")
                .dir("c8y")
                .file("tedge.toml")
                .with_raw_content("url = \"test.c8y.io\"");

            // Run migration - should do nothing since tedge.toml has no c8y config
            location
                .migrate_mapper_config(CloudType::C8y)
                .await
                .unwrap();

            // Verify existing files unchanged
            let content =
                tokio::fs::read_to_string(location.mappers_config_dir().join("c8y/tedge.toml"))
                    .await
                    .unwrap();
            assert!(content.contains("test.c8y.io"));
        }

        #[tokio::test]
        async fn migration_retries_after_partial_failure() {
            let (dir, location) = create_temp_tedge_config(
                "c8y.url = \"default.c8y.io\"\nc8y.profiles.prod.url = \"prod.c8y.io\"",
            )
            .unwrap();
            let _env_lock = EnvSandbox::new().await;

            // Simulate partial migration: create only default profile with wrong data
            dir.dir("mappers")
                .dir("c8y")
                .file("tedge.toml")
                .with_raw_content("url = \"wrong.c8y.io\"");

            // Verify tedge.toml still has config (incomplete migration marker)
            assert!(
                location
                    .tedge_toml_contains_cloud_config_for(CloudType::C8y)
                    .await
            );

            // Retry migration - should clean up and recreate
            location
                .migrate_mapper_config(CloudType::C8y)
                .await
                .unwrap();

            // Verify both files now exist with correct data
            let default_toml =
                tokio::fs::read_to_string(location.mappers_config_dir().join("c8y/tedge.toml"))
                    .await
                    .unwrap();
            assert!(default_toml.contains("default.c8y.io"));

            let prod_toml = tokio::fs::read_to_string(
                location.mappers_config_dir().join("c8y.prod/tedge.toml"),
            )
            .await
            .unwrap();
            assert!(prod_toml.contains("prod.c8y.io"));

            // Verify tedge.toml cleaned up
            assert!(
                !location
                    .tedge_toml_contains_cloud_config_for(CloudType::C8y)
                    .await
            );
        }

        #[tokio::test]
        async fn cleanup_removes_all_profiles() {
            let (dir, location) = create_temp_tedge_config("").unwrap();
            let _env_lock = EnvSandbox::new().await;

            // Create multiple profile configs
            dir.dir("mappers")
                .dir("c8y")
                .file("tedge.toml")
                .with_raw_content("url = \"default\"");
            dir.dir("mappers")
                .dir("c8y.prod")
                .file("tedge.toml")
                .with_raw_content("url = \"prod\"");
            dir.dir("mappers")
                .dir("c8y.test")
                .file("tedge.toml")
                .with_raw_content("url = \"test\"");

            // Also create a flow file that should be preserved
            dir.dir("mappers")
                .dir("c8y.prod")
                .dir("flows")
                .file("my-flow.toml")
                .with_raw_content("# flow");

            let mappers_dir = location.mappers_config_dir();

            // Verify all exist
            assert!(tokio::fs::try_exists(mappers_dir.join("c8y/tedge.toml"))
                .await
                .unwrap());
            assert!(
                tokio::fs::try_exists(mappers_dir.join("c8y.prod/tedge.toml"))
                    .await
                    .unwrap()
            );
            assert!(
                tokio::fs::try_exists(mappers_dir.join("c8y.test/tedge.toml"))
                    .await
                    .unwrap()
            );

            // Run cleanup
            location
                .cleanup_existing_mapper_configs(CloudType::C8y)
                .await
                .unwrap();

            // Verify tedge.toml files removed
            assert!(!tokio::fs::try_exists(mappers_dir.join("c8y/tedge.toml"))
                .await
                .unwrap());
            assert!(
                !tokio::fs::try_exists(mappers_dir.join("c8y.prod/tedge.toml"))
                    .await
                    .unwrap()
            );
            assert!(
                !tokio::fs::try_exists(mappers_dir.join("c8y.test/tedge.toml"))
                    .await
                    .unwrap()
            );

            // Verify empty directories removed
            assert!(!tokio::fs::try_exists(mappers_dir.join("c8y"))
                .await
                .unwrap());
            assert!(!tokio::fs::try_exists(mappers_dir.join("c8y.test"))
                .await
                .unwrap());

            // Verify flow file and directory preserved
            assert!(tokio::fs::try_exists(mappers_dir.join("c8y.prod"))
                .await
                .unwrap());
            assert!(
                tokio::fs::try_exists(mappers_dir.join("c8y.prod/flows/my-flow.toml"))
                    .await
                    .unwrap()
            );
        }

        #[tokio::test]
        async fn migration_is_idempotent() {
            let (_dir, location) = create_temp_tedge_config("c8y.url = \"test.c8y.io\"").unwrap();
            let _env_lock = EnvSandbox::new().await;

            // First migration
            location
                .migrate_mapper_config(CloudType::C8y)
                .await
                .unwrap();

            let first_content =
                tokio::fs::read_to_string(location.mappers_config_dir().join("c8y/tedge.toml"))
                    .await
                    .unwrap();

            // Run migration again (tedge.toml now has no c8y section)
            location
                .migrate_mapper_config(CloudType::C8y)
                .await
                .unwrap();

            // Verify file unchanged
            let second_content =
                tokio::fs::read_to_string(location.mappers_config_dir().join("c8y/tedge.toml"))
                    .await
                    .unwrap();
            assert_eq!(first_content, second_content);
        }
    }

    static LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    #[allow(unused)]
    /// A pseudo-sandbox for writing tests that interact with environment
    /// variables
    ///
    /// Creating an [EnvSandbox] will first acquire a mutex to ensure no other
    /// test is interacting with the environment, then clear any configured
    /// environment variables.
    struct EnvSandbox<'a>(MutexGuard<'a, ()>);

    impl EnvSandbox<'_> {
        pub async fn new() -> Self {
            let guard = LOCK.lock().await;
            for (key, _) in std::env::vars_os() {
                std::env::remove_var(key);
            }
            Self(guard)
        }

        pub fn set_var(&mut self, key: &str, value: &str) {
            std::env::set_var(key, value);
        }
    }

    fn create_temp_tedge_config(
        content: &str,
    ) -> std::io::Result<(TempTedgeDir, TEdgeConfigLocation)> {
        let dir = TempTedgeDir::new();
        dir.file("tedge.toml").with_raw_content(content);
        let config_location = TEdgeConfigLocation::from_custom_root(dir.path());
        Ok((dir, config_location))
    }

    enum Void {}

    impl From<Void> for Cloud<'_> {
        fn from(value: Void) -> Self {
            match value {}
        }
    }
}
