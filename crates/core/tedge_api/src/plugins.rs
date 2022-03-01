//! Implementing a plugin is done in two steps:
//!
//! 1. Create a struct that implements `PluginBuilder`
//!     - Its purpose is to simply instantiate your plugins as needed with custom logic if required
//! 2. Create your plugin struct that implements `Plugin`

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::{errors::PluginError, messages::Message};

type ReplySender = tokio::sync::oneshot::Sender<Message>;
type ReplyReceiver = tokio::sync::oneshot::Receiver<Message>;

#[derive(Clone)]
pub struct Comms {
    plugin_name: String,
    sender: tokio::sync::mpsc::Sender<Message>,
    replymap: Arc<RwLock<HashMap<uuid::Uuid, ReplySender>>>,
}

impl std::fmt::Debug for Comms {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("Comms")
            .field("plugin_name", &self.plugin_name)
            .finish()
    }
}

impl Comms {
    pub fn new(plugin_name: String, sender: tokio::sync::mpsc::Sender<Message>) -> Self {
        Self {
            plugin_name,
            sender,
            replymap: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn new_message(&self, destination: crate::Address, kind: crate::MessageKind) -> Message {
        let addr = crate::Address::new(crate::address::EndpointKind::Plugin { id: self.plugin_name.clone() });
        Message::new(addr, destination, kind)
    }

    pub async fn send<T: Into<Message>>(&self, msg: T) -> Result<(), PluginError> {
        self.sender.send(msg.into()).await?;

        Ok(())
    }

    pub async fn send_and_wait_for_reply<T: Into<Message>>(&self, msg: T) -> Result<tokio::sync::oneshot::Receiver<Message>, PluginError> {
        let msg = msg.into();
        let mut map = self.replymap.write().await;
        let (tx, rx) = tokio::sync::oneshot::channel();
        map.insert(msg.id().clone(), tx);
        self.send(msg).await.map(|_| rx)

    }

    /// Process a message that could be a reply
    ///
    /// # Returns
    ///
    /// * Ok(Some(Message)) if the message was not handled
    /// * Ok(None) if the message was handled
    /// * Err(_) in case of error
    ///
    pub async fn handle_reply(&self, msg: Message) -> Result<Option<Message>, PluginError> {
        if let Some(sender) = self.replymap.write().await.remove(msg.id()) {
            match sender.send(msg) {
                Ok(()) => Ok(None),
                Err(msg) => Ok(Some(msg)), // TODO: Is this the right way?
            }
        } else {
            Ok(Some(msg))
        }
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
        tedge_comms: Comms,
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
    use super::{Comms, Plugin, PluginBuilder};
    use static_assertions::{assert_impl_all, assert_obj_safe};

    // Object Safety
    assert_obj_safe!(PluginBuilder);
    assert_obj_safe!(Plugin);

    // Sync + Send
    assert_impl_all!(Comms: Send, Clone);
}
