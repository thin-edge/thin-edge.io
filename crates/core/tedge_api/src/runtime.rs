use crate::*;
use async_trait::async_trait;

/// The configuration for a plugin instance
pub trait PluginConfig {
    type Plugin: Plugin;

    /// Create a plugin from the config
    fn instantiate(self) -> Result<Self::Plugin, RuntimeError>;
}

/// A plugin instance
#[async_trait]
pub trait Plugin {
    /// The type of Input messages this plugin can process
    type Input;

    /// The address where messages for this plugin can be sent
    fn get_address(&self) -> Address<Self::Input>;

    /// Start the plugin in the background
    async fn start(self) -> Result<(), RuntimeError>;
}

/// Empty enum used by plugin that consumes no messages.
pub enum NoInput {}
