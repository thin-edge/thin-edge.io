use miette::Context;
use miette::IntoDiagnostic;
use std::net::IpAddr;

pub struct TedgeConfig {
    pub c8y: TedgeC8yConfig,
    pub mqtt: TedgeMqttConfig,
}

pub struct TedgeC8yConfig {
    pub url: String,
}

pub struct TedgeMqttConfig {
    pub port: u16,
    pub bind_address: IpAddr,
}

impl TedgeConfig {
    pub fn read_from_disk() -> miette::Result<Self> {
        use tedge_config::C8yUrlSetting;
        use tedge_config::ConfigSettingAccessor;
        use tedge_config::MqttBindAddressSetting;
        use tedge_config::MqttPortSetting;
        let config = tedge_config::get_tedge_config()
            .into_diagnostic()
            .context("Reading config")?;

        Ok(Self {
            c8y: TedgeC8yConfig {
                url: config
                    .query(C8yUrlSetting)
                    .into_diagnostic()?
                    .as_str()
                    .to_owned(),
            },
            mqtt: TedgeMqttConfig {
                port: config.query(MqttPortSetting).into_diagnostic()?.0,
                bind_address: config.query(MqttBindAddressSetting).into_diagnostic()?.0,
            },
        })
    }
}
