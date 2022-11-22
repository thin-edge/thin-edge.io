use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ChannelError {
    #[error("Receiver peer has been dropped")]
    DroppedPeer(#[from] futures::channel::mpsc::SendError),
}
