use crate::*;
use certificate::CertificateError;
use certificate::PemCertificate;
use std::convert::TryFrom;
use std::convert::TryInto;

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

fn http_bind_address_read_only_error() -> ConfigSettingError {
    ConfigSettingError::ReadonlySetting {
        message: concat!(
            "The http address cannot be set directly. It is read from the mqtt bind address.\n",
            "To set 'http.bind_address' to some <address>, you can `tedge config set mqtt.bind_address <address>`.",
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

impl ConfigSettingAccessor<AwsUrlSetting> for TEdgeConfig {
    fn query(&self, _setting: AwsUrlSetting) -> ConfigSettingResult<ConnectUrl> {
        self.data
            .aws
            .url
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: AwsUrlSetting::KEY,
            })
    }

    fn update(&mut self, _setting: AwsUrlSetting, value: ConnectUrl) -> ConfigSettingResult<()> {
        self.data.aws.url = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: AwsUrlSetting) -> ConfigSettingResult<()> {
        self.data.aws.url = None;
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

impl ConfigSettingAccessor<C8ySmartRestTemplates> for TEdgeConfig {
    fn query(&self, _setting: C8ySmartRestTemplates) -> ConfigSettingResult<TemplatesSet> {
        Ok(self
            .data
            .c8y
            .smartrest_templates
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_c8y_smartrest_templates.clone()))
    }

    fn update(
        &mut self,
        _setting: C8ySmartRestTemplates,
        value: TemplatesSet,
    ) -> ConfigSettingResult<()> {
        self.data.c8y.smartrest_templates = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: C8ySmartRestTemplates) -> ConfigSettingResult<()> {
        self.data.c8y.smartrest_templates = None;
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

impl ConfigSettingAccessor<AwsRootCertPathSetting> for TEdgeConfig {
    fn query(&self, _setting: AwsRootCertPathSetting) -> ConfigSettingResult<FilePath> {
        Ok(self
            .data
            .aws
            .root_cert_path
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_aws_root_cert_path.clone()))
    }

    fn update(
        &mut self,
        _setting: AwsRootCertPathSetting,
        value: FilePath,
    ) -> ConfigSettingResult<()> {
        self.data.aws.root_cert_path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: AwsRootCertPathSetting) -> ConfigSettingResult<()> {
        self.data.aws.root_cert_path = None;
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

impl ConfigSettingAccessor<AwsMapperTimestamp> for TEdgeConfig {
    fn query(&self, _setting: AwsMapperTimestamp) -> ConfigSettingResult<Flag> {
        Ok(self
            .data
            .aws
            .mapper_timestamp
            .map(Flag)
            .unwrap_or_else(|| self.config_defaults.default_mapper_timestamp.clone()))
    }

    fn update(&mut self, _setting: AwsMapperTimestamp, value: Flag) -> ConfigSettingResult<()> {
        self.data.aws.mapper_timestamp = Some(value.into());
        Ok(())
    }

    fn unset(&mut self, _setting: AwsMapperTimestamp) -> ConfigSettingResult<()> {
        self.data.aws.mapper_timestamp = None;
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

impl ConfigSettingAccessor<HttpPortSetting> for TEdgeConfig {
    fn query(&self, _setting: HttpPortSetting) -> ConfigSettingResult<Port> {
        Ok(self
            .data
            .http
            .port
            .map(Port)
            .unwrap_or_else(|| self.config_defaults.default_http_port))
    }

    fn update(&mut self, _setting: HttpPortSetting, value: Port) -> ConfigSettingResult<()> {
        self.data.http.port = Some(value.into());
        Ok(())
    }

    fn unset(&mut self, _setting: HttpPortSetting) -> ConfigSettingResult<()> {
        self.data.http.port = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<HttpBindAddressSetting> for TEdgeConfig {
    fn query(&self, _setting: HttpBindAddressSetting) -> ConfigSettingResult<IpAddress> {
        // we match to external bind address if there is one,
        // otherwise match to internal bind address
        let internal_bind_address: IpAddress = self.query(MqttBindAddressSetting)?;
        let external_bind_address_or_err = self.query(MqttExternalBindAddressSetting);

        match external_bind_address_or_err {
            Ok(external_bind_address) => Ok(external_bind_address),
            Err(_) => Ok(internal_bind_address),
        }
    }

    fn update(
        &mut self,
        _setting: HttpBindAddressSetting,
        _value: IpAddress,
    ) -> ConfigSettingResult<()> {
        Err(http_bind_address_read_only_error())
    }

    fn unset(&mut self, _setting: HttpBindAddressSetting) -> ConfigSettingResult<()> {
        Err(http_bind_address_read_only_error())
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

impl ConfigSettingAccessor<TmpPathSetting> for TEdgeConfig {
    fn query(&self, _setting: TmpPathSetting) -> ConfigSettingResult<FilePath> {
        Ok(self
            .data
            .tmp
            .dir_path
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_tmp_path.clone()))
    }

    fn update(&mut self, _setting: TmpPathSetting, value: FilePath) -> ConfigSettingResult<()> {
        self.data.tmp.dir_path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: TmpPathSetting) -> ConfigSettingResult<()> {
        self.data.tmp.dir_path = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<LogPathSetting> for TEdgeConfig {
    fn query(&self, _setting: LogPathSetting) -> ConfigSettingResult<FilePath> {
        Ok(self
            .data
            .logs
            .dir_path
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_logs_path.clone()))
    }

    fn update(&mut self, _setting: LogPathSetting, value: FilePath) -> ConfigSettingResult<()> {
        self.data.logs.dir_path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: LogPathSetting) -> ConfigSettingResult<()> {
        self.data.logs.dir_path = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<RunPathSetting> for TEdgeConfig {
    fn query(&self, _setting: RunPathSetting) -> ConfigSettingResult<FilePath> {
        Ok(self
            .data
            .run
            .dir_path
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_run_path.clone()))
    }

    fn update(&mut self, _setting: RunPathSetting, value: FilePath) -> ConfigSettingResult<()> {
        self.data.run.dir_path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: RunPathSetting) -> ConfigSettingResult<()> {
        self.data.run.dir_path = None;
        Ok(())
    }
}
