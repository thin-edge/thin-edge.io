use async_trait::async_trait;
use crate::error::PluginError;
use crate::message::Envelop;
use crate::message::Message;

/// The plugin configuration as a `toml::Spanned` table.
///
/// It is important that configuration errors are communicated precisely
/// and concisely. Reporting the span is not a must, but greatly helps users
/// in diagnostics of errors as well as sources of configuration.
pub type PluginConfiguration = toml::Spanned<toml::value::Value>;

/// A plugin builder for a given plugin
#[async_trait]
pub trait PluginBuilder: Sync + Send + 'static {
    type Request : Message;
    type Response : Message;

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
    ) -> Result<Box<dyn Plugin<Request=Self::Request, Response = Self::Response> + 'static>, PluginError>;
}

/// A functionality extension to ThinEdge
#[async_trait]
pub trait Plugin: Sync + Send {
    type Request : Message;
    type Response : Message;

    /// The plugin can set itself up here
    async fn setup(&mut self) -> Result<(), PluginError>;

    /// Handle a message specific to this plugin
    async fn handle_message(&self, message: Envelop<Self::Request>) -> Result<Envelop<Self::Response>, PluginError>;

    /// Gracefully handle shutdown
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}