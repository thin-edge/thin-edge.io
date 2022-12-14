mod c8y_http_proxy;
mod config_manager;
mod file_system_ext;
mod mqtt_ext;

use crate::config_manager::{ConfigManager, ConfigManagerConfig};
use tedge_actors::Runtime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime_events_logger = None;
    let mut runtime = Runtime::try_new(runtime_events_logger).await?;

    // Create actor instances
    let mut http_actor =
        tedge_http_ext::HttpActorBuilder::new(tedge_http_ext::HttpConfig::default())?;
    let mut config_actor =
        ConfigManager::new(ConfigManagerConfig::from_tedge_config("/etc/tedge")?);

    // Connect actor instances
    config_actor.with_http_connection(&mut http_actor)?;

    // Run the actors
    runtime.spawn(http_actor).await?;
    runtime.spawn(config_actor).await?;

    runtime.run_to_completion().await?;
    Ok(())
}
