#[derive(Debug, thiserror::Error)]
pub enum UpdaterError {
    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error("Command returned non 0 exit code.")]
    CommandFailed,

    #[error("Failed an operation related to update: {reason}, when spawning {process}")]
    ProcessError { reason: String, process: String },
}
