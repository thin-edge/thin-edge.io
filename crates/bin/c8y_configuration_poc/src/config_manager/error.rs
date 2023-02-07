use mqtt_channel::Topic;
use std::io;
use tedge_utils::file::FileError;

#[derive(thiserror::Error, Debug)]
pub enum ConfigManagementError {
    #[error(
        "The requested config_type {config_type} is not defined in the plugin configuration file."
    )]
    InvalidRequestedConfigType { config_type: String },

    #[error("Message received on invalid topic from child device: {topic}")]
    InvalidChildDeviceTopic { topic: String },

    #[error("Invalid operation response with empty status received on topic: {0:?}")]
    EmptyOperationStatus(Topic),

    #[error(transparent)]
    FromTEdgeConfig(#[from] tedge_config::TEdgeConfigError),

    #[error(transparent)]
    FromConfigSetting(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    FromFile(#[from] FileError),

    #[error(transparent)]
    FromIoError(#[from] io::Error),

    #[error(transparent)]
    FromMqttError(#[from] mqtt_channel::MqttError),

    #[error(transparent)]
    FromSmartRestSerializerError(#[from] c8y_api::smartrest::error::SmartRestSerializerError),

    #[error("Failed to parse response from child device with: {0}")]
    FromSerdeJsonError(#[from] serde_json::Error),
}
