#[derive(thiserror::Error, Debug)]
pub enum ConnectError {
    #[error("Couldn't load certificate, provide valid certificate path in configuration. Use 'tedge config --set'")]
    Certificate,

    #[error(transparent)]
    Configuration(#[from] crate::ConfigError),

    #[error("Connection is already established. To remove existing connection use 'tedge disconnect {cloud}' and try again.",)]
    ConfigurationExists { cloud: String },

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    MqttClient(#[from] rumqttc::ClientError),

    #[error(transparent)]
    PathsError(#[from] tedge_utils::paths::PathsError),

    #[error("Provided endpoint url is not valid, provide valid url.\n{0}")]
    UrlParse(#[from] url::ParseError),

    #[error(transparent)]
    SystemServiceError(#[from] crate::system_services::SystemServiceError),

    #[error("Operation timed out. Is mosquitto running?")]
    TimeoutElapsedError,

    #[error(transparent)]
    PortSettingError(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    ConfigLoadError(#[from] tedge_config::TEdgeConfigError),

    #[error("Connection check failed")]
    ConnectionCheckError,

    #[error("Device is not connected to {cloud} cloud")]
    DeviceNotConnected { cloud: String },

    #[error("Provided JWT token is invalid: {token}")]
    InvalidJWTToken { token: String },

    #[error("Provided payload is not Base64 encoded.\n{0}")]
    FromBase64Decode(#[from] base64::DecodeError),

    #[error("Provided JSON does not contain URL.\n{0}")]
    FromSerdeJson(#[from] serde_json::Error),
}
