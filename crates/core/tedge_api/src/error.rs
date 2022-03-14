use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("The sender could not transfer the message to its receiving end. Did it get closed?")]
    CouldNotSendMessage(#[from] tokio::sync::mpsc::error::SendError<crate::Message>),
    #[error("An error in the configuration was found")]
    Configuration(#[from] toml::de::Error),
    #[error(transparent)]
    Custom(#[from] anyhow::Error),
}
