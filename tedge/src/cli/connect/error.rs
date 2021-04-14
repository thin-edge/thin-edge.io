#[derive(thiserror::Error, Debug)]
pub enum ConnectError {
    #[error("Couldn't load certificate, provide valid certificate path in configuration. Use 'tedge config --set'")]
    Certificate,

    #[error(transparent)]
    Configuration(#[from] crate::config::ConfigError),

    #[error("Connection is already established. To remove existing connection use 'tedge disconnect {cloud}' and try again.",)]
    ConfigurationExists { cloud: String },

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    MqttClient(#[from] mqtt_client::Error),

    #[error(transparent)]
    PathsError(#[from] crate::utils::paths::PathsError),

    #[error(transparent)]
    PersistError(#[from] tempfile::PersistError),

    #[error("Provided endpoint url is not valid, provide valid url.\n{0}")]
    UrlParse(#[from] url::ParseError),

    #[error(transparent)]
    ServicesError(#[from] crate::services::ServicesError),
}
