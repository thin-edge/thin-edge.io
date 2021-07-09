#[derive(thiserror::Error, Debug)]
pub enum TEdgeError {
    #[error("TOML parse error")]
    TomlParseError(#[from] toml::de::Error),

    #[error("TOML serialization error")]
    InvalidTomlError(#[from] toml::ser::Error),

    #[error("I/O error")]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    PathsError(#[from] crate::utils::paths::PathsError),

    #[error(transparent)]
    TEdgeConfigError(#[from] tedge_config::TEdgeConfigError),

    #[error(transparent)]
    TEdgeConfigSettingError(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    MqttClientError(#[from] mqtt_client::MqttClientError),
}
