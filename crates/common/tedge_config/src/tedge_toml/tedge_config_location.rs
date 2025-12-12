use std::path::Path;

use crate::tedge_toml::mapper_config::ExpectedCloudType;
use crate::tedge_toml::mapper_config::HasPath;
use crate::tedge_toml::DtoKey;
use crate::ConfigSettingResult;
use crate::TEdgeConfig;
use crate::TEdgeConfigDto;
use crate::TEdgeConfigError;
use crate::TEdgeConfigReader;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use serde::Deserialize as _;
use serde::Serialize;
use std::path::PathBuf;
use tedge_config_macros::MultiDto;
use tedge_config_macros::ProfileName;
use tedge_utils::file::change_mode;
use tedge_utils::file::change_user_and_group;
use tedge_utils::fs::atomically_write_file_async;
use tokio::fs::DirEntry;
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
        }
    }

    pub(crate) fn tedge_config_root_path(&self) -> &Utf8Path {
        &self.tedge_config_root_path
    }

    pub fn mappers_config_dir(&self) -> Utf8PathBuf {
        self.tedge_config_root_path.join("mappers")
    }

    /// Decide which configuration source to use for a given cloud and profile
    ///
    /// This function centralizes the decision logic for mapper configuration precedence:
    /// 1. New format (`mappers/[cloud].toml` or `mappers/[cloud].d/[profile].toml`) takes precedence
    /// 2. If new format exists for some profiles but not the requested one, returns NotFound
    /// 3. If the config directory is inaccessible due to a permissions error, return Error
    /// 4. If no new format exists at all, fall back to legacy tedge.toml format
    pub async fn decide_config_source<T>(&self, profile: Option<&ProfileName>) -> ConfigDecision
    where
        T: ExpectedCloudType,
    {
        use tokio::fs::try_exists;

        let mapper_config_dir = self.mappers_config_dir();
        let ty = T::expected_cloud_type().to_string();

        let filename = profile.map_or_else(|| format!("{ty}.toml"), |p| format!("{ty}.d/{p}.toml"));
        let path = mapper_config_dir.join(&filename);

        let default_profile_path = mapper_config_dir.join(format!("{ty}.toml"));
        let profile_dir_path = mapper_config_dir.join(format!("{ty}.d"));

        match (
            try_exists(&default_profile_path).await,
            try_exists(&profile_dir_path).await,
            try_exists(&path).await,
        ) {
            // The specific config we're looking for exists
            (_, _, Ok(true)) => ConfigDecision::LoadNew { path },

            // New format configs exist for this cloud, but not the specific profile requested
            (Ok(true), _, _) | (_, Ok(true), _) => ConfigDecision::NotFound { path },

            // No new format configs exist for this cloud, use legacy
            (Ok(false), Ok(false), _) => ConfigDecision::LoadLegacy,

            // Permission error accessing mapper config directory
            (Err(err), _, _) | (_, Err(err), _) => ConfigDecision::PermissionError {
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
    pub(crate) fn load_toml_str(toml: &str, location: TEdgeConfigLocation) -> TEdgeConfig {
        let (tedge_config, warnings) = Self::load_toml_str_with_warnings(toml, location);
        warnings.emit();
        tedge_config
    }

    #[cfg(feature = "test")]
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
    ) -> Option<
        Box<dyn futures::stream::Stream<Item = Option<ProfileName>> + Unpin + Send + Sync + '_>,
    >
    where
        T: ExpectedCloudType,
    {
        use futures::future::ready;
        use futures::StreamExt;

        fn file_name_string(entry: tokio::io::Result<DirEntry>) -> Option<String> {
            entry.ok()?.file_name().into_string().ok()
        }

        fn profile_name_from_filename(filename: &str) -> Option<ProfileName> {
            ProfileName::try_from(filename.strip_suffix(".toml")?.to_owned()).ok()
        }

        let ty = T::expected_cloud_type();

        match self.decide_config_source::<T>(None).await {
            ConfigDecision::LoadNew { .. }
            | ConfigDecision::NotFound { .. }
            | ConfigDecision::PermissionError { .. } => {
                let default_profile = futures::stream::once(ready(None));
                match tokio::fs::read_dir(self.mappers_config_dir().join(format!("{ty}.d"))).await {
                    Ok(profile_dir) => Some(Box::new(
                        default_profile.chain(
                            tokio_stream::wrappers::ReadDirStream::new(profile_dir)
                                .filter_map(|entry| ready(file_name_string(entry)))
                                .filter_map(|s| ready(profile_name_from_filename(&s)))
                                .map(Some),
                        ),
                    )),
                    Err(_) => Some(Box::new(default_profile)),
                }
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

                self.store_in(self.toml_path(), &migrated_toml).await?;

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
        self.store_in(self.toml_path(), &config).await
    }

    async fn store_in<S: Serialize>(
        &self,
        toml_path: &Utf8Path,
        config: &S,
    ) -> Result<(), TEdgeConfigError> {
        let toml = toml::to_string_pretty(&config)?;

        // Create `$HOME/.tedge` or `/etc/tedge` directory in case it does not exist yet
        if !tokio::fs::try_exists(toml_path).await.unwrap_or(false) {
            tokio::fs::create_dir_all(toml_path.parent().expect("provided path must have parent"))
                .await?;
        }

        atomically_write_file_async(toml_path, toml.as_bytes()).await?;

        if let Err(err) =
            change_user_and_group(toml_path.into(), "tedge".into(), "tedge".into()).await
        {
            warn!("failed to set file ownership for '{toml_path}': {err}");
        }

        if let Err(err) = change_mode(toml_path.as_ref(), 0o644).await {
            warn!("failed to set file permissions for '{toml_path}': {err}");
        }

        Ok(())
    }

    async fn store_cloud<S>(&self, cloud: &mut MultiDto<S>) -> Result<(), TEdgeConfigError>
    where
        S: Serialize + HasPath + Default + PartialEq,
    {
        if let Some(paths) = cloud.non_profile.config_path() {
            self.store_in(&paths.path_for(None), &cloud.non_profile)
                .await?;
            for (name, profile) in &mut cloud.profiles {
                self.store_in(&paths.path_for(Some(name)), profile).await?;
            }
            std::mem::take(cloud);
        }
        Ok(())
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
        error: std::io::Error,
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

#[cfg(test)]
mod tests {
    use crate::models::AbsolutePath;
    use crate::tedge_toml::mapper_config::AzMapperSpecificConfig;
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
