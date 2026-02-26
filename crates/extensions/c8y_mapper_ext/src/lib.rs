pub mod actor;
pub mod alarm_converter;
pub mod availability;
pub mod compatibility_adapter;
pub mod config;
pub mod converter;
pub mod dynamic_discovery;
pub mod entity_cache;
pub mod error;
pub mod flows;
mod fragments;
mod inventory;
pub mod json;
mod mea;
mod operations;
mod serializer;
pub mod service_monitor;
mod signals;
mod supported_operations;

#[cfg(test)]
mod shuffled_tests;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct Capabilities {
    pub log_upload: bool,
    pub config_snapshot: bool,
    pub config_update: bool,
    pub firmware_update: bool,
    pub device_profile: bool,
    pub device_restart: bool,
    pub software_update: bool,
}

#[cfg(test)]
impl Default for Capabilities {
    fn default() -> Self {
        Capabilities {
            log_upload: true,
            config_snapshot: true,
            config_update: true,
            firmware_update: true,
            device_profile: true,
            device_restart: true,
            software_update: true,
        }
    }
}
