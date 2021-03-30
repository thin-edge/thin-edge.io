use crate::*;

impl QuerySetting<AzureUrlSetting> for TEdgeConfig {
    fn query(&self, _setting: AzureUrlSetting) -> ConfigSettingResult<ConnectUrl> {
        self.azure
            .url
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: AzureUrlSetting::EXTERNAL_KEY,
            })
    }
}

impl QuerySetting<C8yUrlSetting> for TEdgeConfig {
    fn query(&self, _setting: C8yUrlSetting) -> ConfigSettingResult<ConnectUrl> {
        self.c8y
            .url
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: C8yUrlSetting::EXTERNAL_KEY,
            })
    }
}

impl QuerySetting<DeviceIdSetting> for TEdgeConfig {
    fn query(&self, _setting: DeviceIdSetting) -> ConfigSettingResult<String> {
        self.device
            .id
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: DeviceIdSetting::EXTERNAL_KEY,
            })
    }
}

impl QuerySetting<DeviceCertPathSetting> for TEdgeConfig {
    fn query(&self, _setting: DeviceCertPathSetting) -> ConfigSettingResult<String> {
        self.device
            .cert_path
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: DeviceCertPathSetting::EXTERNAL_KEY,
            })
    }
}

impl QuerySetting<DeviceKeyPathSetting> for TEdgeConfig {
    fn query(&self, _setting: DeviceKeyPathSetting) -> ConfigSettingResult<String> {
        self.device
            .key_path
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: DeviceKeyPathSetting::EXTERNAL_KEY,
            })
    }
}

impl QuerySetting<AzureRootCertPathSetting> for TEdgeConfig {
    fn query(&self, _setting: AzureRootCertPathSetting) -> ConfigSettingResult<String> {
        self.azure
            .root_cert_path
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: AzureRootCertPathSetting::EXTERNAL_KEY,
            })
    }
}

impl QuerySetting<C8yRootCertPathSetting> for TEdgeConfig {
    fn query(&self, _setting: C8yRootCertPathSetting) -> ConfigSettingResult<String> {
        self.c8y
            .root_cert_path
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: C8yRootCertPathSetting::EXTERNAL_KEY,
            })
    }
}

const DEFAULT_ROOT_CERT_PATH: &str = "/etc/ssl/certs";

impl QuerySettingWithDefault<AzureRootCertPathSetting> for TEdgeConfig {
    fn query_with_default(&self, _setting: AzureRootCertPathSetting) -> ConfigSettingResult<String> {
        Ok(self.azure
            .root_cert_path
            .clone()
            .unwrap_or_else(|| DEFAULT_ROOT_CERT_PATH.into()))
    }
}

impl QuerySettingWithDefault<C8yRootCertPathSetting> for TEdgeConfig {
    fn query_with_default(&self, _setting: C8yRootCertPathSetting) -> ConfigSettingResult<String> {
        Ok(self.c8y
            .root_cert_path
            .clone()
            .unwrap_or_else(|| DEFAULT_ROOT_CERT_PATH.into()))
    }
}


impl UpdateSetting<DeviceIdSetting> for TEdgeConfig {
    fn update(&mut self, _setting: DeviceIdSetting, value: String) -> ConfigSettingResult<()> {
        self.device.id = Some(value);
        Ok(())
    }
}

impl UpdateSetting<AzureUrlSetting> for TEdgeConfig {
    fn update(&mut self, _setting: AzureUrlSetting, value: ConnectUrl) -> ConfigSettingResult<()> {
        self.azure.url = Some(value);
        Ok(())
    }
}

impl UpdateSetting<C8yUrlSetting> for TEdgeConfig {
    fn update(&mut self, _setting: C8yUrlSetting, value: ConnectUrl) -> ConfigSettingResult<()> {
        self.c8y.url = Some(value);
        Ok(())
    }
}

impl UpdateSetting<DeviceCertPathSetting> for TEdgeConfig {
    fn update(
        &mut self,
        _setting: DeviceCertPathSetting,
        value: String,
    ) -> ConfigSettingResult<()> {
        self.device.cert_path = Some(value);
        Ok(())
    }
}

impl UpdateSetting<DeviceKeyPathSetting> for TEdgeConfig {
    fn update(&mut self, _setting: DeviceKeyPathSetting, value: String) -> ConfigSettingResult<()> {
        self.device.key_path = Some(value);
        Ok(())
    }
}

impl UpdateSetting<AzureRootCertPathSetting> for TEdgeConfig {
    fn update(
        &mut self,
        _setting: AzureRootCertPathSetting,
        value: String,
    ) -> ConfigSettingResult<()> {
        self.azure.root_cert_path = Some(value);
        Ok(())
    }
}

impl UpdateSetting<C8yRootCertPathSetting> for TEdgeConfig {
    fn update(
        &mut self,
        _setting: C8yRootCertPathSetting,
        value: String,
    ) -> ConfigSettingResult<()> {
        self.c8y.root_cert_path = Some(value);
        Ok(())
    }
}

impl UnsetSetting<AzureRootCertPathSetting> for TEdgeConfig {
    fn unset(&mut self, _setting: AzureRootCertPathSetting) -> ConfigSettingResult<()> {
        self.azure.root_cert_path = None;
        Ok(())
    }
}

impl UnsetSetting<C8yRootCertPathSetting> for TEdgeConfig {
    fn unset(&mut self, _setting: C8yRootCertPathSetting) -> ConfigSettingResult<()> {
        self.c8y.root_cert_path = None;
        Ok(())
    }
}

impl UnsetSetting<DeviceIdSetting> for TEdgeConfig {
    fn unset(&mut self, _setting: DeviceIdSetting) -> ConfigSettingResult<()> {
        self.device.id = None;
        Ok(())
    }
}

impl UnsetSetting<DeviceKeyPathSetting> for TEdgeConfig {
    fn unset(&mut self, _setting: DeviceKeyPathSetting) -> ConfigSettingResult<()> {
        self.device.key_path = None;
        Ok(())
    }
}

impl UnsetSetting<DeviceCertPathSetting> for TEdgeConfig {
    fn unset(&mut self, _setting: DeviceCertPathSetting) -> ConfigSettingResult<()> {
        self.device.cert_path = None;
        Ok(())
    }
}

impl UnsetSetting<C8yUrlSetting> for TEdgeConfig {
    fn unset(&mut self, _setting: C8yUrlSetting) -> ConfigSettingResult<()> {
        self.c8y.url = None;
        Ok(())
    }
}

impl UnsetSetting<AzureUrlSetting> for TEdgeConfig {
    fn unset(&mut self, _setting: AzureUrlSetting) -> ConfigSettingResult<()> {
        self.azure.url = None;
        Ok(())
    }
}
