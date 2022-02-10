use thiserror::Error;

#[derive(Error, Debug)]
#[error("An error occured while interacting with this plugin")]
pub enum PluginError {
    #[error("The sender could not transfer the message to its receiving end. Did it get closed?")]
    CouldNotSendMessage(#[from] tokio::sync::mpsc::error::SendError<crate::Message>),
    Configuration(#[from] toml::de::Error),
}

