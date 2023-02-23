#[allow(clippy::large_enum_variant)]
#[derive(thiserror::Error, Debug)]
pub enum FirmwareManagementError {
    #[error("Invalid topic received from child device: {topic}")]
    InvalidTopicFromChildOperation { topic: String },

    #[error("Failed to copy a file from {src} to {dest}")]
    FileCopyFailed {
        src: std::path::PathBuf,
        dest: std::path::PathBuf,
    },

    #[error(
        "Directory {path} is not found. Run 'c8y-firmware-plugin --init' to create the directory."
    )]
    DirectoryNotFound { path: std::path::PathBuf },

    #[error("The received SmartREST request is duplicated with already addressed operation. Ignore this request.")]
    RequestAlreadyAddressed,

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

    #[error(transparent)]
    FromSMCumulocityMapperError(#[from] c8y_api::smartrest::error::SMCumulocityMapperError),

    #[error(transparent)]
    FromSystemServiceError(#[from] tedge_config::system_services::SystemServiceError),

    #[error(transparent)]
    FromTEdgeConfigError(#[from] tedge_config::TEdgeConfigError),

    #[error(transparent)]
    FromConfigSettingError(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    FromSendError(#[from] futures::channel::mpsc::SendError),
}
