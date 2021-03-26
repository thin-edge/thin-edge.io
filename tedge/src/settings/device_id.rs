use crate::{config::*, settings::*};

///
/// Identifier of the device within the fleet. It must be globally unique and the same one used in
/// the device certificate. Example: Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665")
///
#[derive(Debug)]
pub struct DeviceIdSetting;

impl GetStringConfigSetting for DeviceIdSetting {
    type Config = TEdgeConfig;

    fn get_string(&self, config: &Self::Config) -> ConfigSettingResult<String> {
        config
            .device
            .id
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet { key: "device.id" })
    }
}
