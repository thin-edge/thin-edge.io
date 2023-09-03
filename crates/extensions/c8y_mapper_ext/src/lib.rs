pub mod actor;
pub mod alarm_converter;
pub mod config;
pub mod converter;
pub mod dynamic_discovery;
pub mod error;
mod fragments;
pub mod json;
mod log_upload;
mod serializer;
pub mod service_monitor;
#[cfg(test)]
mod tests;

#[derive(Debug, serde::Deserialize)]
pub struct Capabilities {
    log_management: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Capabilities {
            log_management: true,
        }
    }
}
