use async_trait::async_trait;
use tedge_config::TEdgeConfig;

#[async_trait]
pub trait TEdgeComponent {
    async fn start(&self, tedge_config: TEdgeConfig) -> Result<(), anyhow::Error>;
    async fn init(&self) -> Result<(), anyhow::Error>;
    async fn clear_session(&self) -> Result<(), anyhow::Error>;
}
