use crate::{config::*, settings::*, types::*};
use std::convert::TryFrom;

///
/// Tenant endpoint URL of Cumulocity tenant. Example: your-tenant.cumulocity.com
///
#[derive(Debug)]
pub struct C8yUrlSetting;

impl GetConfigSetting for C8yUrlSetting {
    type Config = TEdgeConfig;
    type Value = ConnectUrl;

    fn get(&self, config: &Self::Config) -> ConfigSettingResult<Self::Value> {
        config
            .c8y
            .url
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet { key: "c8y.url" })
    }
}

impl GetStringConfigSetting for C8yUrlSetting {
    type Config = TEdgeConfig;

    fn get_string(&self, config: &Self::Config) -> ConfigSettingResult<String> {
        self.get(config).map(Into::into)
    }
}

impl SetStringConfigSetting for C8yUrlSetting {
    type Config = TEdgeConfig;

    fn set_string(&self, config: &mut Self::Config, value: String) -> ConfigSettingResult<()> {
        let c8y_url = ConnectUrl::try_from(value)
            .map_err(|err: InvalidConnectUrl| ConfigSettingError::InvalidConfigUrl(err.0))?;

        config.c8y.url = Some(c8y_url.into());

        Ok(())
    }
}

impl UnsetConfigSetting for C8yUrlSetting {
    type Config = TEdgeConfig;

    fn unset(&self, config: &mut Self::Config) -> ConfigSettingResult<()> {
        config.c8y.url = None;
        Ok(())
    }
}
