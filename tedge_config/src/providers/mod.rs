use crate::*;

pub mod config_file_manager;
pub mod tedge_config_dto;

pub trait TEdgeConfigManager:
    QuerySetting<DeviceIdSetting>
    + QuerySetting<DeviceKeyPathSetting>
    + QuerySetting<DeviceCertPathSetting>
    + QuerySetting<C8yUrlSetting>
    + QuerySetting<C8yRootCertPathSetting>
    + QuerySetting<AzureUrlSetting>
    + QuerySetting<AzureRootCertPathSetting>
{
}
