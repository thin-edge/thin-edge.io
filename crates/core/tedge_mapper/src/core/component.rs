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
}
