use miette::Context;
use miette::IntoDiagnostic;
use serde::Deserialize;
use tokio::fs::read_to_string;

#[derive(Deserialize)]
pub struct TedgeConfig {
    pub c8y: TedgeC8yConfig,
    #[serde(default)]
    pub mqtt: TedgeMqttConfig,
}

#[derive(Deserialize)]
pub struct TedgeC8yConfig {
    pub url: String,
}

#[derive(Deserialize, Default)]
pub struct TedgeMqttConfig {
    pub port: Option<u16>,
    pub bind_address: Option<String>,
}

impl TedgeConfig {
    pub async fn read_from_disk() -> miette::Result<Self> {
        let path = "/etc/tedge/tedge.toml";
        let config = read_to_string(path)
            .await
            .into_diagnostic()
            .with_context(|| format!("Reading {path}"))?;

        toml::from_str(&config)
            .into_diagnostic()
            .with_context(|| format!("Parsing {path}"))
    }
}
