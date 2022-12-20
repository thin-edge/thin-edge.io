mod c8y_http_proxy;
mod config_manager;
mod file_system_ext;
mod mqtt_ext;

use crate::c8y_http_proxy::{C8YHttpConfig, C8YHttpProxyBuilder};
use crate::config_manager::{ConfigManagerBuilder, ConfigManagerConfig};
use crate::mqtt_ext::MqttActorBuilder;
use file_system_ext::FsWatchActorBuilder;
use tedge_actors::Runtime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime_events_logger = None;
    let mut runtime = Runtime::try_new(runtime_events_logger).await?;

    // Create actor instances
    let mut mqtt_actor = MqttActorBuilder::new(mqtt_channel::Config::default());
    let mut http_actor =
        tedge_http_ext::HttpActorBuilder::new(tedge_http_ext::HttpConfig::default())?;
    let mut c8y_http_proxy_actor = C8YHttpProxyBuilder::new(C8YHttpConfig::default());
    let mut fs_watch_actor = FsWatchActorBuilder::new();
    let mut config_actor =
        ConfigManagerBuilder::new(ConfigManagerConfig::from_default_tedge_config()?);

    // Connect actor instances
    c8y_http_proxy_actor.with_http_connection(&mut http_actor)?;
    config_actor.with_fs_connection(&mut fs_watch_actor)?;
    config_actor.with_c8y_http_proxy(&mut c8y_http_proxy_actor)?;
    config_actor.with_mqtt_connection(&mut mqtt_actor)?;

    // Run the actors
    runtime.spawn(mqtt_actor).await?;
    runtime.spawn(http_actor).await?;
    runtime.spawn(fs_watch_actor).await?;
    runtime.spawn(config_actor).await?;

    runtime.run_to_completion().await?;
    Ok(())
}
