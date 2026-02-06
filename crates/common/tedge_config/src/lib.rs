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
        let mut config_location = TEdgeConfigLocation::from_custom_root(config_dir.as_ref());
        config_location.default_to_mapper_config_dir();
        config_location.load().await
    }

    pub async fn update_toml(
        self,
        update: &impl Fn(&mut TEdgeConfigDto, &TEdgeConfigReader) -> ConfigSettingResult<()>,
    ) -> Result<(), TEdgeConfigError> {
        self.location().update_toml(update).await
    }

    pub async fn migrate_mapper_configs(self) -> Result<(), TEdgeConfigError> {
        for cloud_type in CloudType::iter() {
            self.location().migrate_mapper_config(cloud_type).await?;
        }
        Ok(())
    }

    pub fn mapper_config_dir<T: ExpectedCloudType>(
        &self,
        profile: Option<&ProfileName>,
    ) -> camino::Utf8PathBuf {
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
    pub fn load_toml_str(toml: &str) -> TEdgeConfig {
        TEdgeConfigLocation::load_toml_str(
            toml,
            TEdgeConfigLocation::from_custom_root("/dont/read/system/etc/tedge"),
        )
    }

    #[cfg(feature = "test")]
    pub fn load_toml_str_with_warnings(toml: &str) -> (TEdgeConfig, UnusedValueWarnings) {
        TEdgeConfigLocation::load_toml_str_with_warnings(
            toml,
            TEdgeConfigLocation::from_custom_root("/dont/read/system/etc/tedge"),
        )
    }

    #[cfg(feature = "test")]
    pub fn load_toml_str_with_root_dir(config_dir: impl AsRef<StdPath>, toml: &str) -> TEdgeConfig {
        TEdgeConfigLocation::load_toml_str(toml, TEdgeConfigLocation::from_custom_root(config_dir))
    }
}
