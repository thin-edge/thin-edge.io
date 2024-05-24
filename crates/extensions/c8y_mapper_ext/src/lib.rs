pub mod actor;
pub mod alarm_converter;
pub mod compatibility_adapter;
pub mod config;
pub mod converter;
pub mod dynamic_discovery;
pub mod error;
mod fragments;
mod inventory;
pub mod json;
mod operations;
mod serializer;
pub mod service_monitor;
#[cfg(test)]
mod tests;

pub(crate) const C8Y_BRIDGE_TOPIC_ID: &str = "device/main/service/mosquitto-c8y-bridge";

#[derive(Debug, serde::Deserialize)]
pub struct Capabilities {
    log_upload: bool,
    config_snapshot: bool,
    config_update: bool,
    firmware_update: bool,
}

#[cfg(test)]
impl Default for Capabilities {
    fn default() -> Self {
        Capabilities {
            log_upload: true,
            config_snapshot: true,
            config_update: true,
            firmware_update: true,
        }
    }
}
