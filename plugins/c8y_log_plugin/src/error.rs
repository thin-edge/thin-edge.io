#[derive(thiserror::Error, Debug)]
pub enum LogRetrievalError {
    #[error(transparent)]
    FromTEdgeConfig(#[from] tedge_config::TEdgeConfigError),

    #[error(transparent)]
    FromConfigSetting(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    FromStdIo(#[from] std::io::Error),

    #[error(transparent)]
    FromGlobPatternError(#[from] glob::PatternError),

    #[error(transparent)]
    FromGlobError(#[from] glob::GlobError),

    // NOTE: `MaxLines` is not a client-facing error. It is used
    // to break out of `read_log_content`.
    #[error("Log file has maximum number of lines.")]
    MaxLines,

    #[error("No logs available for log type: {log_type}. Hint: is your `path` key correct?")]
    NoLogsAvailableForType { log_type: String },
}
