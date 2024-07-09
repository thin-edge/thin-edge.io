#[derive(Debug, thiserror::Error)]
#[allow(clippy::enum_variant_names)]
pub enum RestartManagerError {
    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error("Command returned non 0 exit code: {command}")]
    CommandFailed { command: String },

    #[error("Failed parsing /proc/uptime")]
    UptimeParserError,

    #[error("Failed to cast string to float.")]
    FloatCastingError,

    #[error(transparent)]
    FromSystemServices(#[from] tedge_config::system_services::SystemServiceError),

    #[error(transparent)]
    FromState(#[from] crate::state_repository::error::StateError),

    #[error("Could not convert {timestamp:?} to unix timestamp. Error message: {error_msg}")]
    TimestampConversionError { timestamp: i64, error_msg: String },

    #[error(transparent)]
    FromAtomFileError(#[from] tedge_utils::fs::AtomFileError),
}
