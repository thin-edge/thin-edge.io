pub use cli::*;
pub mod bridge;
pub mod cli;
pub mod command;
pub mod error;
mod system_services;

pub type ConfigError = crate::error::TEdgeError;
const BROKER_USER: &str = "mosquitto";
const BROKER_GROUP: &str = "mosquitto";

#[cfg(not(any(feature = "aws", feature = "azure", feature = "c8y")))]
compile_error!("Either feature \"aws\", \"azure\", or \"c8y\" must be enabled.");
