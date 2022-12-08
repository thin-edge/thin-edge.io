use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum DisconnectBridgeError {
    #[error("Bridge file does not exist.")]
    BridgeFileDoesNotExist,

    #[error(transparent)]
    Configuration(#[from] crate::ConfigError),

    #[error("File operation error. Check permissions for {1}.")]
    FileOperationFailed(#[source] std::io::Error, PathBuf),

    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error("Service operation failed.")]
    ServiceFailed,

    #[error(transparent)]
    SystemServiceError(#[from] tedge_config::system_services::SystemServiceError),
}
