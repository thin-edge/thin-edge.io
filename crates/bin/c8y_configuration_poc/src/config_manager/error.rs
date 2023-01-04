use mqtt_channel::Topic;
use std::io;
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

    #[error(transparent)]
    FromIoError(#[from] io::Error),
}

#[allow(clippy::large_enum_variant)]
#[derive(thiserror::Error, Debug)]
pub enum ChildDeviceConfigManagementError {
    #[error("Invalid topic received from child device: {topic}")]
    InvalidTopicFromChildOperation { topic: String },

    #[error("Invalid operation response with empty status received on topic: {0:?}")]
    EmptyOperationStatus(Topic),

    #[error(transparent)]
    FromMqttError(#[from] mqtt_channel::MqttError),

    #[error("Failed to parse response from child device with: {0}")]
    FromSerdeJsonError(#[from] serde_json::Error),

    #[error(transparent)]
    FromSmartRestSerializerError(#[from] c8y_api::smartrest::error::SmartRestSerializerError),

    #[error(transparent)]
    FromIoError(#[from] io::Error),

    #[error(transparent)]
    FromFile(#[from] FileError),
}
