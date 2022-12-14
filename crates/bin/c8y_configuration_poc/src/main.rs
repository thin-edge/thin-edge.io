mod c8y_http_proxy;
mod config_manager;
mod file_system_ext;
mod mqtt_ext;

use crate::config_manager::{ConfigManagerBuilder, ConfigManagerConfig};
use crate::mqtt_ext::MqttActorBuilder;
use tedge_actors::Runtime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime_events_logger = None;
    let mut runtime = Runtime::try_new(runtime_events_logger).await?;

    // Create actor instances
    let mut mqtt_actor_builder = MqttActorBuilder::new(mqtt_channel::Config::default());
    let mut http_actor =
        tedge_http_ext::HttpActorBuilder::new(tedge_http_ext::HttpConfig::default())?;
    let mut config_actor =
        ConfigManagerBuilder::new(ConfigManagerConfig::from_tedge_config("/etc/tedge")?);

    // Connect actor instances
    config_actor.with_http_connection(&mut http_actor)?;
    config_actor.with_mqtt_connection(&mut mqtt_actor_builder)?;

    // Run the actors
    runtime.spawn(mqtt_actor_builder).await?;
    runtime.spawn(http_actor).await?;
    runtime.spawn(config_actor).await?;

    runtime.run_to_completion().await?;
    Ok(())
}
