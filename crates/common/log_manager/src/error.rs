#[derive(thiserror::Error, Debug)]
pub enum LogRetrievalError {
    #[error(transparent)]
    FromStdIo(#[from] std::io::Error),

    #[error(transparent)]
    FromGlobPatternError(#[from] glob::PatternError),

    #[error(transparent)]
    FromGlobError(#[from] glob::GlobError),

    #[error(transparent)]
    FromPathsError(#[from] tedge_utils::paths::PathsError),

    #[error(transparent)]
    FromFileError(#[from] tedge_utils::file::FileError),

    // NOTE: `MaxLines` is not a client-facing error. It is used
    // to break out of `read_log_content`.
    #[error("Log file has maximum number of lines.")]
    MaxLines,

    #[error("No logs found for log type {log_type:?}")]
    NoLogsAvailableForType { log_type: String },
}
