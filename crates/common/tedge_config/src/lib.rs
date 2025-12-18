mod sudo;
pub use sudo::SudoCommandBuilder;
pub use sudo::SudoError;
pub mod cli;
mod system_toml;
pub use system_toml::*;

pub mod tedge_toml;
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
use std::path::Path as StdPath;
pub use tedge_config_macros::all_or_nothing;
pub use tedge_config_macros::OptionalConfig;

impl TEdgeConfig {
    pub async fn load(config_dir: impl AsRef<StdPath>) -> Result<Self, TEdgeConfigError> {
        let config_location = TEdgeConfigLocation::from_custom_root(config_dir.as_ref());
        config_location.load().await
    }

    pub async fn update_toml(
        self,
        update: &impl Fn(&mut TEdgeConfigDto, &TEdgeConfigReader) -> ConfigSettingResult<()>,
    ) -> Result<(), TEdgeConfigError> {
        self.location().update_toml(update).await
    }

    pub async fn migrate_mapper_config(
        self,
        cloud_type: models::CloudType,
    ) -> Result<(), TEdgeConfigError> {
        self.location().migrate_mapper_config(cloud_type).await
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
