use std::str::FromStr;
use tedge_config::*;

/// Wrapper type for configuration keys.
///
/// This is used to reject invalid config keys during CLI parsing.
pub struct ConfigKey {
    pub key: &'static str,
    pub description: &'static str,
    pub get: GetConfigStringValue<TEdgeConfig>,
    pub set: SetConfigStringValue<TEdgeConfig>,
    pub unset: UnsetConfigValue<TEdgeConfig>,
}

type GetConfigStringValue<C> = Box<dyn Fn(&C) -> ConfigSettingResult<String>>;
type SetConfigStringValue<C> = Box<dyn Fn(&mut C, String) -> ConfigSettingResult<()>>;
type UnsetConfigValue<C> = Box<dyn Fn(&mut C) -> ConfigSettingResult<()>>;

impl std::fmt::Debug for ConfigKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ConfigKey({})", self.key)
    }
}

macro_rules! config_key {
    ($setting:tt) => {
        ConfigKey {
            key: $setting::KEY,
            description: $setting::DESCRIPTION,
            get: Box::new(move |config: &TEdgeConfig| config.query_string($setting)),
            set: Box::new(move |config: &mut TEdgeConfig, value: String| {
                config.update_string($setting, value)
            }),
            unset: Box::new(move |config: &mut TEdgeConfig| config.unset($setting)),
        }
    };
}

impl ConfigKey {
    #[allow(deprecated)]
    pub fn list_all() -> Vec<ConfigKey> {
        vec![
            config_key!(DeviceIdSetting),
            config_key!(DeviceTypeSetting),
            config_key!(DeviceKeyPathSetting),
            config_key!(DeviceCertPathSetting),
            config_key!(C8yUrlSetting),
            config_key!(C8yHttpSetting),
            config_key!(C8yMqttSetting),
            config_key!(C8yRootCertPathSetting),
            config_key!(C8ySmartRestTemplates),
            config_key!(AzureUrlSetting),
            config_key!(AzureRootCertPathSetting),
            config_key!(AwsUrlSetting),
            config_key!(AwsRootCertPathSetting),
            config_key!(AwsMapperTimestamp),
            config_key!(AzureMapperTimestamp),
            config_key!(MqttBindAddressSetting),
            config_key!(HttpBindAddressSetting),
            config_key!(MqttClientHostSetting),
            config_key!(MqttClientPortSetting),
            config_key!(MqttClientCafileSetting),
            config_key!(MqttClientCapathSetting),
            config_key!(MqttClientAuthCertSetting),
            config_key!(MqttClientAuthKeySetting),
            config_key!(MqttPortSetting),
            config_key!(HttpPortSetting),
            config_key!(MqttExternalPortSetting),
            config_key!(MqttExternalBindAddressSetting),
            config_key!(MqttExternalBindInterfaceSetting),
            config_key!(MqttExternalCAPathSetting),
            config_key!(MqttExternalCertfileSetting),
            config_key!(MqttExternalKeyfileSetting),
            config_key!(SoftwarePluginDefaultSetting),
            config_key!(TmpPathSetting),
            config_key!(LogPathSetting),
            config_key!(RunPathSetting),
            config_key!(DataPathSetting),
            config_key!(FirmwareChildUpdateTimeoutSetting),
            config_key!(ServiceTypeSetting),
            config_key!(LockFilesSetting),
        ]
    }
}

impl FromStr for ConfigKey {
    type Err = String;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        ConfigKey::list_all()
            .into_iter()
            .find(|config_key| config_key.key == key)
            .ok_or_else(|| {
                format!(
                    "Invalid key `{}'. See `tedge config list --doc` for a list of valid keys.",
                    key,
                )
            })
    }
}
