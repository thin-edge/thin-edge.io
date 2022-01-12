use crate::system_services;

#[derive(thiserror::Error, Debug)]
pub enum TEdgeError {
    #[error("TOML parse error")]
    FromTomlParse(#[from] toml::de::Error),

    #[error("TOML serialization error")]
    FromInvalidToml(#[from] toml::ser::Error),

    #[error("I/O error")]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromPaths(#[from] tedge_utils::paths::PathsError),

    #[error(transparent)]
    FromTEdgeConfig(#[from] tedge_config::TEdgeConfigError),

    #[error(transparent)]
    FromTEdgeConfigSetting(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    FromRumqttClient(#[from] rumqttc::ClientError),

    #[error(transparent)]
    FromSystemServiceError(#[from] system_services::SystemServiceError),
}
