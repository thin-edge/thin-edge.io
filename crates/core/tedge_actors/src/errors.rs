use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ChannelError {
    #[error("Fail to send a message: the peer has been dropped")]
    SendError(#[from] futures::channel::mpsc::SendError),
}

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
}
