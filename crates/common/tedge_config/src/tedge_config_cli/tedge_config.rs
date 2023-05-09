use crate::*;
use camino::Utf8PathBuf;
use certificate::CertificateError;
use certificate::PemCertificate;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::num::NonZeroU16;

/// loads tedge config from system default
pub fn get_tedge_config() -> Result<TEdgeConfig, TEdgeConfigError> {
    let tedge_config_location = TEdgeConfigLocation::default();
    TEdgeConfigRepository::new(tedge_config_location).load()
}

/// loads the new tedge config from system default
pub fn get_new_tedge_config() -> Result<new::TEdgeConfig, TEdgeConfigError> {
    let tedge_config_location = TEdgeConfigLocation::default();
    TEdgeConfigRepository::new(tedge_config_location).load_new()
}

/// Represents the complete configuration of a thin edge device.
/// This configuration is a wrapper over the device specific configurations
/// as well as the IoT cloud provider specific configurations.
///
#[derive(Debug)]
pub struct TEdgeConfig {
    pub(crate) data: new::TEdgeConfigDto,
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
            .ty
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_device_type.clone());
        Ok(device_type)
    }

    fn update(
        &mut self,
        _setting: DeviceTypeSetting,
        value: <DeviceTypeSetting as ConfigSetting>::Value,
    ) -> ConfigSettingResult<()> {
        self.data.device.ty = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: DeviceTypeSetting) -> ConfigSettingResult<()> {
        self.data.device.ty = None;
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
            "To set 'http.bind_address' to some <address>, you can `tedge config set mqtt.bind.address <address>`.",
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

#[allow(deprecated)]
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

impl ConfigSettingAccessor<C8yHttpSetting> for TEdgeConfig {
    #[allow(deprecated)]
    fn query(&self, _setting: C8yHttpSetting) -> ConfigSettingResult<HostPort<HTTPS_PORT>> {
        self.data
            .c8y
            .http
            .as_ref()
            .cloned()
            .or(self.data.c8y.url.as_ref().cloned().map(ConnectUrl::into))
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: C8yUrlSetting::KEY,
            })
    }

    fn update(
        &mut self,
        _setting: C8yHttpSetting,
        value: HostPort<HTTPS_PORT>,
    ) -> ConfigSettingResult<()> {
        self.data.c8y.http = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: C8yHttpSetting) -> ConfigSettingResult<()> {
        self.data.c8y.http = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<C8yMqttSetting> for TEdgeConfig {
    #[allow(deprecated)]
    fn query(&self, _setting: C8yMqttSetting) -> ConfigSettingResult<HostPort<MQTT_TLS_PORT>> {
        self.data
            .c8y
            .mqtt
            .as_ref()
            .cloned()
            .or(self.data.c8y.url.as_ref().cloned().map(ConnectUrl::into))
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: C8yUrlSetting::KEY,
            })
    }

    fn update(
        &mut self,
        _setting: C8yMqttSetting,
        value: HostPort<MQTT_TLS_PORT>,
    ) -> ConfigSettingResult<()> {
        self.data.c8y.mqtt = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: C8yMqttSetting) -> ConfigSettingResult<()> {
        self.data.c8y.mqtt = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<C8ySmartRestTemplates> for TEdgeConfig {
    fn query(&self, _setting: C8ySmartRestTemplates) -> ConfigSettingResult<TemplatesSet> {
        Ok(self
            .data
            .c8y
            .smartrest
            .templates
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_c8y_smartrest_templates.clone()))
    }

    fn update(
        &mut self,
        _setting: C8ySmartRestTemplates,
        value: TemplatesSet,
    ) -> ConfigSettingResult<()> {
        self.data.c8y.smartrest.templates = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: C8ySmartRestTemplates) -> ConfigSettingResult<()> {
        self.data.c8y.smartrest.templates = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<DeviceCertPathSetting> for TEdgeConfig {
    fn query(&self, _setting: DeviceCertPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
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
        value: Utf8PathBuf,
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
    fn query(&self, _setting: DeviceKeyPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
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
        value: Utf8PathBuf,
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
    fn query(&self, _setting: AzureRootCertPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
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
        value: Utf8PathBuf,
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
    fn query(&self, _setting: AwsRootCertPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
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
        value: Utf8PathBuf,
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
            .mapper
            .timestamp
            .map(Flag)
            .unwrap_or_else(|| self.config_defaults.default_mapper_timestamp.clone()))
    }

    fn update(&mut self, _setting: AzureMapperTimestamp, value: Flag) -> ConfigSettingResult<()> {
        self.data.az.mapper.timestamp = Some(value.into());
        Ok(())
    }

    fn unset(&mut self, _setting: AzureMapperTimestamp) -> ConfigSettingResult<()> {
        self.data.az.mapper.timestamp = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<AwsMapperTimestamp> for TEdgeConfig {
    fn query(&self, _setting: AwsMapperTimestamp) -> ConfigSettingResult<Flag> {
        Ok(self
            .data
            .aws
            .mapper
            .timestamp
            .map(Flag)
            .unwrap_or_else(|| self.config_defaults.default_mapper_timestamp.clone()))
    }

    fn update(&mut self, _setting: AwsMapperTimestamp, value: Flag) -> ConfigSettingResult<()> {
        self.data.aws.mapper.timestamp = Some(value.into());
        Ok(())
    }

    fn unset(&mut self, _setting: AwsMapperTimestamp) -> ConfigSettingResult<()> {
        self.data.aws.mapper.timestamp = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<C8yRootCertPathSetting> for TEdgeConfig {
    fn query(&self, _setting: C8yRootCertPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
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
        value: Utf8PathBuf,
    ) -> ConfigSettingResult<()> {
        self.data.c8y.root_cert_path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: C8yRootCertPathSetting) -> ConfigSettingResult<()> {
        self.data.c8y.root_cert_path = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttClientHostSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttClientHostSetting) -> ConfigSettingResult<String> {
        Ok(self
            .data
            .mqtt
            .client
            .host
            .clone()
            .unwrap_or(self.config_defaults.default_mqtt_client_host.clone()))
    }

    fn update(
        &mut self,
        _setting: MqttClientHostSetting,
        value: String,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.client.host = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttClientHostSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.client.host = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttClientPortSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttClientPortSetting) -> ConfigSettingResult<Port> {
        Ok(self
            .data
            .mqtt
            .client
            .port
            .map(|p| Port(p.into()))
            .unwrap_or_else(|| self.config_defaults.default_mqtt_port))
    }

    fn update(&mut self, _setting: MqttClientPortSetting, value: Port) -> ConfigSettingResult<()> {
        let port: u16 = value.into();
        let port: NonZeroU16 = port.try_into().map_err(|_| ConfigSettingError::Other {
            msg: "Can't use 0 for a client port",
        })?;
        self.data.mqtt.client.port = Some(port);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttClientPortSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.client.port = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttClientCafileSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttClientCafileSetting) -> ConfigSettingResult<Utf8PathBuf> {
        self.data
            .mqtt
            .client
            .auth
            .ca_file
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: "mqtt.client.auth.ca_file",
            })
    }

    fn update(
        &mut self,
        _setting: MqttClientCafileSetting,
        ca_file: Utf8PathBuf,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.client.auth.ca_file = Some(ca_file);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttClientCafileSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.client.auth.ca_file = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttClientCapathSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttClientCapathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        self.data
            .mqtt
            .client
            .auth
            .ca_dir
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: "mqtt.client.auth.ca_dir",
            })
    }

    fn update(
        &mut self,
        _setting: MqttClientCapathSetting,
        cafile: Utf8PathBuf,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.client.auth.ca_dir = Some(cafile);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttClientCapathSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.client.auth.ca_dir = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttClientAuthCertSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttClientAuthCertSetting) -> ConfigSettingResult<Utf8PathBuf> {
        self.data
            .mqtt
            .client
            .auth
            .cert_file
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: "mqtt.client.auth.cert_file",
            })
    }

    fn update(
        &mut self,
        _setting: MqttClientAuthCertSetting,
        cafile: Utf8PathBuf,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.client.auth.cert_file = Some(cafile);

        Ok(())
    }

    fn unset(&mut self, _setting: MqttClientAuthCertSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.client.auth.cert_file = None;

        Ok(())
    }
}

impl ConfigSettingAccessor<MqttClientAuthKeySetting> for TEdgeConfig {
    fn query(&self, _setting: MqttClientAuthKeySetting) -> ConfigSettingResult<Utf8PathBuf> {
        self.data
            .mqtt
            .client
            .auth
            .key_file
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: "mqtt.client.auth.key_file",
            })
    }

    fn update(
        &mut self,
        _setting: MqttClientAuthKeySetting,
        key_file: Utf8PathBuf,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.client.auth.key_file = Some(key_file);

        Ok(())
    }

    fn unset(&mut self, _setting: MqttClientAuthKeySetting) -> ConfigSettingResult<()> {
        self.data.mqtt.client.auth.key_file = None;

        Ok(())
    }
}

impl ConfigSettingAccessor<MqttPortSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttPortSetting) -> ConfigSettingResult<Port> {
        Ok(self
            .data
            .mqtt
            .bind
            .port
            .map(u16::from)
            .map(Port)
            .unwrap_or_else(|| self.config_defaults.default_mqtt_port))
    }

    fn update(&mut self, _setting: MqttPortSetting, value: Port) -> ConfigSettingResult<()> {
        self.data.mqtt.bind.port =
            Some(NonZeroU16::new(value.0).ok_or(ConfigSettingError::Other {
                msg: "mqtt.bind.port cannot be set to 0",
            })?);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttPortSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.bind.port = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<HttpPortSetting> for TEdgeConfig {
    fn query(&self, _setting: HttpPortSetting) -> ConfigSettingResult<Port> {
        Ok(self
            .data
            .http
            .bind
            .port
            .map(Port)
            .unwrap_or_else(|| self.config_defaults.default_http_port))
    }

    fn update(&mut self, _setting: HttpPortSetting, value: Port) -> ConfigSettingResult<()> {
        self.data.http.bind.port = Some(value.into());
        Ok(())
    }

    fn unset(&mut self, _setting: HttpPortSetting) -> ConfigSettingResult<()> {
        self.data.http.bind.port = None;
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
            .bind
            .address
            .map(IpAddress)
            .unwrap_or_else(|| self.config_defaults.default_mqtt_bind_address.clone()))
    }

    fn update(
        &mut self,
        _setting: MqttBindAddressSetting,
        value: IpAddress,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.bind.address = Some(value.0);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttBindAddressSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.bind.address = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttExternalPortSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttExternalPortSetting) -> ConfigSettingResult<Port> {
        self.data
            .mqtt
            .external
            .bind
            .port
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
        self.data.mqtt.external.bind.port = Some(value.into());
        Ok(())
    }

    fn unset(&mut self, _setting: MqttExternalPortSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.external.bind.port = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttExternalBindAddressSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttExternalBindAddressSetting) -> ConfigSettingResult<IpAddress> {
        self.data.mqtt.external.bind.address.map(IpAddress).ok_or(
            ConfigSettingError::ConfigNotSet {
                key: MqttExternalBindAddressSetting::KEY,
            },
        )
    }

    fn update(
        &mut self,
        _setting: MqttExternalBindAddressSetting,
        value: IpAddress,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.external.bind.address = Some(value.0);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttExternalBindAddressSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.external.bind.address = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttExternalBindInterfaceSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttExternalBindInterfaceSetting) -> ConfigSettingResult<String> {
        self.data
            .mqtt
            .external
            .bind
            .interface
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
        self.data.mqtt.external.bind.interface = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttExternalBindInterfaceSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.external.bind.interface = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttExternalCAPathSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttExternalCAPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        self.data
            .mqtt
            .external
            .ca_path
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: MqttExternalCAPathSetting::KEY,
            })
    }

    fn update(
        &mut self,
        _setting: MqttExternalCAPathSetting,
        value: Utf8PathBuf,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.external.ca_path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttExternalCAPathSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.external.ca_path = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttExternalCertfileSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttExternalCertfileSetting) -> ConfigSettingResult<Utf8PathBuf> {
        self.data
            .mqtt
            .external
            .cert_file
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: MqttExternalCertfileSetting::KEY,
            })
    }

    fn update(
        &mut self,
        _setting: MqttExternalCertfileSetting,
        value: Utf8PathBuf,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.external.cert_file = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttExternalCertfileSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.external.cert_file = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<MqttExternalKeyfileSetting> for TEdgeConfig {
    fn query(&self, _setting: MqttExternalKeyfileSetting) -> ConfigSettingResult<Utf8PathBuf> {
        self.data
            .mqtt
            .external
            .key_file
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet {
                key: MqttExternalKeyfileSetting::KEY,
            })
    }

    fn update(
        &mut self,
        _setting: MqttExternalKeyfileSetting,
        value: Utf8PathBuf,
    ) -> ConfigSettingResult<()> {
        self.data.mqtt.external.key_file = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: MqttExternalKeyfileSetting) -> ConfigSettingResult<()> {
        self.data.mqtt.external.key_file = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<SoftwarePluginDefaultSetting> for TEdgeConfig {
    fn query(&self, _setting: SoftwarePluginDefaultSetting) -> ConfigSettingResult<String> {
        self.data
            .software
            .plugin
            .default
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
        self.data.software.plugin.default = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: SoftwarePluginDefaultSetting) -> ConfigSettingResult<()> {
        self.data.software.plugin.default = None;
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
    fn query(&self, _setting: TmpPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        Ok(self
            .data
            .tmp
            .path
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_tmp_path.clone()))
    }

    fn update(&mut self, _setting: TmpPathSetting, value: Utf8PathBuf) -> ConfigSettingResult<()> {
        self.data.tmp.path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: TmpPathSetting) -> ConfigSettingResult<()> {
        self.data.tmp.path = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<LogPathSetting> for TEdgeConfig {
    fn query(&self, _setting: LogPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        Ok(self
            .data
            .logs
            .path
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_logs_path.clone()))
    }

    fn update(&mut self, _setting: LogPathSetting, value: Utf8PathBuf) -> ConfigSettingResult<()> {
        self.data.logs.path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: LogPathSetting) -> ConfigSettingResult<()> {
        self.data.logs.path = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<RunPathSetting> for TEdgeConfig {
    fn query(&self, _setting: RunPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        Ok(self
            .data
            .run
            .path
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_run_path.clone()))
    }

    fn update(&mut self, _setting: RunPathSetting, value: Utf8PathBuf) -> ConfigSettingResult<()> {
        self.data.run.path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: RunPathSetting) -> ConfigSettingResult<()> {
        self.data.run.path = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<DataPathSetting> for TEdgeConfig {
    fn query(&self, _setting: DataPathSetting) -> ConfigSettingResult<Utf8PathBuf> {
        Ok(self
            .data
            .data
            .path
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_data_path.clone()))
    }

    fn update(&mut self, _setting: DataPathSetting, value: Utf8PathBuf) -> ConfigSettingResult<()> {
        self.data.data.path = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: DataPathSetting) -> ConfigSettingResult<()> {
        self.data.data.path = None;
        Ok(())
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

    fn update(&mut self, _setting: LockFilesSetting, value: Flag) -> ConfigSettingResult<()> {
        self.data.run.lock_files = Some(value.into());
        Ok(())
    }

    fn unset(&mut self, _setting: LockFilesSetting) -> ConfigSettingResult<()> {
        self.data.run.lock_files = None;
        Ok(())
    }
}

impl ConfigSettingAccessor<FirmwareChildUpdateTimeoutSetting> for TEdgeConfig {
    fn query(&self, _setting: FirmwareChildUpdateTimeoutSetting) -> ConfigSettingResult<Seconds> {
        Ok(self
            .data
            .firmware
            .child
            .update
            .timeout
            .map(|s| Seconds(s.duration().as_secs()))
            .unwrap_or(self.config_defaults.default_firmware_child_update_timeout))
    }

    fn update(
        &mut self,
        _setting: FirmwareChildUpdateTimeoutSetting,
        value: Seconds,
    ) -> ConfigSettingResult<()> {
        self.data.firmware.child.update.timeout = Some(Seconds(value.into()));
        Ok(())
    }

    fn unset(&mut self, _setting: FirmwareChildUpdateTimeoutSetting) -> ConfigSettingResult<()> {
        self.data.firmware.child.update.timeout = Some(Seconds(
            self.config_defaults
                .default_firmware_child_update_timeout
                .into(),
        ));
        Ok(())
    }
}

impl ConfigSettingAccessor<ServiceTypeSetting> for TEdgeConfig {
    fn query(&self, _setting: ServiceTypeSetting) -> ConfigSettingResult<String> {
        Ok(self
            .data
            .service
            .ty
            .clone()
            .unwrap_or_else(|| self.config_defaults.default_service_type.clone()))
    }

    fn update(&mut self, _setting: ServiceTypeSetting, value: String) -> ConfigSettingResult<()> {
        self.data.service.ty = Some(value);
        Ok(())
    }

    fn unset(&mut self, _setting: ServiceTypeSetting) -> ConfigSettingResult<()> {
        self.data.service.ty = Some(self.config_defaults.default_service_type.clone());
        Ok(())
    }
}
