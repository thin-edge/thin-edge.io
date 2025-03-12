#[derive(thiserror::Error, Debug)]
pub enum DisconnectBridgeError {
    #[error("Bridge file does not exist.")]
    BridgeFileDoesNotExist,

    #[error(transparent)]
    Configuration(#[from] crate::ConfigError),

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    SystemServiceError(#[from] crate::system_services::SystemServiceError),

    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}
