pub mod plugin;
pub use plugin::{CoreCommunication, Plugin, PluginBuilder, PluginConfiguration};

pub mod address;
pub use address::Address;

pub mod error;
pub use error::PluginError;
