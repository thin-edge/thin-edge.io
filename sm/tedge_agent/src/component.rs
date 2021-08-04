use std::error::Error;

use async_trait::async_trait;
use tedge_config::TEdgeConfig;

#[async_trait]
pub trait TEdgeComponent {
    async fn start(&self) -> Result<(), Box<dyn Error>>;

    async fn start_with_config(&self, _tedge_config: TEdgeConfig) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}
