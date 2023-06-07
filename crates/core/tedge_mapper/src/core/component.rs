use std::path::Path;

use async_trait::async_trait;
use tedge_config::ConfigRepository;
use tedge_config::TEdgeConfig;

#[async_trait]
pub trait TEdgeComponent: Sync + Send {
    fn session_name(&self) -> &str;
    async fn start(&self, tedge_config: TEdgeConfig, cfg_dir: &Path) -> Result<(), anyhow::Error>;

    fn mqtt_config(&self) -> Result<mqtt_channel::Config, anyhow::Error> {
        let config_repository =
            tedge_config::TEdgeConfigRepository::new(tedge_config::TEdgeConfigLocation::default());
        let tedge_config = config_repository.load()?;

        let mqtt_config = tedge_config
            .mqtt_config()?
            .with_session_name(self.session_name())
            .with_clean_session(false);

        Ok(mqtt_config)
    }
}
