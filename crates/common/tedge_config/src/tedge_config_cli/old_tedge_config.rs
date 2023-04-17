use crate::seconds::Seconds;
use crate::*;
use camino::Utf8PathBuf;
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
                cause: Box::new(err),
            },
        },
        _ => ConfigSettingError::DerivationFailed {
            key,
            cause: Box::new(err),
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
}

impl ConfigSettingAccessor<DeviceCertPathSetting> for TEdgeConfig {
    fn query(&self, _setting: DeviceCertPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        Ok(self
            .data
            .device
            .cert_path
            .clone()
            .map(|p| p.0)
            .unwrap_or_else(|| self.config_defaults.default_device_cert_path.clone()))
    }
}

impl ConfigSettingAccessor<DeviceKeyPathSetting> for TEdgeConfig {
    fn query(&self, _setting: DeviceKeyPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        Ok(self
            .data
            .device
            .key_path
            .clone()
            .map(|p| p.0)
            .unwrap_or_else(|| self.config_defaults.default_device_key_path.clone()))
    }
}

impl ConfigSettingAccessor<AzureRootCertPathSetting> for TEdgeConfig {
    fn query(&self, _setting: AzureRootCertPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        Ok(self
            .data
            .az
            .root_cert_path
            .clone()
            .map(|p| p.0)
            .unwrap_or_else(|| self.config_defaults.default_azure_root_cert_path.clone()))
    }
}

impl ConfigSettingAccessor<AwsRootCertPathSetting> for TEdgeConfig {
    fn query(&self, _setting: AwsRootCertPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        Ok(self
            .data
            .aws
            .root_cert_path
            .clone()
            .map(|p| p.0)
            .unwrap_or_else(|| self.config_defaults.default_aws_root_cert_path.clone()))
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
}

impl ConfigSettingAccessor<C8yRootCertPathSetting> for TEdgeConfig {
    fn query(&self, _setting: C8yRootCertPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        Ok(self
            .data
            .c8y
            .root_cert_path
            .clone()
            .map(|p| p.0)
            .unwrap_or_else(|| self.config_defaults.default_c8y_root_cert_path.clone()))
    }
}

impl ConfigSettingAccessor<MqttClientHostSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttClientHostSetting) -> ConfigSettingResult<String> {
        Ok(self
            .data
            .mqtt
            .client_host
            .clone()
            .unwrap_or(self.config_defaults.default_mqtt_client_host.clone()))
    }
}

impl ConfigSettingAccessor<MqttClientPortSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttClientPortSetting) -> ConfigSettingResult<Port> {
        Ok(self
            .data
            .mqtt
            .client_port
            .map(|p| Port(p.0.into()))
            .unwrap_or_else(|| self.config_defaults.default_mqtt_port))
    }
}

impl ConfigSettingAccessor<MqttClientCafileSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttClientCafileSetting) -> ConfigSettingResult<Utf8PathBuf> {
        self.data
            .mqtt
            .client_ca_file
            .clone()
            .map(|p| p.0)
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: "mqtt.client.ca_file",
            })
    }
}

impl ConfigSettingAccessor<MqttClientCapathSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttClientCapathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        self.data
            .mqtt
            .client_ca_path
            .clone()
            .map(|p| p.0)
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: "mqtt.client.ca_path",
            })
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
}

impl ConfigSettingAccessor<MqttBindAddressSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttBindAddressSetting) -> ConfigSettingResult<IpAddress> {
        Ok(self
            .data
            .mqtt
            .bind_address
            .unwrap_or(self.config_defaults.default_mqtt_bind_address))
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
}

impl ConfigSettingAccessor<MqttExternalBindAddressSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttExternalBindAddressSetting) -> ConfigSettingResult<IpAddress> {
        self.data
            .mqtt
            .external_bind_address
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: MqttExternalBindAddressSetting::KEY,
            })
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
}

impl ConfigSettingAccessor<MqttExternalCAPathSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttExternalCAPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        self.data.mqtt.external_ca_path.clone().map(|p| p.0).ok_or(
            ConfigSettingError::ConfigNotSet {
                key: MqttExternalCAPathSetting::KEY,
            },
        )
    }
}

impl ConfigSettingAccessor<MqttExternalCertfileSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttExternalCertfileSetting) -> ConfigSettingResult<Utf8PathBuf> {
        self.data
            .mqtt
            .external_cert_file
            .clone()
            .map(|p| p.0)
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: MqttExternalCertfileSetting::KEY,
            })
    }
}

impl ConfigSettingAccessor<MqttExternalKeyfileSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttExternalKeyfileSetting) -> ConfigSettingResult<Utf8PathBuf> {
        self.data.mqtt.external_key_file.clone().map(|p| p.0).ok_or(
            ConfigSettingError::ConfigNotSet {
                key: MqttExternalKeyfileSetting::KEY,
            },
        )
    }
}

impl ConfigSettingAccessor<SoftwarePluginDefaultSetting> for TEdgeConfig {
    fn query(&self, _setting: SoftwarePluginDefaultSetting) -> ConfigSettingResult<String> {
        self.data
            .software
            .default_plugin
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: SoftwarePluginDefaultSetting::KEY,
            })
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
}

impl ConfigSettingAccessor<TmpPathSetting> for TEdgeConfig {
    fn query(&self, _setting: TmpPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        Ok(self
            .data
            .tmp
            .path
            .clone()
            .map(|p| p.0)
            .unwrap_or_else(|| self.config_defaults.default_tmp_path.clone()))
    }
}

impl ConfigSettingAccessor<LogPathSetting> for TEdgeConfig {
    fn query(&self, _setting: LogPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        Ok(self
            .data
            .logs
            .path
            .clone()
            .map(|p| p.0)
            .unwrap_or_else(|| self.config_defaults.default_logs_path.clone()))
    }
}

impl ConfigSettingAccessor<RunPathSetting> for TEdgeConfig {
    fn query(&self, _setting: RunPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        Ok(self
            .data
            .run
            .path
            .clone()
            .map(|p| p.0)
            .unwrap_or_else(|| self.config_defaults.default_run_path.clone()))
    }
}

impl ConfigSettingAccessor<DataPathSetting> for TEdgeConfig {
    fn query(&self, _setting: DataPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        Ok(self
            .data
            .data
            .path
            .clone()
            .map(|p| p.0)
            .unwrap_or_else(|| self.config_defaults.default_data_path.clone()))
    }
}

impl ConfigSettingAccessor<LockFilesSetting> for TEdgeConfig {
    fn query(&self, _setting: LockFilesSetting) -> ConfigSettingResult<Flag> {
        Ok(self
            .data
            .run
            .lock_files
            .map(Flag)
            .unwrap_or_else(|| self.config_defaults.default_lock_files.clone()))
    }
}

impl ConfigSettingAccessor<FirmwareChildUpdateTimeoutSetting> for TEdgeConfig {
    fn query(&self, _setting: FirmwareChildUpdateTimeoutSetting) -> ConfigSettingResult<Seconds> {
        Ok(self
            .data
            .firmware
            .child_update_timeout
            .map(Seconds)
            .unwrap_or(self.config_defaults.default_firmware_child_update_timeout))
    }
}

impl ConfigSettingAccessor<ServiceTypeSetting> for TEdgeConfig {
    fn query(&self, _setting: ServiceTypeSetting) -> ConfigSettingResult<String> {
        Ok(self
            .data
            .service
            .service_type
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_service_type.clone()))
    }
}
