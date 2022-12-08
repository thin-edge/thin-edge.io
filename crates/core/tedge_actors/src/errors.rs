use thiserror::Error;

/// Error raised while exchanging messages
#[derive(Error, Debug, Clone)]
pub enum ChannelError {
    #[error("Fail to send a message: the receiver has been dropped")]
    SendError(#[from] futures::channel::mpsc::SendError),

    #[error("Fail to receive a message: the sender has been dropped")]
    ReceiveError(),
}

/// Error raised by the runtime
#[derive(Error, Debug, Clone)]
pub enum RuntimeError {
    #[error("Fail to send a message to the runtime: the runtime has been dropped")]
    RuntimeSendError(#[from] futures::channel::mpsc::SendError),

    #[error("Fail to send a message: the peer has been dropped")]
    ChannelError(#[from] ChannelError),

    #[error("The runtime has been cancelled")]
    RuntimeCancellation,

    #[error("The runtime panicked")]
    RuntimePanic,

    #[error(transparent)]
    LinkError(#[from] LinkError),
}

/// Error raised while connecting actor instances
#[derive(Error, Debug, Clone)]
pub enum LinkError {
    #[error("Missing peer for {role}")]
    MissingPeer { role: String },

    #[error("Extra peer for {role}")]
    ExcessPeer { role: String },
}
