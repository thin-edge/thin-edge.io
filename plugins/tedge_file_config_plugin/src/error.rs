use std::io;
use std::path::PathBuf;

use camino::Utf8PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("The requested config_type {0} is not defined in the plugin configuration file.")]
    InvalidConfigType(String),

    #[error("Config file not found: {0}")]
    FileNotFound(Utf8PathBuf),

    #[error("Failed to parse TOML config: {0}")]
    TomlError(#[from] toml::de::Error),

    #[error("Failed to create parent directory: {path}")]
    ParentDirCreationFailed { path: PathBuf, source: io::Error },

    #[error("Failed to set file permissions: {path}")]
    PermissionError { path: PathBuf, source: io::Error },

    #[error("Service {0} is not running")]
    ServiceNotRunning(String),

    #[error(transparent)]
    AnyhowError(#[from] anyhow::Error),
}
