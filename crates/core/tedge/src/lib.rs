pub use cli::*;
use std::future::Future;
pub mod bridge;
pub mod cli;
pub mod command;
pub mod error;
mod system_services;

pub type ConfigError = crate::error::TEdgeError;
const BROKER_USER: &str = "mosquitto";
const BROKER_GROUP: &str = "mosquitto";

pub fn block_on<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}
