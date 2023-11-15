use axum::response::IntoResponse;
use hyper::StatusCode;
use tedge_actors::RuntimeError;

#[derive(Debug, thiserror::Error)]
pub enum FileTransferError {
    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromHyperError(#[from] hyper::Error),

    #[error("Invalid URI: {value:?}")]
    InvalidURI { value: String },

    #[error(transparent)]
    FromAddressParseError(#[from] std::net::AddrParseError),

    #[error(transparent)]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    #[error("Could not bind to address: {address}. Address already in use.")]
    BindingAddressInUse { address: std::net::SocketAddr },
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
            InvalidURI { .. } => StatusCode::NOT_FOUND,
        };
        status_code.into_response()
    }
}
