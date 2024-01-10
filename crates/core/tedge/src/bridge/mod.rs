//! Creating and updating `mosquitto.conf` files for MQTT bridges to different clouds.

mod common_mosquitto_config;
mod config;

pub mod aws;
pub mod azure;
pub mod c8y;

pub use common_mosquitto_config::*;
pub use config::BridgeConfig;

pub const C8Y_CONFIG_FILENAME: &str = "c8y-bridge.conf";
pub const AZURE_CONFIG_FILENAME: &str = "az-bridge.conf";
pub const AWS_CONFIG_FILENAME: &str = "aws-bridge.conf";

pub const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "mosquitto-conf";
