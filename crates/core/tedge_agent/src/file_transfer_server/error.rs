use axum::response::IntoResponse;
use camino::Utf8PathBuf;
use hyper::StatusCode;
use tedge_actors::RuntimeError;

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

    #[error("Invalid URI: {value:?}")]
    InvalidURI { value: String },

    #[error("File not found: {0:?}")]
    FileNotFound(Utf8PathBuf),
}

impl From<FileTransferError> for RuntimeError {
    fn from(error: FileTransferError) -> Self {
        RuntimeError::ActorError(Box::new(error))
    }
}

impl IntoResponse for FileTransferError {
    fn into_response(self) -> axum::response::Response {
        use FileTransferError::*;
        let status_code = match self {
            // TODO split out errors into startup and runtime errors
            FromIo(_)
            | FromHyperError(_)
            | FromAddressParseError(_)
            | FromUtf8Error(_)
            | BindingAddressInUse { .. } => {
                tracing::error!("{self}");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        status_code.into_response()
    }
}

impl IntoResponse for FileTransferRequestError {
    fn into_response(self) -> axum::response::Response {
        use FileTransferRequestError::*;
        match self {
            FromIo(_) => {
                tracing::error!("{self}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal error".to_owned(),
                )
            }
            InvalidURI { .. } | FileNotFound(_) => (StatusCode::NOT_FOUND, self.to_string()),
        }
        .into_response()
    }
}
