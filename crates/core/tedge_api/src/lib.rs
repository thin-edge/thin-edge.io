#![deny(
    missing_docs,
    missing_debug_implementations,
    unreachable_pub,
    unsafe_code
)]
#![doc = include_str!("../README.md")]

/// All the parts required to write a plugin
pub mod plugin;
pub use plugin::{Message, Plugin, PluginBuilder, PluginConfiguration, PluginDirectory, PluginExt};

/// Generic representation of a configuration
pub mod config;
pub use config::{AsConfig, ConfigDescription, ConfigKind};

/// Addresses allow plugins to exchange messages
pub mod address;
pub use address::Address;

/// Known error types
pub mod error;
pub use error::PluginError;

/// Predefined messages
pub mod message;
pub use message::CoreMessages;

/// Cancellation token used by `tedge_api`
///
pub use tokio_util::sync::CancellationToken;

/// Derive macro for self-describing configurations
pub use tedge_config_derive::Config;

#[doc(hidden)]
pub mod _internal {
    pub use futures::future::BoxFuture;
}
