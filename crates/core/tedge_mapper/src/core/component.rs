use async_trait::async_trait;
use tedge_config::{
    ConfigRepository, ConfigSettingAccessor, MqttBindAddressSetting, MqttPortSetting, TEdgeConfig,
};
use tracing::info;

#[async_trait]
pub trait TEdgeComponent: Sync + Send {
    fn session_name(&self) -> &str;
    async fn start(&self, tedge_config: TEdgeConfig) -> Result<(), anyhow::Error>;
    async fn init(&self) -> Result<(), anyhow::Error>;

    async fn clear_session(&self) -> Result<(), anyhow::Error> {
        info!("Clear {} session", self.session_name());
        mqtt_channel::clear_session(&self.get_mqtt_config()?).await?;
        Ok(())
    }

    fn get_mqtt_config(&self) -> Result<mqtt_channel::Config, anyhow::Error> {
        let config_repository =
            tedge_config::TEdgeConfigRepository::new(tedge_config::TEdgeConfigLocation::default());
        let tedge_config = config_repository.load()?;

        let mqtt_config = mqtt_channel::Config::default()
            .with_host(tedge_config.query(MqttBindAddressSetting)?.to_string())
            .with_port(tedge_config.query(MqttPortSetting)?.into())
            .with_session_name(self.session_name())
            .with_clean_session(false);

        Ok(mqtt_config)
    }
}
