use async_trait::async_trait;
use tedge_config::TEdgeConfig;

#[async_trait]
pub trait TEdgeComponent: Sync + Send {
    fn session_name(&self) -> &str;
    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        cfg_dir: &tedge_config::Path,
    ) -> Result<(), anyhow::Error>;

    fn mqtt_config(&self) -> Result<mqtt_channel::Config, anyhow::Error> {
        let tedge_config =
            tedge_config::TEdgeConfig::try_new(tedge_config::TEdgeConfigLocation::default())?;

        let mqtt_config = tedge_config
            .mqtt_config()?
            .with_session_name(self.session_name())
            .with_clean_session(false);

        Ok(mqtt_config)
    }
}
