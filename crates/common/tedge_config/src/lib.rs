pub mod mqtt_config;
pub mod system_services;
pub mod tedge_config_cli;

pub use self::tedge_config_cli::config_setting::*;
pub use self::tedge_config_cli::error::*;
pub use self::tedge_config_cli::models::*;
pub use self::tedge_config_cli::new::*;
pub use self::tedge_config_cli::tedge_config_location::*;
pub use self::tedge_config_cli::tedge_config_repository::*;
pub use camino::Utf8Path as Path;
pub use camino::Utf8PathBuf as PathBuf;
pub use certificate::CertificateError;

/// loads the new tedge config from system default
pub fn get_new_tedge_config() -> Result<TEdgeConfig, TEdgeConfigError> {
    let tedge_config_location = TEdgeConfigLocation::default();
    TEdgeConfigRepository::new(tedge_config_location).load_new()
}
