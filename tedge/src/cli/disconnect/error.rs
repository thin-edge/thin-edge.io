use crate::services;
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
    ServicesError(#[from] services::ServicesError),

    #[error("Bridge file does not exist.")]
    BridgeFileDoesNotExist,
}
