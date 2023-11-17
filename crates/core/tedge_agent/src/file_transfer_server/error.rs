use axum::extract::rejection::PathRejection;
use axum::response::IntoResponse;
use hyper::StatusCode;
use tedge_actors::RuntimeError;

use super::request_files::RequestPath;

#[derive(Debug, thiserror::Error)]
pub enum FileTransferError {
    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromHyperError(#[from] hyper::Error),

    #[error(transparent)]
    FromAddressParseError(#[from] std::net::AddrParseError),

    #[error(transparent)]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    #[error("Could not bind to address: {address}. Address already in use.")]
    BindingAddressInUse { address: std::net::SocketAddr },
}

#[derive(Debug, thiserror::Error)]
pub enum FileTransferRequestError {
    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error("Request to delete {path:?} failed: {source}")]
    DeleteIoError {
        source: std::io::Error,
        path: RequestPath,
    },

    #[error("Request to upload to {path:?} failed: {source:?}")]
    Upload {
        source: anyhow::Error,
        path: RequestPath,
    },

    #[error("Invalid file path: {path:?}")]
    InvalidPath { path: RequestPath },

    #[error("File not found: {0:?}")]
    FileNotFound(RequestPath),

    #[error("Path rejection: {0:?}")]
    PathRejection(#[from] PathRejection),
}

impl From<FileTransferError> for RuntimeError {
    fn from(error: FileTransferError) -> Self {
        RuntimeError::ActorError(Box::new(error))
    }
}

impl IntoResponse for FileTransferRequestError {
    fn into_response(self) -> axum::response::Response {
        use FileTransferRequestError::*;
        let error_message = self.to_string();
        match self {
            PathRejection(err) => {
                tracing::error!("{error_message}");
                err.into_response()
            }
            FromIo(_) => {
                tracing::error!("{error_message}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal error".to_owned(),
                )
                    .into_response()
            }
            DeleteIoError { path, .. } => {
                tracing::error!("{error_message}");
                (
                    StatusCode::FORBIDDEN,
                    format!("Cannot delete path {path:?}"),
                )
                    .into_response()
            }
            Upload { path, .. } => {
                tracing::error!("{error_message}");
                (
                    StatusCode::FORBIDDEN,
                    format!("Cannot upload to path {path:?}"),
                )
                    .into_response()
            }
            InvalidPath { .. } | FileNotFound(_) => {
                (StatusCode::NOT_FOUND, self.to_string()).into_response()
            }
        }
    }
}
