#[allow(clippy::enum_variant_names)]
#[derive(thiserror::Error, Debug)]

pub enum LogManagementError {
    #[error(transparent)]
    FromStdIoError(#[from] std::io::Error),

    #[error(transparent)]
    FromMqttError(#[from] tedge_mqtt_ext::MqttError),

    #[error("Failed to parse response with: {0}")]
    FromSerdeJsonError(#[from] serde_json::Error),

    #[error(transparent)]
    FromChannelError(#[from] tedge_actors::ChannelError),

    #[error(transparent)]
    FromPathsError(#[from] tedge_utils::paths::PathsError),

    #[error(transparent)]
    FromLogRetrievalError(#[from] crate::manager::LogRetrievalError),
}

impl From<LogManagementError> for tedge_actors::RuntimeError {
    fn from(error: LogManagementError) -> Self {
        tedge_actors::RuntimeError::ActorError(Box::new(error))
    }
}
