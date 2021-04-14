pub use self::{az::*, bridge_command::*, bridge_config::*, c8y::*, cli::*, error::*};

mod bridge_command;
mod bridge_config;
mod cli;
mod error;

pub mod az;
pub mod c8y;

use crate::config::{
    AZURE_ROOT_CERT_PATH, AZURE_URL, C8Y_ROOT_CERT_PATH, C8Y_URL, DEVICE_CERT_PATH, DEVICE_ID,
    DEVICE_KEY_PATH,
};

pub const COMMON_MOSQUITTO_CONFIG_FILENAME: &str = "tedge-mosquitto.conf";
const DEFAULT_ROOT_CERT_PATH: &str = "/etc/ssl/certs";
const MOSQUITTO_RESTART_TIMEOUT_SECONDS: u64 = 5;
const MQTT_TLS_PORT: u16 = 8883;
pub const TEDGE_BRIDGE_CONF_DIR_PATH: &str = "mosquitto-conf";
const WAIT_FOR_CHECK_SECONDS: u64 = 10;

fn ok_if_not_found(err: std::io::Error) -> std::io::Result<()> {
    match err.kind() {
        std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(err),
    }
}

#[cfg(test)]
mod test;
