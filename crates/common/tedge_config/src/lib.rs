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

/// loads the new tedge config from system default
pub fn get_new_tedge_config() -> Result<TEdgeConfig, TEdgeConfigError> {
    let tedge_config_location = TEdgeConfigLocation::default();
    TEdgeConfigRepository::new(tedge_config_location).load()
}

/// loads the tedge config from a config directory
pub fn load_tedge_config(config_dir: &Path) -> Result<TEdgeConfig, TEdgeConfigError> {
    let tedge_config_location = TEdgeConfigLocation::from_custom_root(config_dir);
    TEdgeConfigRepository::new(tedge_config_location).load()
}
