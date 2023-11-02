use async_trait::async_trait;
use miette::IntoDiagnostic;
use std::path::Path;
use tedge_config::TEdgeConfig;

#[async_trait]
pub trait TEdgeComponent: Sync + Send {
    fn session_name(&self) -> &str;
    async fn start(&self, tedge_config: TEdgeConfig, cfg_dir: &Path) -> Result<(), miette::Error>;

    fn mqtt_config(&self) -> Result<mqtt_channel::Config, miette::Error> {
        let config_repository =
            tedge_config::TEdgeConfigRepository::new(tedge_config::TEdgeConfigLocation::default());
        let tedge_config = config_repository.load().into_diagnostic()?;

        let mqtt_config = tedge_config
            .mqtt_config()
            .into_diagnostic()?
            .with_session_name(self.session_name())
            .with_clean_session(false);

        Ok(mqtt_config)
    }
}
