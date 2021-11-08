#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error(transparent)]
    FromBackoff(#[from] backoff::Error<reqwest::Error>),

    #[error(transparent)]
    FromElapsed(#[from] tokio::time::error::Elapsed),

    #[error("I/O error: {reason:?}")]
    FromIo { reason: String },

    #[error("JSON parse error: {reason:?}")]
    FromReqwest { reason: String },

    #[error(transparent)]
    FromUrlParse(#[from] url::ParseError),

    #[error(transparent)]
    FromNix(#[from] nix::Error),
}

impl From<reqwest::Error> for DownloadError {
    fn from(err: reqwest::Error) -> Self {
        DownloadError::FromReqwest {
            reason: format!("{}", err),
        }
    }
}

impl From<std::io::Error> for DownloadError {
    fn from(err: std::io::Error) -> Self {
        DownloadError::FromIo {
            reason: format!("{}", err),
        }
    }
}
