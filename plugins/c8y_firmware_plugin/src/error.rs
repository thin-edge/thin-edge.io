#[allow(clippy::large_enum_variant)]
#[derive(thiserror::Error, Debug)]
pub enum FirmwareManagementError {
    #[error("Invalid topic received from child device: {topic}")]
    InvalidTopicFromChildOperation { topic: String },

    #[error("Failed to copy a file from {src:?} to {dest:?}")]
    FileCopyFailed {
        src: std::path::PathBuf,
        dest: std::path::PathBuf,
    },

    #[error("No corresponding server URL is found with the local URL.")]
    InvalidLocalURL { url: String },

    #[error(transparent)]
    FromMqttError(#[from] mqtt_channel::MqttError),

    #[error("Failed to parse response from child device with: {0}")]
    FromSerdeJsonError(#[from] serde_json::Error),

    #[error(transparent)]
    FromSmartRestSerializerError(#[from] c8y_api::smartrest::error::SmartRestSerializerError),

    #[error(transparent)]
    FromIoError(#[from] std::io::Error),
}
