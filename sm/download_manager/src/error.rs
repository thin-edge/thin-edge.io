use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, thiserror::Error, PartialEq)]
pub enum DownloadError {
    // #[error(transparent)]
    // FromIo(#[from] std::io::Error),
    #[error("I/O error: {reason:?}")]
    FromIo { reason: String },

    #[error("JSON parse error: {reason:?}")]
    FromReqwest { reason: String },
    // #[error(transparent)]
    // FromReqwest(#[from] reqwest::Error),
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
