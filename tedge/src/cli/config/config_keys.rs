use crate::{config::*, settings::*};

pub struct ConfigKey {
    pub key: &'static str,
    pub getter: Option<Box<dyn GetStringConfigSetting<Config = TEdgeConfig>>>,
    pub setter: Option<Box<dyn SetStringConfigSetting<Config = TEdgeConfig>>>,
    pub unsetter: Option<Box<dyn UnsetConfigSetting<Config = TEdgeConfig>>>,
    pub description: &'static str,
}

impl ConfigKey {
    pub fn all() -> Vec<Self> {
        vec![
            ConfigKey {
                key: "device.id",
                getter: Some(Box::new(DeviceIdSetting)),
                setter: None,
                unsetter: None,
                description: "Identifier of the device within the fleet. It must be globally unique and the same one used in the device certificate. Example: Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665",
            },
            ConfigKey {
                key: "device.key.path",
                getter: Some(Box::new(DeviceKeyPathSetting)),
                setter: Some(Box::new(DeviceKeyPathSetting)),
                unsetter: Some(Box::new(DeviceKeyPathSetting)),
                description: "Path to the private key file. Example: /home/user/.tedge/tedge-private-key.pem"
            },
            ConfigKey {
                key: "device.cert.path",
                getter: Some(Box::new(DeviceCertPathSetting)),
                setter: Some(Box::new(DeviceCertPathSetting)),
                unsetter: Some(Box::new(DeviceCertPathSetting)),
                description: "Path to the certificate file. Example: /home/user/.tedge/tedge-certificate.crt"
            },
            ConfigKey {
                key: "c8y.url",
                getter: Some(Box::new(C8yUrlSetting)),
                setter: Some(Box::new(C8yUrlSetting)),
                unsetter: Some(Box::new(C8yUrlSetting)),
                description: "Tenant endpoint URL of Cumulocity tenant. Example: your-tenant.cumulocity.com",
            },
            ConfigKey {
                key: "c8y.root.cert.path",
                getter: Some(Box::new(C8yRootCertPathSetting)),
                setter: Some(Box::new(C8yRootCertPathSetting)),
                unsetter: Some(Box::new(C8yRootCertPathSetting)),
description: "Path where Cumulocity root certificate(s) are located. Example: /home/user/.tedge/c8y-trusted-root-certificates.pem"
            },
            ConfigKey {
                key: "azure.url",
                getter: Some(Box::new(AzureUrlSetting)),
                setter: Some(Box::new(AzureUrlSetting)),
                unsetter: Some(Box::new(AzureUrlSetting)),
                description: "Tenant endpoint URL of Azure IoT tenant. Example:  MyAzure.azure-devices.net"
            },
            ConfigKey {
                key: "azure.root.cert.path",
                getter: Some(Box::new(AzureRootCertPathSetting)),
                setter: Some(Box::new(AzureRootCertPathSetting)),
                unsetter: Some(Box::new(AzureRootCertPathSetting)),
                description: "Path where Azure IoT root certificate(s) are located. Example: /home/user/.tedge/azure-trusted-root-certificates.pem"
            },
        ]
    }
}

pub const HELP_VALID_READABLE_KEYS: &'static str = "One of:
    device.id
    device.key.path
    device.cert.path
    c8y.url
    c8y.root.cert.path
    azure.url
    azure.root.cert.path
";

pub const HELP_VALID_WRITABLE_KEYS: &'static str = "One of:
    device.key.path
    device.cert.path
    c8y.url
    c8y.root.cert.path
    azure.url
    azure.root.cert.path
";

pub const HELP_VALID_UNSETTABLE_KEYS: &'static str = "One of:
    device.key.path
    device.cert.path
    c8y.url
    c8y.root.cert.path
    azure.url
    azure.root.cert.path
";

/// Wrapper type for readable configuration keys.
#[derive(Debug)]
pub struct ReadableConfigKey {
    key: String,
    getter: Box<dyn GetStringConfigSetting<Config = TEdgeConfig>>,
}

impl ReadableConfigKey {
    pub fn as_str(&self) -> &str {
        self.key.as_str()
    }
    pub fn get_config_value(&self, config: &TEdgeConfig) -> ConfigSettingResult<String> {
        self.getter.get_string(config)
    }
}

impl std::str::FromStr for ReadableConfigKey {
    type Err = String;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        for config_key in ConfigKey::all().into_iter() {
            match config_key {
                ConfigKey {
                    key: k,
                    getter: Some(getter),
                    ..
                } if k == key => {
                    return Ok(Self {
                        key: key.into(),
                        getter,
                    })
                }
                _ => {}
            }
        }
        Err(format!(
            "Invalid key `{}'. See `tedge config get -h` for valid keys.",
            key,
        ))
    }
}

/// Wrapper type for updatable (Read-Write mode) configuration keys.
///
#[derive(Debug)]
pub struct WritableConfigKey {
    key: String,
    setter: Box<dyn SetStringConfigSetting<Config = TEdgeConfig>>,
}

impl WritableConfigKey {
    pub fn as_str(&self) -> &str {
        self.key.as_str()
    }
    pub fn set_config_value(
        &self,
        config: &mut TEdgeConfig,
        value: String,
    ) -> ConfigSettingResult<()> {
        self.setter.set_string(config, value)
    }
}

impl std::str::FromStr for WritableConfigKey {
    type Err = String;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        for config_key in ConfigKey::all().into_iter() {
            match config_key {
                ConfigKey {
                    key: k,
                    setter: Some(setter),
                    ..
                } if k == key => {
                    return Ok(Self {
                        key: key.into(),
                        setter,
                    })
                }
                _ => {}
            }
        }

        match key {
            "device.id" => Err("Invalid key `device.id'. \n\
                Setting the device id is only allowed with tedge cert create. \
                To set 'device.id', use `tedge cert create --device-id <id>`."
                .into()),
            _ => Err(format!(
                "Invalid key `{}'. See `tedge config set -h` for valid keys.",
                key,
            )),
        }
    }
}

#[derive(Debug)]
pub struct UnsettableConfigKey {
    key: String,
    unsetter: Box<dyn UnsetConfigSetting<Config = TEdgeConfig>>,
}

impl UnsettableConfigKey {
    pub fn as_str(&self) -> &str {
        self.key.as_str()
    }
    pub fn unset_config_value(&self, config: &mut TEdgeConfig) -> ConfigSettingResult<()> {
        self.unsetter.unset(config)
    }
}

impl std::str::FromStr for UnsettableConfigKey {
    type Err = String;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        for config_key in ConfigKey::all().into_iter() {
            match config_key {
                ConfigKey {
                    key: k,
                    unsetter: Some(unsetter),
                    ..
                } if k == key => {
                    return Ok(Self {
                        key: key.into(),
                        unsetter,
                    })
                }
                _ => {}
            }
        }

        Err(format!(
            "Invalid key `{}'. See `tedge config unset -h` for valid keys.",
            key,
        ))
    }
}
