use rumqttc::tokio_rustls::rustls;
use tedge_config::mqtt_config::MqttConfigBuildError;

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

    #[error("Can't crate MQTT config")]
    CreateMqttConfig(#[from] MqttConfigBuildError),

    #[error("Can't create TLS config")]
    CreateTlsConfig(#[from] rustls::Error),

    #[error(transparent)]
    PathsError(#[from] tedge_utils::paths::PathsError),

    #[error("Provided endpoint url is not valid, provide valid url.\n{0}")]
    UrlParse(#[from] url::ParseError),

    #[error(transparent)]
    SystemServiceError(#[from] tedge_config::system_services::SystemServiceError),

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

    #[error(
        "The JWT token received from Cumulocity is invalid.\nToken: {token}\nReason: {reason}"
    )]
    InvalidJWTToken { token: String, reason: String },

    #[error(transparent)]
    CertificateError(#[from] certificate::CertificateError),
}
