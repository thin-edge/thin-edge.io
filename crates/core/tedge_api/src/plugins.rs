//! Implementing a plugin is done in two steps:
//!
//! 1. Create a struct that implements `PluginBuilder`
//!     - Its purpose is to simply instantiate your plugins as needed with custom logic if required
//! 2. Create your plugin struct that implements `Plugin`

use async_trait::async_trait;

use crate::errors::{PluginConfigurationError, PluginError};

#[derive(Clone)]
pub struct Comms {
    sender: tokio::sync::mpsc::Sender<CoreMessage>,
}

impl Comms {
    pub const fn new(sender: tokio::sync::mpsc::Sender<CoreMessage>) -> Self {
        Self { sender }
    }

    pub async fn send(&self, msg: CoreMessage) -> Result<(), ()> {
        todo!("Send this message: {:?}", msg)
    }
}

/// An address which could be either a target or source of messages
///
/// Nesting addresses allows to disambiguated between different kind of
/// sources and the way they have arrived here.
#[derive(Debug, Clone)]
pub struct Address {
    endpoint: EndpointKind,
    source: Option<Box<Address>>,
}

impl Address {
    /// Get the original source of an `Address`
    pub fn origin(&self) -> &Address {
        if let Some(source) = self.source.as_ref() {
            source.origin()
        } else {
            self
        }
    }

    pub fn add_new_step(&self, endpoint: EndpointKind) -> Self {
        Self {
            endpoint,
            source: Some(Box::new(self.clone())),
        }
    }
}

/// What kind of endpoint is it
#[derive(Debug, Clone)]
pub enum EndpointKind {
    /// The `tedge` core
    Core,
    /// A specific plugin
    Plugin { id: String },
}

/// A message to be received by the `tedge` core component
///
/// It will be internally routed according to its destination
#[derive(Debug)]
pub struct CoreMessage {
    destination: Address,
    content: CoreMessageKind,
}

#[derive(Debug)]
pub enum CoreMessageKind {
    SendGenericMessage { message: Vec<u8> },
    SignalPluginState { state: String },
    // etc...
}

/// A message to be handled by a plugin
#[derive(Debug)]
pub struct PluginMessage {
    origin: Address,
    content: PluginMessageKind,
}

#[derive(Debug)]
pub enum PluginMessageKind {
    /// The plugin is being asked if it is currently able to respond
    /// to requests. It is meant to reply with `CoreMessageKind` stating
    /// its status.
    CheckReadyness,
}

/// The plugin configuration as a `toml::Spanned` table.
///
/// It is important that configuration errors are communicated precisely
/// and concisely. Reporting the span is not a must, but greatly helps users
/// in diagnostics of errors as well as sources of configuration.
type PluginConfiguration = toml::Spanned<toml::value::Table>;

/// A plugin builder for a given plugin
pub trait PluginBuilder: Sync + Send + 'static {
    /// The name of the plugins this creates, this should be unique and will prevent startup otherwise
    fn name(&self) -> &'static str;

    /// This may be called anytime to verify whether a plugin could be instantiated with the
    /// passed configuration.
    fn verify_configuration(
        &self,
        config: PluginConfiguration,
    ) -> Result<(), PluginConfigurationError>;

    /// Instantiate a new instance of this plugin using the given configuration
    ///
    /// This _must not_ block
    fn instantiate(
        &self,
        config: PluginConfiguration,
        tedge_comms: Comms,
    ) -> Box<dyn Plugin + 'static>;
}

/// A functionality extension to ThinEdge
#[async_trait]
pub trait Plugin: Sync + Send {
    /// The plugin can set itself up here
    async fn setup(&mut self) -> Result<(), PluginError>;

    /// Handle a message specific to this plugin
    async fn handle_message(&self, message: PluginMessage) -> Result<(), PluginError>;

    /// Gracefully handle shutdown
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}

#[cfg(test)]
mod tests {
    use super::{Comms, Plugin, PluginBuilder, PluginMessage};
    use static_assertions::{assert_impl_all, assert_obj_safe};

    // Object Safety
    assert_obj_safe!(PluginBuilder);
    assert_obj_safe!(Plugin);

    // Sync + Send
    assert_impl_all!(Comms: Send, Clone);
    assert_impl_all!(PluginMessage: Send);
}
