use crate::utils::paths;

#[derive(thiserror::Error, Debug)]
pub enum DisconnectBridgeError {
    #[error(transparent)]
    Configuration(#[from] crate::ConfigError),

    #[error("File operation error. Check permissions for {1}.")]
    FileOperationFailed(#[source] std::io::Error, String),

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    PathsError(#[from] paths::PathsError),

    #[error(transparent)]
    ServicesError(#[from] crate::system_services::ServicesError),

    #[error("Bridge file does not exist.")]
    BridgeFileDoesNotExist,
}
