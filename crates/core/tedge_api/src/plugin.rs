//! Implementing a plugin is done in two steps:
//!
//! 1. Create a struct that implements `PluginBuilder`
//!     - Its purpose is to simply instantiate your plugins as needed with custom logic if required
//! 2. Create your plugin struct that implements `Plugin`

use async_trait::async_trait;

use crate::{error::PluginError, message::Message, Address, MessageKind};

/// The communication struct to interface with the core of ThinEdge
///
/// It's main purpose is the [`send`](CoreCommunication::send) method, through which one plugin
/// can communicate with another.
#[derive(Clone)]
pub struct CoreCommunication {
    plugin_name: String,
    sender: tokio::sync::mpsc::Sender<Message>,
}

impl std::fmt::Debug for CoreCommunication {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Comms")
            .field("plugin_name", &self.plugin_name)
            .finish_non_exhaustive()
    }
}

impl CoreCommunication {
    #[doc(hidden)]
    pub fn new(plugin_name: String, sender: tokio::sync::mpsc::Sender<Message>) -> Self {
        Self {
            plugin_name,
            sender,
        }
    }

    pub async fn send(
        &self,
        msg_kind: MessageKind,
        destination: Address,
    ) -> Result<(), PluginError> {
        let origin = Address::new(crate::address::EndpointKind::Plugin {
            id: self.plugin_name.clone(),
        });
        let msg = Message::new(origin, destination, msg_kind);
        self.sender.send(msg.into()).await?;

        Ok(())
    }
}

/// The plugin configuration as a `toml::Spanned` table.
///
/// It is important that configuration errors are communicated precisely
/// and concisely. Reporting the span is not a must, but greatly helps users
/// in diagnostics of errors as well as sources of configuration.
pub type PluginConfiguration = toml::Spanned<toml::value::Value>;

/// A plugin builder for a given plugin
#[async_trait]
pub trait PluginBuilder: Sync + Send + 'static {
    /// The a name for the kind of plugins this creates, this should be unique and will prevent startup otherwise
    fn kind_name(&self) -> &'static str;

    /// This may be called anytime to verify whether a plugin could be instantiated with the
    /// passed configuration.
    async fn verify_configuration(&self, config: &PluginConfiguration) -> Result<(), PluginError>;

    /// Instantiate a new instance of this plugin using the given configuration
    ///
    /// This _must not_ block
    async fn instantiate(
        &self,
        config: PluginConfiguration,
        core_comms: CoreCommunication,
    ) -> Result<Box<dyn Plugin + 'static>, PluginError>;
}

/// A functionality extension to ThinEdge
#[async_trait]
pub trait Plugin: Sync + Send {
    /// The plugin can set itself up here
    async fn setup(&mut self) -> Result<(), PluginError>;

    /// Handle a message specific to this plugin
    async fn handle_message(&self, message: Message) -> Result<(), PluginError>;

    /// Gracefully handle shutdown
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}

#[cfg(test)]
mod tests {
    use super::{CoreCommunication, Plugin, PluginBuilder};
    use static_assertions::{assert_impl_all, assert_obj_safe};

    // Object Safety
    assert_obj_safe!(PluginBuilder);
    assert_obj_safe!(Plugin);

    // Sync + Send
    assert_impl_all!(CoreCommunication: Send, Clone);
}
