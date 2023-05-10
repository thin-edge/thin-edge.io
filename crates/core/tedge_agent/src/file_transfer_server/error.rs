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
    FromRouterServer(#[from] routerify::RouteError),

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
