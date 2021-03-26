use crate::{config::*, settings::*, types::*};
use std::convert::TryFrom;

///
/// Tenant endpoint URL of Azure IoT tenant. Example:  MyAzure.azure-devices.net
///
#[derive(Debug)]
pub struct AzureUrlSetting;

impl GetConfigSetting for AzureUrlSetting {
    type Config = TEdgeConfig;
    type Value = ConnectUrl;

    fn get(&self, config: &Self::Config) -> ConfigSettingResult<Self::Value> {
        config
            .azure
            .url
            .clone()
            .ok_or(ConfigSettingError::ConfigNotSet { key: "azure.url" })
    }
}

impl GetStringConfigSetting for AzureUrlSetting {
    type Config = TEdgeConfig;

    fn get_string(&self, config: &Self::Config) -> ConfigSettingResult<String> {
        self.get(config).map(Into::into)
    }
}

impl SetStringConfigSetting for AzureUrlSetting {
    type Config = TEdgeConfig;

    fn set_string(&self, config: &mut Self::Config, value: String) -> ConfigSettingResult<()> {
        let azure_url = ConnectUrl::try_from(value)
            .map_err(|err: InvalidConnectUrl| ConfigSettingError::InvalidConfigUrl(err.0))?;

        config.azure.url = Some(azure_url.into());

        Ok(())
    }
}

impl UnsetConfigSetting for AzureUrlSetting {
    type Config = TEdgeConfig;

    fn unset(&self, config: &mut Self::Config) -> ConfigSettingResult<()> {
        config.azure.url = None;
        Ok(())
    }
}
