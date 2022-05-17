use std::path::PathBuf;
use tedge_utils::file::FileError;

#[derive(thiserror::Error, Debug)]
pub enum ConfigManagementError {
    #[error("No write access to {path:?}")]
    NoWriteAccess { path: PathBuf },

    #[error("The file name is not found or invalid: {path:?}")]
    FileNameNotFound { path: PathBuf },

    #[error("Failed to copy a file from {src:?} to {dest:?}")]
    FileCopyFailed { src: PathBuf, dest: PathBuf },

    #[error(
        "The requested config_type {config_type} is not defined in the plugin configuration file."
    )]
    InvalidRequestedConfigType { config_type: String },

    #[error(transparent)]
    FromTEdgeConfig(#[from] tedge_config::TEdgeConfigError),

    #[error(transparent)]
    FromConfigSetting(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    FromFile(#[from] FileError),
}
