use thiserror::Error;
use tokio::task::JoinError;

/// A wrapper for errors raised from actors to the runtime.
///
/// Boxing is required as the errors of different actors would be of different sizes.
pub type DynError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Error raised while exchanging messages
#[derive(Error, Debug)]
pub enum ChannelError {
    #[error("Fail to send a message: the receiver has been dropped")]
    SendError(#[from] futures::channel::mpsc::SendError),

    #[error("Fail to receive a message: the sender has been dropped")]
    ReceiveError(),
}

/// Error raised during runtime by actors as well as the runtime
#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error(transparent)]
    ActorError(#[from] DynError),

    #[error("Fail to send a message to the runtime: the runtime has been dropped")]
    RuntimeSendError(#[from] futures::channel::mpsc::SendError),

    #[error("Fail to send a message: the peer has been dropped")]
    ChannelError(#[from] ChannelError),

    #[error("The runtime has been cancelled")]
    RuntimeCancellation,

    #[error("The runtime panicked")]
    RuntimePanic,

    #[error(transparent)]
    JoinError(#[from] JoinError),

    #[error(transparent)]
    LinkError(#[from] LinkError),
}

impl<T> From<Box<T>> for RuntimeError
where
    T: std::error::Error + Send + Sync + 'static,
{
    fn from(error: Box<T>) -> Self {
        RuntimeError::ActorError(error)
    }
}

/// Error raised while connecting actor instances
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    #[error("Missing peer for {role}")]
    MissingPeer { role: String },

    #[error("Extra peer for {role}")]
    ExcessPeer { role: String },
}
