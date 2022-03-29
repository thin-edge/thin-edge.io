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

    // /// Send a request to a peer
    // async fn send_request_to<Req, Res: Message + Into<Self::Input>>(
    //     &self,
    //     peer: &mut impl Recipient<Request<Req, Res>>,
    //     request: Req,
    // ) -> Result<(), RuntimeError>
    // where
    //     Res: Into<Self::Input>,
    // {
    //     self.get_address().send_request_to(peer, request).await?;
    //     Ok(())
    // }
}

/// Empty enum used by plugin that consumes no messages.
pub enum NoInput {}
