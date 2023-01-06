use std::io;
use tedge_utils::file::FileError;

#[derive(thiserror::Error, Debug)]
pub enum ConfigManagementError {
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

    #[error(transparent)]
    FromIoError(#[from] io::Error),
}
