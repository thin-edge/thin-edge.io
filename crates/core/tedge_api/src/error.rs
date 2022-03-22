use thiserror::Error;

#[derive(Error, Debug)]
pub enum RuntimeError {
    #[error("Send failed: the channel is closed")]
    SendError(#[from] futures::channel::mpsc::SendError),
}
