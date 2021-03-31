use crate::*;
use serde::{Deserialize, Serialize};

/// Represents the complete configuration of a thin edge device.
/// This configuration is a wrapper over the device specific configurations
/// as well as the IoT cloud provider specific configurations.
///
/// The following example showcases how the thin edge configuration can be read
/// and how individual configuration values can be retrieved out of it:
///
/// # Examples
/// ```ignore
/// /// Read the default tedge.toml file into a TEdgeConfigDto object
/// let config: TEdgeConfigDto = TEdgeConfigDto::from_default_config().unwrap();
///
/// /// Fetch the device config from the TEdgeConfigDto object
/// let device_config: DeviceConfigDto = config.device;
/// /// Fetch the device id from the DeviceConfigDto object
/// let device_id = device_config.id.unwrap();
///
/// /// Fetch the Cumulocity config from the TEdgeConfigDto object
/// let cumulocity_config: CumulocityConfigDto = config.c8y;
/// /// Fetch the Cumulocity URL from the CumulocityConfigDto object
/// let cumulocity_url = cumulocity_config.url.unwrap();
/// ```
///
#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct TEdgeConfigDto {
    /// Captures the device specific configurations
    #[serde(default)]
    pub(crate) device: DeviceConfigDto,

    /// Captures the configurations required to connect to Cumulocity
    #[serde(default)]
    pub(crate) c8y: CumulocityConfigDto,
    #[serde(default)]
    pub(crate) azure: AzureConfigDto,
}

/// Represents the device specific configurations defined in the [device] section
/// of the thin edge configuration TOML file
#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct DeviceConfigDto {
    /// The unique id of the device
    pub(crate) id: Option<String>,

    /// Path where the device's private key is stored.
    /// Defaults to $HOME/.tedge/tedge-private.pem
    pub(crate) key_path: Option<String>,

    /// Path where the device's certificate is stored.
    /// Defaults to $HOME/.tedge/tedge-certificate.crt
    pub(crate) cert_path: Option<String>,
}

/// Represents the Cumulocity specific configurations defined in the
/// [c8y] section of the thin edge configuration TOML file
#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct CumulocityConfigDto {
    /// Preserves the current status of the connection
    connect: Option<String>,

    /// Endpoint URL of the Cumulocity tenant
    pub(crate) url: Option<ConnectUrl>,

    /// The path where Cumulocity root certificate(s) are stored.
    /// The value can be a directory path as well as the path of the direct certificate file.
    pub(crate) root_cert_path: Option<String>,
}

#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct AzureConfigDto {
    connect: Option<String>,
    pub(crate) url: Option<ConnectUrl>,
    pub(crate) root_cert_path: Option<String>,
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
