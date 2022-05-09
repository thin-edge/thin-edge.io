use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum ConfigManagementError {
    #[error("The file is read-only {path:?}")]
    ReadOnlyFile { path: PathBuf },

    #[error("The file name is not found from {path:?}")]
    FileNameNotFound { path: PathBuf },

    #[error("The file name is invalid. {path:?}")]
    InvalidFileName { path: PathBuf },

    #[error("The file is not accessible. {path:?}")]
    FileNotAccessible { path: PathBuf },

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
}
