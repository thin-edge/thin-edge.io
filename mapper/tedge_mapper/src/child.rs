use async_trait::async_trait;
use tedge_config::TEdgeConfig;

#[async_trait]
pub trait SupportChildren {
    async fn add_child(&self, child_id: &str, client: &mqtt_client::Client) -> Result<(), anyhow::Error>;
}
