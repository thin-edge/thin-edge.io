pub mod actor;
pub mod alarm_converter;
pub mod compatibility_adapter;
pub mod config;
mod config_operations;
pub mod converter;
pub mod dynamic_discovery;
pub mod error;
mod fragments;
mod inventory;
pub mod json;
mod log_upload;
mod serializer;
pub mod service_monitor;
#[cfg(test)]
mod tests;

#[derive(Debug, serde::Deserialize)]
pub struct Capabilities {
    log_upload: bool,
    config_snapshot: bool,
    config_update: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities {
            log_upload: true,
            config_snapshot: true,
            config_update: true,
        }
    }
}
