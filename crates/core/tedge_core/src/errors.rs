#[derive(Debug, thiserror::Error)]
pub enum TedgeApplicationError {
    #[error("Plugin error")]
    Plugin(#[from] tedge_api::errors::PluginError),

    #[error("Plugin kind exists already: {0}")]
    PluginKindExists(String),

    #[error("The following Plugin kind are not covered in the configuration: {0}")]
    UnconfiguredPlugins(crate::utils::CommaSeperatedString),

    #[error("The following Plugin has no configuration: {0}")]
    PluginConfigMissing(String),

    #[error("Unknown Plugin kind: {0}")]
    UnknownPluginKind(String),

    #[error("Failed to send message")]
    TokioError(#[from] tokio::sync::mpsc::error::SendError<tedge_api::Message>),
}

pub type Result<T> = std::result::Result<T, TedgeApplicationError>;

