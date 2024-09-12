use c8y_api::json_c8y_deserializer::C8yDeviceControlOperation;

pub mod actor;
pub mod alarm_converter;
pub mod availability;
pub mod compatibility_adapter;
pub mod config;
pub mod converter;
pub mod dynamic_discovery;
pub mod error;
mod fragments;
mod inventory;
pub mod json;
pub mod operations;
mod serializer;
pub mod service_monitor;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct Capabilities {
    pub log_upload: bool,
    pub config_snapshot: bool,
    pub config_update: bool,
    pub firmware_update: bool,
    pub device_profile: bool,
}

impl Capabilities {
    pub fn is_enabled(&self, operation: &C8yDeviceControlOperation) -> bool {
        match operation {
            C8yDeviceControlOperation::LogfileRequest(_) => self.log_upload,
            C8yDeviceControlOperation::UploadConfigFile(_) => self.config_snapshot,
            C8yDeviceControlOperation::DownloadConfigFile(_) => self.config_update,
            C8yDeviceControlOperation::Firmware(_) => self.firmware_update,
            C8yDeviceControlOperation::DeviceProfile(_) => self.device_profile,
            _ => true,
        }
    }
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
        }
    }
}
