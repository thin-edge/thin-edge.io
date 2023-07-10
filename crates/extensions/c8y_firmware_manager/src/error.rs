use tedge_actors::RuntimeError;

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

    #[error("Failed to retrieve JWT token.")]
    NoJwtToken,

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
    FromChannelError(#[from] tedge_actors::ChannelError),

    #[error(transparent)]
    FromMqttError(#[from] tedge_mqtt_ext::MqttError),
}

impl From<FirmwareManagementError> for RuntimeError {
    fn from(error: FirmwareManagementError) -> Self {
        RuntimeError::ActorError(Box::new(error))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FirmwareManagementConfigBuildError {
    #[error(transparent)]
    FromReadError(#[from] tedge_config::new::ReadError),

    #[error(transparent)]
    FromConfigNotSet(#[from] tedge_config::new::ConfigNotSet),
}
