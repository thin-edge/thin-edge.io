pub mod mqtt_config;
pub mod system_services;
pub mod tedge_config_cli;

pub use self::tedge_config_cli::config_setting::*;
pub use self::tedge_config_cli::error::*;
pub use self::tedge_config_cli::models::*;
pub use self::tedge_config_cli::tedge_config::*;
pub use self::tedge_config_cli::tedge_config_location::*;
pub use self::tedge_config_cli::tedge_config_repository::*;
pub use camino::Utf8Path as Path;
pub use camino::Utf8PathBuf as PathBuf;
pub use certificate::CertificateError;
pub use tedge_config_macros::all_or_nothing;
pub use tedge_config_macros::OptionalConfig;

impl TEdgeConfig {
    pub fn new(config_location: TEdgeConfigLocation) -> Result<Self, TEdgeConfigError> {
        TEdgeConfigRepository::new(config_location).load()
    }

    pub fn load(config_dir: &Path) -> Result<TEdgeConfig, TEdgeConfigError> {
        let config_location = TEdgeConfigLocation::from_custom_root(config_dir);
        TEdgeConfig::new(config_location)
    }
}
