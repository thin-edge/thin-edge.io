use axum::extract::rejection::PathRejection;
use axum::response::IntoResponse;
use hyper::StatusCode;
use tedge_actors::RuntimeError;

use super::request_files::RequestPath;

#[derive(Debug, thiserror::Error)]
pub(crate) enum HttpServerError {
    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromHyperError(#[from] hyper::Error),

    #[error(transparent)]
    FromAddressParseError(#[from] std::net::AddrParseError),

    #[error(transparent)]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum HttpRequestError {
    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error("Cannot delete: {path:?} is a directory, not a file")]
    CannotDeleteDirectory { path: RequestPath },

    #[error("Cannot upload: {path:?} is a directory, not a file")]
    CannotUploadDirectory { path: RequestPath },

    #[error("Request to delete {path:?} failed: {source}")]
    Delete {
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

    #[error("Path rejection: {0}")]
    PathRejection(#[from] PathRejection),
}

impl From<HttpServerError> for RuntimeError {
    fn from(error: HttpServerError) -> Self {
        RuntimeError::ActorError(Box::new(error))
    }
}

impl IntoResponse for HttpRequestError {
    fn into_response(self) -> axum::response::Response {
        use HttpRequestError as E;
        let error_message = self.to_string();
        match self {
            E::PathRejection(err) => {
                tracing::error!("{error_message}");
                err.into_response()
            }
            E::FromIo(_) | E::Delete { .. } | E::Upload { .. } => {
                tracing::error!("{error_message}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal error".to_owned(),
                )
                    .into_response()
            }
            // All of these from an invalid URL, so `Not Found` is most appropriate response
            E::InvalidPath { .. } | E::FileNotFound(_) | E::CannotDeleteDirectory { .. } => {
                (StatusCode::NOT_FOUND, error_message).into_response()
            }
            E::CannotUploadDirectory { .. } => {
                (StatusCode::CONFLICT, error_message).into_response()
            }
        }
    }
}
