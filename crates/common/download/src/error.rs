use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error(transparent)]
    FromBackoff(#[from] backoff::Error<reqwest::Error>),

    #[error(transparent)]
    FromElapsed(#[from] tokio::time::error::Elapsed),

    #[error("I/O error: {reason:?}")]
    FromIo { reason: String },

    #[error("JSON parse error: {reason:?}")]
    JsonParse { reason: String },

    #[error(transparent)]
    FromUrlParse(#[from] url::ParseError),

    #[error(transparent)]
    FromNix(#[from] nix::Error),

    #[error(transparent)]
    FromFileError(#[from] tedge_utils::file::FileError),

    #[error("Not enough disk space")]
    InsufficientSpace,

    #[error("No write access to {path:?}")]
    NoWriteAccess { path: PathBuf },

    #[error("From reqwest")]
    FromReqwest(#[from] reqwest::Error),
}

impl From<std::io::Error> for DownloadError {
    fn from(err: std::io::Error) -> Self {
        DownloadError::FromIo {
            reason: format!("{}", err),
        }
    }
}
