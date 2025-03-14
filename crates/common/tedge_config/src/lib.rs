mod sudo;
pub use sudo::SudoCommandBuilder;
pub mod cli;
mod system_toml;
pub use system_toml::*;

pub mod tedge_toml;
pub use tedge_toml::error::*;
pub use tedge_toml::models;
pub use tedge_toml::tedge_config::TEdgeConfig;
pub use tedge_toml::tedge_config::TEdgeConfigDto;
pub use tedge_toml::tedge_config::TEdgeConfigReader;
pub use tedge_toml::tedge_config_location::*;

pub use camino::Utf8Path as Path;
pub use camino::Utf8PathBuf as PathBuf;
pub use certificate::CertificateError;
pub use tedge_config_macros::all_or_nothing;
pub use tedge_config_macros::OptionalConfig;

impl TEdgeConfig {
    pub async fn try_new(config_location: TEdgeConfigLocation) -> Result<Self, TEdgeConfigError> {
        config_location.load().await
    }

    pub fn try_new_sync(config_location: TEdgeConfigLocation) -> Result<Self, TEdgeConfigError> {
        config_location.load_sync()
    }

    pub async fn load(config_dir: &Path) -> Result<TEdgeConfig, TEdgeConfigError> {
        let config_location = TEdgeConfigLocation::from_custom_root(config_dir);
        TEdgeConfig::try_new(config_location).await
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
        TEdgeConfigLocation::load_toml_str(toml)
    }
}
