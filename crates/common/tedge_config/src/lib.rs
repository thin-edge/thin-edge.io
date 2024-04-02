pub mod mqtt_config;
mod sudo;
pub mod system_services;
pub mod tedge_config_cli;
pub use sudo::SudoCommandBuilder;

pub use self::tedge_config_cli::config_setting::*;
pub use self::tedge_config_cli::error::*;
pub use self::tedge_config_cli::models::*;
pub use self::tedge_config_cli::tedge_config::*;
pub use self::tedge_config_cli::tedge_config_location::*;
pub use camino::Utf8Path as Path;
pub use camino::Utf8PathBuf as PathBuf;
pub use certificate::CertificateError;
pub use tedge_config_macros::all_or_nothing;
pub use tedge_config_macros::OptionalConfig;

impl TEdgeConfig {
    pub fn try_new(config_location: TEdgeConfigLocation) -> Result<Self, TEdgeConfigError> {
        config_location.load()
    }

    pub fn load(config_dir: &Path) -> Result<TEdgeConfig, TEdgeConfigError> {
        let config_location = TEdgeConfigLocation::from_custom_root(config_dir);
        TEdgeConfig::try_new(config_location)
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
