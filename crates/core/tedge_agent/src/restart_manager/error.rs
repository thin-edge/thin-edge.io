#[derive(Debug, thiserror::Error)]
#[allow(clippy::enum_variant_names)]
pub enum RestartManagerError {
    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error("Command returned non 0 exit code.")]
    CommandFailed,

    #[error("Failed parsing /proc/uptime")]
    UptimeParserError,

    #[error("Failed to cast string to float.")]
    FloatCastingError,

    #[error(transparent)]
    FromSystemServices(#[from] tedge_config::system_services::SystemServiceError),

    #[error(transparent)]
    FromChannelError(#[from] tedge_actors::ChannelError),

    #[error(transparent)]
    FromState(#[from] crate::state_repository::error::StateError),

    #[error("Could not convert {timestamp:?} to unix timestamp. Error message: {error_msg}")]
    TimestampConversionError { timestamp: i64, error_msg: String },
}

impl From<RestartManagerError> for tedge_actors::RuntimeError {
    fn from(error: RestartManagerError) -> Self {
        tedge_actors::RuntimeError::ActorError(Box::new(error))
    }
}
