mod c8y_http_proxy;
mod config_manager;
mod file_system_ext;

use crate::c8y_http_proxy::credentials::C8YJwtRetriever;
use crate::c8y_http_proxy::C8YHttpConfig;
use crate::c8y_http_proxy::C8YHttpProxyBuilder;
use crate::config_manager::ConfigManagerBuilder;
use crate::config_manager::ConfigManagerConfig;
use file_system_ext::FsWatchActorBuilder;
use tedge_actors::Runtime;
use tedge_http_ext::HttpActorBuilder;
use tedge_http_ext::HttpConfig;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_signal_ext::SignalActor;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let runtime_events_logger = None;
    let mut runtime = Runtime::try_new(runtime_events_logger).await?;

    // Create actor instances
    let mut mqtt_actor = MqttActorBuilder::new(mqtt_channel::Config::default());
    let mut jwt_actor = C8YJwtRetriever::builder(mqtt_channel::Config::default());
    let mut http_actor = HttpActorBuilder::new(HttpConfig::default())?;
    let mut c8y_http_proxy_actor = C8YHttpProxyBuilder::new(
        C8YHttpConfig::new("thin-edge-io.eu-latest.cumulocity.com", "albin-tedge"), //FIXME: Read from tedge config
        &mut http_actor,
        &mut jwt_actor,
    );
    let mut fs_watch_actor = FsWatchActorBuilder::new();
    let mut config_actor =
        ConfigManagerBuilder::new(ConfigManagerConfig::from_default_tedge_config()?);
    let signal_actor = SignalActor::builder();

    // Connect actor instances
    config_actor.with_fs_connection(&mut fs_watch_actor)?;
    config_actor.with_c8y_http_proxy(&mut c8y_http_proxy_actor)?;
    config_actor.with_mqtt_connection(&mut mqtt_actor)?;

    // Run the actors
    // FIXME having to list all the actors is error prone
    runtime.spawn(signal_actor).await?;
    runtime.spawn(mqtt_actor).await?;
    runtime.spawn(jwt_actor).await?;
    runtime.spawn(http_actor).await?;
    runtime.spawn(c8y_http_proxy_actor).await?;
    runtime.spawn(fs_watch_actor).await?;
    runtime.spawn(config_actor).await?;

    runtime.run_to_completion().await?;
    Ok(())
}
