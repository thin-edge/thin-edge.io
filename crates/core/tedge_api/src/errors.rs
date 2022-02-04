use thiserror::Error;

#[derive(Debug)]
pub struct PluginConfigurationError {
    kind: PluginConfigurationErrorKind,
    span: Option<(usize, usize)>,
}

impl PluginConfigurationError {
    pub const fn new(span: (usize, usize), kind: PluginConfigurationErrorKind) -> Self {
        Self {
            span: Some(span),
            kind,
        }
    }

    /// Get a reference to the plugin configuration error's kind.
    pub fn kind(&self) -> &PluginConfigurationErrorKind {
        &self.kind
    }

    /// Get the plugin configuration error's span.
    pub fn span(&self) -> Option<(usize, usize)> {
        self.span
    }
}

impl From<PluginConfigurationErrorKind> for PluginConfigurationError {
    fn from(kind: PluginConfigurationErrorKind) -> Self {
        Self { span: None, kind }
    }
}

#[derive(Error, Debug)]
pub enum PluginConfigurationErrorKind {}

#[derive(Error, Debug)]
#[error("An error occured while interacting with this plugin")]
pub enum PluginError {
    #[error("The sender could not transfer the message to its receiving end. Did it get closed?")]
    CouldNotSendMessage(#[from] tokio::sync::mpsc::error::SendError<crate::CoreMessage>),
}
