use crate::*;
use certificate::{CertificateError, PemCertificate};
use std::convert::{TryFrom, TryInto};

/// Represents the complete configuration of a thin edge device.
/// This configuration is a wrapper over the device specific configurations
/// as well as the IoT cloud provider specific configurations.
///
#[derive(Debug)]
pub struct TEdgeConfig {
    pub(crate) data: TEdgeConfigDto,
    pub(crate) config_location: TEdgeConfigLocation,
    pub(crate) config_defaults: TEdgeConfigDefaults,
}

impl ConfigSettingAccessor<DeviceIdSetting> for TEdgeConfig {
    fn query(&self, _setting: DeviceIdSetting) -> ConfigSettingResult<String> {
        let cert_path = self.query(DeviceCertPathSetting)?;
        let pem = PemCertificate::from_pem_file(cert_path)
            .map_err(|err| cert_error_into_config_error(DeviceIdSetting::KEY, err))?;
        let device_id = pem
            .subject_common_name()
            .map_err(|err| cert_error_into_config_error(DeviceIdSetting::KEY, err))?;
        Ok(device_id)
    }

    fn update(&mut self, _setting: DeviceIdSetting, _value: String) -> ConfigSettingResult<()> {
        Err(ConfigSettingError::ReadonlySetting {
            message: concat!(
                "Setting the device id is only allowed with `tedge cert create`.\n",
                "To set 'device.id', use `tedge cert create --device-id <id>`."
            ),
        })
    }

    fn unset(&mut self, _setting: DeviceIdSetting) -> ConfigSettingResult<()> {
        Err(ConfigSettingError::ReadonlySetting {
            message: concat!(
                "Setting the device id is only allowed with `tedge cert create`.\n",
                "To set 'device.id', use `tedge cert create --device-id <id>`."
            ),
        })
    }
}

fn cert_error_into_config_error(key: &'static str, err: CertificateError) -> ConfigSettingError {
    match &err {
        CertificateError::IoError(io_err) => match io_err.kind() {
            std::io::ErrorKind::NotFound => ConfigSettingError::ConfigNotSet { key },
            _ => ConfigSettingError::DerivationFailed {
                key,
                cause: format!("{}", err),
            },
        },
        _ => ConfigSettingError::DerivationFailed {
            key,
            cause: format!("{}", err),
        },
    }
}

impl ConfigSettingAccessor<WritableDeviceIdSetting> for TEdgeConfig {
    fn query(&self, _setting: WritableDeviceIdSetting) -> ConfigSettingResult<String> {
        self.data
            .device
            .id
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: WritableDeviceIdSetting::KEY,
            })
    }

    fn update(
        &mut self,
        _setting: WritableDeviceIdSetting,
        value: String,
    ) -> ConfigSettingResult<()> {
        self.data.device.id = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: WritableDeviceIdSetting) -> ConfigSettingResult<()> {
        self.data.device.id = None;
        Ok(())
    }
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
    fn query(&self, _setting: DeviceCertPathSetting) -> ConfigSettingResult<FilePath> {
        Ok(self
            .data
            .device
            .cert_path
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_device_cert_path.clone()))
    }

    fn update(
        &mut self,
        _setting: DeviceCertPathSetting,
        value: FilePath,
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
    fn query(&self, _setting: DeviceKeyPathSetting) -> ConfigSettingResult<FilePath> {
        Ok(self
            .data
            .device
            .key_path
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_device_key_path.clone()))
    }

    fn update(
        &mut self,
        _setting: DeviceKeyPathSetting,
        value: FilePath,
    ) -> ConfigSettingResult<()> {
        self.data.device.key_path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: DeviceKeyPathSetting) -> ConfigSettingResult<()> {
        self.data.device.key_path = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<AzureRootCertPathSetting> for TEdgeConfig {
    fn query(&self, _setting: AzureRootCertPathSetting) -> ConfigSettingResult<FilePath> {
        Ok(self
            .data
            .azure
            .root_cert_path
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_azure_root_cert_path.clone()))
    }

    fn update(
        &mut self,
        _setting: AzureRootCertPathSetting,
        value: FilePath,
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
    fn query(&self, _setting: C8yRootCertPathSetting) -> ConfigSettingResult<FilePath> {
        Ok(self
            .data
            .c8y
            .root_cert_path
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_c8y_root_cert_path.clone()))
    }

    fn update(
        &mut self,
        _setting: C8yRootCertPathSetting,
        value: FilePath,
    ) -> ConfigSettingResult<()> {
        self.data.c8y.root_cert_path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: C8yRootCertPathSetting) -> ConfigSettingResult<()> {
        self.data.c8y.root_cert_path = None;
        Ok(())
    }
}

/// Generic extension trait implementation for all `ConfigSetting`s of `TEdgeConfig`
/// that provide `TryFrom`/`TryInto` implementations for `String`.
impl<T, E, F> ConfigSettingAccessorStringExt<T> for TEdgeConfig
where
    T: ConfigSetting,
    TEdgeConfig: ConfigSettingAccessor<T>,
    T::Value: TryFrom<String, Error = E>,
    T::Value: TryInto<String, Error = F>,
{
    fn query_string(&self, setting: T) -> ConfigSettingResult<String> {
        self.query(setting)?
            .try_into()
            .map_err(|_e| ConfigSettingError::ConversionIntoStringFailed)
    }

    fn update_string(&mut self, setting: T, string_value: String) -> ConfigSettingResult<()> {
        T::Value::try_from(string_value)
            .map_err(|_e| ConfigSettingError::ConversionFromStringFailed)
            .and_then(|value| self.update(setting, value))
    }
}
