mod sudo;
pub use sudo::SudoCommandBuilder;
pub use sudo::SudoError;
pub mod cli;
mod system_toml;
pub use system_toml::*;

pub mod tedge_toml;
use tedge_config_macros::ProfileName;
pub use tedge_toml::error::*;
pub use tedge_toml::models;
pub use tedge_toml::tedge_config::TEdgeConfig;
pub use tedge_toml::tedge_config::TEdgeConfigDto;
pub use tedge_toml::tedge_config::TEdgeConfigReader;
pub use tedge_toml::tedge_config::TEdgeMqttClientAuthConfig;
pub use tedge_toml::tedge_config_location::*;
use tedge_utils::paths::TedgePaths;

pub use camino::Utf8Path as Path;
pub use camino::Utf8PathBuf as PathBuf;
pub use certificate::CertificateError;
use models::CloudType;
use std::path::Path as StdPath;
use strum::IntoEnumIterator;
pub use tedge_config_macros::all_or_nothing;
pub use tedge_config_macros::OptionalConfig;

use crate::tedge_toml::mapper_config::ExpectedCloudType;

impl TEdgeConfig {
    pub async fn load(config_dir: impl AsRef<StdPath>) -> Result<Self, TEdgeConfigError> {
        let config_location = TEdgeConfigLocation::from_custom_root(config_dir.as_ref());
        config_location.load().await
    }

    pub fn read_system_config(&self) -> SystemConfig {
        SystemConfig::try_new(self.root_dir()).unwrap_or_default()
    }

    pub fn config_root(&self) -> TedgePaths {
        let system = self.read_system_config();
        TedgePaths::from_root_with_defaults(self.root_dir(), system.user, system.group)
    }

    /// Load [TEdgeConfig], using a separate mapper config file as the default
    /// behaviour if no clouds are already configured
    ///
    /// As of 2026-01-05, this is only used for testing how the new default will
    /// work before we fully adopt the new format. When we do this, we should
    /// probably also include `tedge config upgrade` commands in the
    /// relevant package postinstall scripts.
    #[cfg(feature = "test")]
    pub async fn load_prefer_separate_mapper_config(
        config_dir: impl AsRef<StdPath>,
    ) -> Result<Self, TEdgeConfigError> {
        TEdgeConfigLocation::from_custom_root(config_dir.as_ref())
            .load()
            .await
    }

    pub async fn update_toml(
        self,
        update: &impl Fn(&mut TEdgeConfigDto, &TEdgeConfigReader) -> ConfigSettingResult<()>,
    ) -> Result<(), TEdgeConfigError> {
        self.location().update_toml(update).await
    }

    pub async fn migrate_mapper_configs(self) -> Result<PathBuf, TEdgeConfigError> {
        // Create a backup of tedge.toml before starting migration
        let backup_path = self.location().backup_tedge_config().await?;
        tracing::info!("Created backup of tedge.toml at: {}", backup_path);

        for cloud_type in CloudType::iter() {
            self.location().migrate_mapper_config(cloud_type).await?;
        }
        Ok(backup_path)
    }

    /// Check if a stale tedge.toml.bak backup file exists
    ///
    /// Returns Some(path) if a backup file is found, None otherwise.
    pub fn check_backup_exists(&self) -> Option<PathBuf> {
        self.location().check_backup_exists()
    }

    pub fn mapper_config_dir<T: ExpectedCloudType>(
        &self,
        profile: Option<&ProfileName>,
    ) -> PathBuf {
        self.location().config_dir::<T>(profile)
    }

    #[cfg(feature = "test")]
    /// A test only method designed for injecting configuration into tests
    ///
    /// ```
    /// use tedge_config::TEdgeConfig;
    /// let config = TEdgeConfig::load_toml_str("service.ty = \"service\"");
    ///
    /// assert_eq!(&config.service.ty, "service");
    /// // Defaults are preserved
    /// assert_eq!(config.sudo.enable, true);
    /// ```
    #[track_caller]
    pub fn load_toml_str(toml: &str) -> TEdgeConfig {
        TEdgeConfigLocation::load_toml_str(
            toml,
            TEdgeConfigLocation::from_custom_root("/dont/read/system/etc/tedge"),
        )
    }

    #[cfg(feature = "test")]
    #[track_caller]
    pub fn load_toml_str_with_warnings(toml: &str) -> (TEdgeConfig, UnusedValueWarnings) {
        TEdgeConfigLocation::load_toml_str_with_warnings(
            toml,
            TEdgeConfigLocation::from_custom_root("/dont/read/system/etc/tedge"),
        )
    }

    #[cfg(feature = "test")]
    #[track_caller]
    pub fn load_toml_str_with_root_dir(config_dir: impl AsRef<StdPath>, toml: &str) -> TEdgeConfig {
        TEdgeConfigLocation::load_toml_str(toml, TEdgeConfigLocation::from_custom_root(config_dir))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;

    #[tokio::test]
    async fn config_root_uses_system_toml_defaults() {
        let ttd = TempTedgeDir::new();
        ttd.file("tedge.toml").with_raw_content("");
        ttd.file("system.toml")
            .with_raw_content("user = 'service-user'\ngroup = 'service-group'\n");

        let config = TEdgeConfig::load(ttd.path()).await.unwrap();
        let config_root = config.config_root();

        assert_eq!(config_root.root(), ttd.path());
        assert_eq!(config_root.default_owner().user, "service-user");
        assert_eq!(config_root.default_owner().group, "service-group");
    }
}
