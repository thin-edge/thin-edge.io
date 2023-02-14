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

    #[error("Operation status file must have mode 644 and owned by tedge or root. Ignore operation={id}.")]
    InvalidFilePermission { id: String },

    #[error("Persistent file is invalid. File path={path}")]
    PersistentStoreError { path: std::path::PathBuf },

    #[error(transparent)]
    FromMqttError(#[from] mqtt_channel::MqttError),

    #[error("Failed to parse response from child device with: {0}")]
    FromSerdeJsonError(#[from] serde_json::Error),

    #[error(transparent)]
    FromSmartRestSerializerError(#[from] c8y_api::smartrest::error::SmartRestSerializerError),

    #[error(transparent)]
    FromIoError(#[from] std::io::Error),

    #[error(transparent)]
    FromFileError(#[from] tedge_utils::file::FileError),
}
