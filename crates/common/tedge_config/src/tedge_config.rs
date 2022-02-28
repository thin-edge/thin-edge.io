use crate::*;
use certificate::{CertificateError, PemCertificate};
use std::convert::{TryFrom, TryInto};

/// loads tedge config from system default
pub fn get_tedge_config() -> Result<TEdgeConfig, TEdgeConfigError> {
    let tedge_config_location = TEdgeConfigLocation::default();
    TEdgeConfigRepository::new(tedge_config_location).load()
}

/// Represents the complete configuration of a thin edge device.
/// This configuration is a wrapper over the device specific configurations
/// as well as the IoT cloud provider specific configurations.
///
#[derive(Debug)]
pub struct TEdgeConfig {
    pub(crate) data: TEdgeConfigDto,
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
        Err(device_id_read_only_error())
    }

    fn unset(&mut self, _setting: DeviceIdSetting) -> ConfigSettingResult<()> {
        Err(device_id_read_only_error())
    }
}

impl ConfigSettingAccessor<DeviceTypeSetting> for TEdgeConfig {
    fn query(&self, _setting: DeviceTypeSetting) -> ConfigSettingResult<String> {
        let device_type = self
            .data
            .device
            .device_type
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_device_type.clone());
        Ok(device_type)
    }

    fn update(
        &mut self,
        _setting: DeviceTypeSetting,
        value: <DeviceTypeSetting as ConfigSetting>::Value,
    ) -> ConfigSettingResult<()> {
        self.data.device.device_type = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: DeviceTypeSetting) -> ConfigSettingResult<()> {
        self.data.device.device_type = None;
        Ok(())
    }
}

fn device_id_read_only_error() -> ConfigSettingError {
    ConfigSettingError::ReadonlySetting {
        message: concat!(
            "The device id is read from the device certificate and cannot be set directly.\n",
            "To set 'device.id' to some <id>, you can use `tedge cert create --device-id <id>`.",
        ),
    }
}

fn cert_error_into_config_error(key: &'static str, err: CertificateError) -> ConfigSettingError {
    match &err {
        CertificateError::IoError(io_err) => match io_err.kind() {
            std::io::ErrorKind::NotFound => ConfigSettingError::SettingIsNotConfigurable { key,
                message: concat!(
                    "The device id is read from the device certificate.\n",
                    "To set 'device.id' to some <id>, you can use `tedge cert create --device-id <id>`.",
                ),
            },
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

impl ConfigSettingAccessor<AzureUrlSetting> for TEdgeConfig {
    fn query(&self, _setting: AzureUrlSetting) -> ConfigSettingResult<ConnectUrl> {
        self.data
            .az
            .url
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: AzureUrlSetting::KEY,
            })
    }

    fn update(&mut self, _setting: AzureUrlSetting, value: ConnectUrl) -> ConfigSettingResult<()> {
        self.data.az.url = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: AzureUrlSetting) -> ConfigSettingResult<()> {
        self.data.az.url = None;
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
            .az
            .root_cert_path
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_azure_root_cert_path.clone()))
    }

    fn update(
        &mut self,
        _setting: AzureRootCertPathSetting,
        value: FilePath,
    ) -> ConfigSettingResult<()> {
        self.data.az.root_cert_path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: AzureRootCertPathSetting) -> ConfigSettingResult<()> {
        self.data.az.root_cert_path = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<AzureMapperTimestamp> for TEdgeConfig {
    fn query(&self, _setting: AzureMapperTimestamp) -> ConfigSettingResult<Flag> {
        Ok(self
            .data
            .az
            .mapper_timestamp
            .map(Flag)
            .unwrap_or_else(|| self.config_defaults.default_mapper_timestamp.clone()))
    }

    fn update(&mut self, _setting: AzureMapperTimestamp, value: Flag) -> ConfigSettingResult<()> {
        self.data.az.mapper_timestamp = Some(value.into());
        Ok(())
    }

    fn unset(&mut self, _setting: AzureMapperTimestamp) -> ConfigSettingResult<()> {
        self.data.az.mapper_timestamp = None;
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

impl ConfigSettingAccessor<MqttPortSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttPortSetting) -> ConfigSettingResult<Port> {
        Ok(self
            .data
            .mqtt
            .port
            .map(Port)
            .unwrap_or_else(|| self.config_defaults.default_mqtt_port))
    }

    fn update(&mut self, _setting: MqttPortSetting, value: Port) -> ConfigSettingResult<()> {
        self.data.mqtt.port = Some(value.into());
        Ok(())
    }

    fn unset(&mut self, _setting: MqttPortSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.port = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttBindAddressSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttBindAddressSetting) -> ConfigSettingResult<IpAddress> {
        Ok(self
            .data
            .mqtt
            .bind_address
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_mqtt_bind_address.clone()))
    }

    fn update(
        &mut self,
        _setting: MqttBindAddressSetting,
        value: IpAddress,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.bind_address = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttBindAddressSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.bind_address = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttExternalPortSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttExternalPortSetting) -> ConfigSettingResult<Port> {
        self.data
            .mqtt
            .external_port
            .map(Port)
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: MqttExternalPortSetting::KEY,
            })
    }

    fn update(
        &mut self,
        _setting: MqttExternalPortSetting,
        value: Port,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.external_port = Some(value.into());
        Ok(())
    }

    fn unset(&mut self, _setting: MqttExternalPortSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.external_port = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttExternalBindAddressSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttExternalBindAddressSetting) -> ConfigSettingResult<IpAddress> {
        self.data
            .mqtt
            .external_bind_address
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: MqttExternalBindAddressSetting::KEY,
            })
    }

    fn update(
        &mut self,
        _setting: MqttExternalBindAddressSetting,
        value: IpAddress,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.external_bind_address = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttExternalBindAddressSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.external_bind_address = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttExternalBindInterfaceSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttExternalBindInterfaceSetting) -> ConfigSettingResult<String> {
        self.data
            .mqtt
            .external_bind_interface
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: MqttExternalBindInterfaceSetting::KEY,
            })
    }

    fn update(
        &mut self,
        _setting: MqttExternalBindInterfaceSetting,
        value: String,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.external_bind_interface = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttExternalBindInterfaceSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.external_bind_interface = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttExternalCAPathSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttExternalCAPathSetting) -> ConfigSettingResult<FilePath> {
        self.data
            .mqtt
            .external_capath
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: MqttExternalCAPathSetting::KEY,
            })
    }

    fn update(
        &mut self,
        _setting: MqttExternalCAPathSetting,
        value: FilePath,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.external_capath = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttExternalCAPathSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.external_capath = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttExternalCertfileSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttExternalCertfileSetting) -> ConfigSettingResult<FilePath> {
        self.data
            .mqtt
            .external_certfile
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: MqttExternalCertfileSetting::KEY,
            })
    }

    fn update(
        &mut self,
        _setting: MqttExternalCertfileSetting,
        value: FilePath,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.external_certfile = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttExternalCertfileSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.external_certfile = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttExternalKeyfileSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttExternalKeyfileSetting) -> ConfigSettingResult<FilePath> {
        self.data
            .mqtt
            .external_keyfile
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: MqttExternalKeyfileSetting::KEY,
            })
    }

    fn update(
        &mut self,
        _setting: MqttExternalKeyfileSetting,
        value: FilePath,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.external_keyfile = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttExternalKeyfileSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.external_keyfile = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<SoftwarePluginDefaultSetting> for TEdgeConfig {
    fn query(&self, _setting: SoftwarePluginDefaultSetting) -> ConfigSettingResult<String> {
        self.data
            .software
            .default_plugin_type
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: SoftwarePluginDefaultSetting::KEY,
            })
    }

    fn update(
        &mut self,
        _setting: SoftwarePluginDefaultSetting,
        value: String,
    ) -> ConfigSettingResult<()> {
        self.data.software.default_plugin_type = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: SoftwarePluginDefaultSetting) -> ConfigSettingResult<()> {
        self.data.software.default_plugin_type = None;
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

impl ConfigSettingAccessor<TmpPathDefaultSetting> for TEdgeConfig {
    fn query(&self, _setting: TmpPathDefaultSetting) -> ConfigSettingResult<FilePath> {
        Ok(self
            .data
            .tmp
            .tmp_path
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_tmp_path.clone()))
    }

    fn update(
        &mut self,
        _setting: TmpPathDefaultSetting,
        value: FilePath,
    ) -> ConfigSettingResult<()> {
        self.data.tmp.tmp_path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: TmpPathDefaultSetting) -> ConfigSettingResult<()> {
        self.data.tmp.tmp_path = None;
        Ok(())
    }
}
