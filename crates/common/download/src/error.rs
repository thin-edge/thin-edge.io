use super::download::InvalidResponseError;
use std::io;
use std::path::PathBuf;

/// An error that can be returned as a result of
/// [`Downloader::download`](super::download::Downloader::download) operation.
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("{context}")]
    FromIo { context: String, source: io::Error },

    #[error("Error while performing a file operation")]
    FromFileError(#[from] tedge_utils::file::FileError),

    #[error("Not enough disk space")]
    InsufficientSpace,

    #[error("No write access to {path:?}")]
    NoWriteAccess { path: PathBuf },

    #[error("Could not make a successful request to the remote server")]
    Request(#[from] reqwest::Error),

    #[error("Invalid server response")]
    InvalidResponse(#[from] InvalidResponseError),
}

/// A trait for attaching context string to io-like errors.
///
/// While using thiserror, it is very easy to create a variant like
/// `FromIo(#[from] io::Error)` and convert io errors to this variant using the
/// `?` operator. This however loses helpful context, e.g. the actual path
/// related to the error (because io::Error does not provide it) or information
/// about what an application tried to do when io::Error was returned.
///
/// [`anyhow`] makes it really easy to attach context to errors by using
/// [`anyhow::Context::context`], but as we want to use typed errors, just not
/// to repeat `.map_err(|err| DownloadError::FromIo { context: "...", source:
/// err })` everywhere, this helper trait provides convenient `anyhow`-like
/// syntax to easily attach context.
pub(crate) trait ErrContext<T> {
    fn context(self, context: String) -> Result<T, DownloadError>;
}

impl<T, E: Into<io::Error>> ErrContext<T> for Result<T, E> {
    fn context(self, context: String) -> Result<T, DownloadError> {
        self.map_err(|err| DownloadError::FromIo {
            context,
            source: err.into(),
        })
    }
}
