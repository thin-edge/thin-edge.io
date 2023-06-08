pub use cli::*;
pub mod cli;
pub mod command;
pub mod error;

type ConfigError = crate::error::TEdgeError;
const BROKER_USER: &str = "mosquitto";
const BROKER_GROUP: &str = "mosquitto";
