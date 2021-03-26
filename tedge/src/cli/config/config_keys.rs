use std::collections::HashMap;
use std::sync::Arc;
use tedge_config::*;

type GetValue = Box<dyn Fn(&TEdgeConfig) -> ConfigSettingResult<String>>;
type SetValue = Box<dyn Fn(&mut TEdgeConfig, String) -> ConfigSettingResult<()>>;
type UnsetValue = Box<dyn Fn(&mut TEdgeConfig) -> ConfigSettingResult<()>>;

pub struct ConfigKey {
    pub key: &'static str,
    pub description: &'static str,
    pub get_value: GetValue,
    set_value: Option<SetValue>,
    unset_value: Option<UnsetValue>,
}

impl std::fmt::Debug for ConfigKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ConfigKey({})", self.key)
    }
}

pub struct ConfigKeyRegistry {
    map: HashMap<&'static str, Arc<ConfigKey>>,
}

impl ConfigKeyRegistry {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    fn read_only<T>(mut self, setting: T) -> Self
    where
        T: ConfigSetting + Copy + 'static,
        TEdgeConfig: QueryStringSetting<T>,
    {
        self.map.insert(
            T::EXTERNAL_KEY,
            Arc::new(ConfigKey {
                key: T::EXTERNAL_KEY,
                description: T::DESCRIPTION,
                get_value: Box::new(move |cfg: &TEdgeConfig| cfg.query_string(setting)),
                set_value: None,
                unset_value: None,
            }),
        );
        self
    }

    fn read_write<T>(mut self, setting: T) -> Self
    where
        T: ConfigSetting + Copy + 'static,
        TEdgeConfig: QueryStringSetting<T> + UpdateStringSetting<T> + UnsetSetting<T>,
    {
        self.map.insert(
            T::EXTERNAL_KEY,
            Arc::new(ConfigKey {
                key: T::EXTERNAL_KEY,
                description: T::DESCRIPTION,
                get_value: Box::new(move |cfg: &TEdgeConfig| cfg.query_string(setting)),
                set_value: Some(Box::new(move |cfg: &mut TEdgeConfig, value: String| {
                    cfg.update_string(setting, value)
                })),
                unset_value: Some(Box::new(move |cfg: &mut TEdgeConfig| cfg.unset(setting))),
            }),
        );
        self
    }

    fn lookup(&self, key: &str) -> Option<Arc<ConfigKey>> {
        self.map.get(key).cloned()
    }

    pub fn iter(&self) -> impl Iterator<Item = &ConfigKey> {
        self.map.values().map(AsRef::as_ref)
    }

    pub fn all() -> Self {
        Self::new()
            .read_only(DeviceIdSetting)
            .read_write(DeviceKeyPathSetting)
            .read_write(DeviceCertPathSetting)
            .read_write(C8yUrlSetting)
            .read_write(C8yRootCertPathSetting)
            .read_write(AzureUrlSetting)
            .read_write(AzureRootCertPathSetting)
    }
}

pub const HELP_VALID_READ_ONLY_KEYS: &'static str = "One of:
    device.id
    device.key.path
    device.cert.path
    c8y.url
    c8y.root.cert.path
    azure.url
    azure.root.cert.path
";

pub const HELP_VALID_READ_WRITE_KEYS: &'static str = "One of:
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

/// Wrapper type for read-only configuration keys.
#[derive(Debug)]
pub struct ReadOnlyConfigKey(Arc<ConfigKey>);

impl ReadOnlyConfigKey {
    pub fn as_str(&self) -> &str {
        &self.0.key
    }

    pub fn get_config_value(&self, config: &TEdgeConfig) -> ConfigSettingResult<String> {
        (self.0.get_value)(config)
    }
}

impl std::str::FromStr for ReadOnlyConfigKey {
    type Err = String;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        ConfigKeyRegistry::all()
            .lookup(key)
            .ok_or_else(|| {
                format!(
                    "Invalid key `{}'. See `tedge config get -h` for valid keys.",
                    key,
                )
            })
            .map(ReadOnlyConfigKey)
    }
}

/// Wrapper type for read-write configuration keys.
///
#[derive(Debug)]
pub struct ReadWriteConfigKey(Arc<ConfigKey>);

impl ReadWriteConfigKey {
    pub fn as_str(&self) -> &str {
        &self.0.key
    }

    pub fn set_config_value(
        &self,
        config: &mut TEdgeConfig,
        value: String,
    ) -> ConfigSettingResult<()> {
        (self.0.set_value.as_ref().unwrap())(config, value)
    }
}

impl std::str::FromStr for ReadWriteConfigKey {
    type Err = String;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        ConfigKeyRegistry::all()
            .lookup(key)
            .ok_or_else(|| {
                format!(
                    "Invalid key `{}'. See `tedge config set -h` for valid keys.",
                    key,
                )
            })
            .and_then(|config_key| {
                if config_key.set_value.is_none() {
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
                } else {
                    Ok(ReadWriteConfigKey(config_key))
                }
            })
    }
}

/// Wrapper type for unsettable configuration keys.
///
#[derive(Debug)]
pub struct UnsettableConfigKey(Arc<ConfigKey>);

impl UnsettableConfigKey {
    pub fn as_str(&self) -> &str {
        &self.0.key
    }

    pub fn unset_config_value(&self, config: &mut TEdgeConfig) -> ConfigSettingResult<()> {
        (self.0.unset_value.as_ref().unwrap())(config)
    }
}

impl std::str::FromStr for UnsettableConfigKey {
    type Err = String;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        ConfigKeyRegistry::all()
            .lookup(key)
            .ok_or_else(|| {
                format!(
                    "Invalid key `{}'. See `tedge config unset -h` for valid keys.",
                    key,
                )
            })
            .and_then(|config_key| {
                if config_key.unset_value.is_none() {
                    Err(format!(
                        "Invalid key `{}'. See `tedge config unset -h` for valid keys.",
                        key,
                    ))
                } else {
                    Ok(UnsettableConfigKey(config_key))
                }
            })
    }
}
