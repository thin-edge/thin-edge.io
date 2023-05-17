use super::download::InvalidResponseError;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("I/O error")]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromUrlParse(#[from] url::ParseError),

    #[error(transparent)]
    FromFileError(#[from] tedge_utils::file::FileError),

    #[error("Not enough disk space")]
    InsufficientSpace,

    #[error("No write access to {path:?}")]
    NoWriteAccess { path: PathBuf },

    #[error("From reqwest")]
    FromReqwest(#[from] reqwest::Error),

    #[error("Invalid server response")]
    InvalidResponse(#[from] InvalidResponseError),
}

impl From<nix::Error> for DownloadError {
    fn from(err: nix::Error) -> Self {
        DownloadError::FromIo(err.into())
    }
}
