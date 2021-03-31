use crate::*;
use serde::{Deserialize, Serialize};

#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct TEdgeConfigDto {
    /// Captures the device specific configurations
    #[serde(default)]
    device: DeviceConfigDto,

    /// Captures the configurations required to connect to Cumulocity
    #[serde(default)]
    c8y: CumulocityConfigDto,
    #[serde(default)]
    azure: AzureConfigDto,
}

/// Represents the device specific configurations defined in the [device] section
/// of the thin edge configuration TOML file
#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct DeviceConfigDto {
    /// The unique id of the device
    id: Option<String>,

    /// Path where the device's private key is stored.
    /// Defaults to $HOME/.tedge/tedge-private.pem
    key_path: Option<String>,

    /// Path where the device's certificate is stored.
    /// Defaults to $HOME/.tedge/tedge-certificate.crt
    cert_path: Option<String>,
}

/// Represents the Cumulocity specific configurations defined in the
/// [c8y] section of the thin edge configuration TOML file
#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct CumulocityConfigDto {
    /// Preserves the current status of the connection
    connect: Option<String>,

    /// Endpoint URL of the Cumulocity tenant
    url: Option<ConnectUrl>,

    /// The path where Cumulocity root certificate(s) are stored.
    /// The value can be a directory path as well as the path of the direct certificate file.
    root_cert_path: Option<String>,
}

#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct AzureConfigDto {
    connect: Option<String>,
    url: Option<ConnectUrl>,
    root_cert_path: Option<String>,
}

impl QuerySetting<AzureUrlSetting> for TEdgeConfigDto {
    fn query(&self, _setting: AzureUrlSetting) -> ConfigSettingResult<ConnectUrl> {
        self.azure
            .url
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: AzureUrlSetting::EXTERNAL_KEY,
            })
    }
}

impl QuerySetting<C8yUrlSetting> for TEdgeConfigDto {
    fn query(&self, _setting: C8yUrlSetting) -> ConfigSettingResult<ConnectUrl> {
        self.c8y
            .url
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: C8yUrlSetting::EXTERNAL_KEY,
            })
    }
}

impl QuerySetting<DeviceIdSetting> for TEdgeConfigDto {
    fn query(&self, _setting: DeviceIdSetting) -> ConfigSettingResult<String> {
        self.device
            .id
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: DeviceIdSetting::EXTERNAL_KEY,
            })
    }
}

impl QuerySetting<DeviceCertPathSetting> for TEdgeConfigDto {
    fn query(&self, _setting: DeviceCertPathSetting) -> ConfigSettingResult<String> {
        self.device
            .cert_path
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: DeviceCertPathSetting::EXTERNAL_KEY,
            })
    }
}

impl QuerySetting<DeviceKeyPathSetting> for TEdgeConfigDto {
    fn query(&self, _setting: DeviceKeyPathSetting) -> ConfigSettingResult<String> {
        self.device
            .key_path
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: DeviceKeyPathSetting::EXTERNAL_KEY,
            })
    }
}

impl QuerySetting<AzureRootCertPathSetting> for TEdgeConfigDto {
    fn query(&self, _setting: AzureRootCertPathSetting) -> ConfigSettingResult<String> {
        self.azure
            .root_cert_path
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: AzureRootCertPathSetting::EXTERNAL_KEY,
            })
    }
}

impl QuerySetting<C8yRootCertPathSetting> for TEdgeConfigDto {
    fn query(&self, _setting: C8yRootCertPathSetting) -> ConfigSettingResult<String> {
        self.c8y
            .root_cert_path
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: C8yRootCertPathSetting::EXTERNAL_KEY,
            })
    }
}

impl UpdateSetting<DeviceIdSetting> for TEdgeConfigDto {
    fn update(&mut self, _setting: DeviceIdSetting, value: String) -> ConfigSettingResult<()> {
        self.device.id = Some(value);
        Ok(())
    }
}

impl UpdateSetting<AzureUrlSetting> for TEdgeConfigDto {
    fn update(&mut self, _setting: AzureUrlSetting, value: ConnectUrl) -> ConfigSettingResult<()> {
        self.azure.url = Some(value);
        Ok(())
    }
}

impl UpdateSetting<C8yUrlSetting> for TEdgeConfigDto {
    fn update(&mut self, _setting: C8yUrlSetting, value: ConnectUrl) -> ConfigSettingResult<()> {
        self.c8y.url = Some(value);
        Ok(())
    }
}

impl UpdateSetting<DeviceCertPathSetting> for TEdgeConfigDto {
    fn update(
        &mut self,
        _setting: DeviceCertPathSetting,
        value: String,
    ) -> ConfigSettingResult<()> {
        self.device.cert_path = Some(value);
        Ok(())
    }
}

impl UpdateSetting<DeviceKeyPathSetting> for TEdgeConfigDto {
    fn update(&mut self, _setting: DeviceKeyPathSetting, value: String) -> ConfigSettingResult<()> {
        self.device.key_path = Some(value);
        Ok(())
    }
}

impl UpdateSetting<AzureRootCertPathSetting> for TEdgeConfigDto {
    fn update(
        &mut self,
        _setting: AzureRootCertPathSetting,
        value: String,
    ) -> ConfigSettingResult<()> {
        self.azure.root_cert_path = Some(value);
        Ok(())
    }
}

impl UpdateSetting<C8yRootCertPathSetting> for TEdgeConfigDto {
    fn update(
        &mut self,
        _setting: C8yRootCertPathSetting,
        value: String,
    ) -> ConfigSettingResult<()> {
        self.c8y.root_cert_path = Some(value);
        Ok(())
    }
}

impl UnsetSetting<AzureRootCertPathSetting> for TEdgeConfigDto {
    fn unset(&mut self, _setting: AzureRootCertPathSetting) -> ConfigSettingResult<()> {
        self.azure.root_cert_path = None;
        Ok(())
    }
}

impl UnsetSetting<C8yRootCertPathSetting> for TEdgeConfigDto {
    fn unset(&mut self, _setting: C8yRootCertPathSetting) -> ConfigSettingResult<()> {
        self.c8y.root_cert_path = None;
        Ok(())
    }
}

impl UnsetSetting<DeviceIdSetting> for TEdgeConfigDto {
    fn unset(&mut self, _setting: DeviceIdSetting) -> ConfigSettingResult<()> {
        self.device.id = None;
        Ok(())
    }
}

impl UnsetSetting<DeviceKeyPathSetting> for TEdgeConfigDto {
    fn unset(&mut self, _setting: DeviceKeyPathSetting) -> ConfigSettingResult<()> {
        self.device.key_path = None;
        Ok(())
    }
}

impl UnsetSetting<DeviceCertPathSetting> for TEdgeConfigDto {
    fn unset(&mut self, _setting: DeviceCertPathSetting) -> ConfigSettingResult<()> {
        self.device.cert_path = None;
        Ok(())
    }
}

impl UnsetSetting<C8yUrlSetting> for TEdgeConfigDto {
    fn unset(&mut self, _setting: C8yUrlSetting) -> ConfigSettingResult<()> {
        self.c8y.url = None;
        Ok(())
    }
}

impl UnsetSetting<AzureUrlSetting> for TEdgeConfigDto {
    fn unset(&mut self, _setting: AzureUrlSetting) -> ConfigSettingResult<()> {
        self.azure.url = None;
        Ok(())
    }
}
