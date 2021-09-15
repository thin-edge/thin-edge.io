#[derive(thiserror::Error, Debug)]
pub enum TEdgeError {
    #[error("TOML parse error")]
    TomlParse(#[from] toml::de::Error),

    #[error("TOML serialization error")]
    InvalidToml(#[from] toml::ser::Error),

    #[error("I/O error")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Paths(#[from] tedge_utils::paths::PathsError),

    #[error(transparent)]
    TEdgeConfig(#[from] tedge_config::TEdgeConfigError),

    #[error(transparent)]
    TEdgeConfigSetting(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    MqttClient(#[from] rumqttc::ClientError),
}
