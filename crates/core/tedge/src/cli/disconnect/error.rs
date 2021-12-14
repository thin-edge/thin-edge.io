use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum DisconnectBridgeError {
    #[error(transparent)]
    Configuration(#[from] crate::ConfigError),

    #[error("File operation error. Check permissions for {1}.")]
    FileOperationFailed(#[source] std::io::Error, PathBuf),

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    SystemServiceError(#[from] crate::system_services::SystemServiceError),

    #[error("Bridge file does not exist.")]
    BridgeFileDoesNotExist,
}
