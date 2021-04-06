use crate::*;

const DEFAULT_ROOT_CERT_PATH: &str = "/etc/ssl/certs";

/// Represents the complete configuration of a thin edge device.
/// This configuration is a wrapper over the device specific configurations
/// as well as the IoT cloud provider specific configurations.
///
#[derive(Debug)]
pub struct TEdgeConfig {
    pub(crate) data: TEdgeConfigDto,
}

impl TEdgeConfig {
    pub fn update_if_not_set<T: ConfigSetting + Copy>(
        &mut self,
        setting: T,
        value: T::Value,
    ) -> Result<(), TEdgeConfigError>
    where
        Self: ConfigSettingAccessor<T>,
    {
        match self.query(setting) {
            Err(ConfigSettingError::ConfigNotSet { .. }) => {
                self.update(setting, value)?;
                Ok(())
            }
            Err(other) => Err(other.into()),
            Ok(_ok) => Ok(()),
        }
    }
}

impl ConfigSettingAccessor<DeviceIdSetting> for TEdgeConfig {
    fn query(&self, _setting: DeviceIdSetting) -> ConfigSettingResult<String> {
        self.data
            .device
            .id
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: DeviceIdSetting::KEY,
            })
    }

    /*
    fn update(&mut self, _setting: DeviceIdSetting, value: String) -> ConfigSettingResult<()> {
        self.device.id = Some(value);
        Ok(())
    }
    fn unset(&mut self, _setting: DeviceIdSetting) -> ConfigSettingResult<()> {
        self.device.id = None;
        Ok(())
    }
    */
}

impl ConfigSettingAccessor<AzureUrlSetting> for TEdgeConfig {
    fn query(&self, _setting: AzureUrlSetting) -> ConfigSettingResult<ConnectUrl> {
        self.data
            .azure
            .url
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: AzureUrlSetting::KEY,
            })
    }

    fn update(&mut self, _setting: AzureUrlSetting, value: ConnectUrl) -> ConfigSettingResult<()> {
        self.data.azure.url = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: AzureUrlSetting) -> ConfigSettingResult<()> {
        self.data.azure.url = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<C8yUrlSetting> for TEdgeConfig {
    fn query(&self, _setting: C8yUrlSetting) -> ConfigSettingResult<ConnectUrl> {
        self.data
            .c8y
            .url
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: C8yUrlSetting::KEY,
            })
    }

    fn update(&mut self, _setting: C8yUrlSetting, value: ConnectUrl) -> ConfigSettingResult<()> {
        self.data.c8y.url = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: C8yUrlSetting) -> ConfigSettingResult<()> {
        self.data.c8y.url = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<DeviceCertPathSetting> for TEdgeConfig {
    fn query(&self, _setting: DeviceCertPathSetting) -> ConfigSettingResult<String> {
        self.data
            .device
            .cert_path
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: DeviceCertPathSetting::KEY,
            })
    }

    fn update(
        &mut self,
        _setting: DeviceCertPathSetting,
        value: String,
    ) -> ConfigSettingResult<()> {
        self.data.device.cert_path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: DeviceCertPathSetting) -> ConfigSettingResult<()> {
        self.data.device.cert_path = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<DeviceKeyPathSetting> for TEdgeConfig {
    fn query(&self, _setting: DeviceKeyPathSetting) -> ConfigSettingResult<String> {
        self.data
            .device
            .key_path
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: DeviceKeyPathSetting::KEY,
            })
    }

    fn update(&mut self, _setting: DeviceKeyPathSetting, value: String) -> ConfigSettingResult<()> {
        self.data.device.key_path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: DeviceKeyPathSetting) -> ConfigSettingResult<()> {
        self.data.device.key_path = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<AzureRootCertPathSetting> for TEdgeConfig {
    fn query(&self, _setting: AzureRootCertPathSetting) -> ConfigSettingResult<String> {
        Ok(self
            .data
            .azure
            .root_cert_path
            .clone()
            .unwrap_or_else(|| DEFAULT_ROOT_CERT_PATH.into()))
    }

    fn update(
        &mut self,
        _setting: AzureRootCertPathSetting,
        value: String,
    ) -> ConfigSettingResult<()> {
        self.data.azure.root_cert_path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: AzureRootCertPathSetting) -> ConfigSettingResult<()> {
        self.data.azure.root_cert_path = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<C8yRootCertPathSetting> for TEdgeConfig {
    fn query(&self, _setting: C8yRootCertPathSetting) -> ConfigSettingResult<String> {
        Ok(self
            .data
            .c8y
            .root_cert_path
            .clone()
            .unwrap_or_else(|| DEFAULT_ROOT_CERT_PATH.into()))
    }

    fn update(
        &mut self,
        _setting: C8yRootCertPathSetting,
        value: String,
    ) -> ConfigSettingResult<()> {
        self.data.c8y.root_cert_path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: C8yRootCertPathSetting) -> ConfigSettingResult<()> {
        self.data.c8y.root_cert_path = None;
        Ok(())
    }
}
