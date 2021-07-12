use async_trait::async_trait;
use tedge_config::TEdgeConfig;

#[async_trait]
pub trait TEdgeComponent {
    async fn start(&self) -> Result<(), anyhow::Error>;

    async fn start_with_config(&self, tedge_config: TEdgeConfig) -> Result<(), anyhow::Error> {
        Ok(())
    }
}
