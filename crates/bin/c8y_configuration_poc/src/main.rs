mod c8y_http_proxy;
mod config_manager;
mod file_system_ext;
mod log_manager;

use crate::c8y_http_proxy::credentials::C8YJwtRetriever;
use crate::c8y_http_proxy::C8YHttpConfig;
use crate::c8y_http_proxy::C8YHttpProxyBuilder;
use crate::config_manager::ConfigManagerBuilder;
use crate::config_manager::ConfigManagerConfig;
use file_system_ext::FsWatchActorBuilder;
use log_manager::LogManagerBuilder;
use log_manager::LogManagerConfig;
use tedge_actors::Runtime;
use tedge_config::get_tedge_config;
use tedge_http_ext::HttpActorBuilder;
use tedge_http_ext::HttpConfig;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_signal_ext::SignalActor;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let runtime_events_logger = None;
    let mut runtime = Runtime::try_new(runtime_events_logger).await?;

    let tedge_config = get_tedge_config()?;

    // Create actor instances
    let mut mqtt_actor = MqttActorBuilder::new(mqtt_channel::Config::default());
    let mut jwt_actor = C8YJwtRetriever::builder(mqtt_channel::Config::default());
    let mut http_actor = HttpActorBuilder::new(HttpConfig::default())?;
    let mut c8y_http_proxy_actor =
        C8YHttpProxyBuilder::new(tedge_config.try_into()?, &mut http_actor, &mut jwt_actor);
    let mut fs_watch_actor = FsWatchActorBuilder::new();
    let signal_actor = SignalActor::builder();

    //Instantiate config manager actor
    let mut config_actor =
        ConfigManagerBuilder::new(ConfigManagerConfig::from_default_tedge_config()?);

    // Connect other actor instances to config manager actor
    config_actor.with_fs_connection(&mut fs_watch_actor)?;
    config_actor.with_c8y_http_proxy(&mut c8y_http_proxy_actor)?;
    config_actor.with_mqtt_connection(&mut mqtt_actor)?;

    //Instantiate log manager actor
    let mut log_actor = LogManagerBuilder::new(LogManagerConfig::from_default_tedge_config()?);

    // Connect other actor instances to log manager actor
    log_actor.with_fs_connection(&mut fs_watch_actor)?;
    log_actor.with_c8y_http_proxy(&mut c8y_http_proxy_actor)?;
    log_actor.with_mqtt_connection(&mut mqtt_actor)?;

    // Run the actors
    // FIXME having to list all the actors is error prone
    runtime.spawn(signal_actor).await?;
    runtime.spawn(mqtt_actor).await?;
    runtime.spawn(jwt_actor).await?;
    runtime.spawn(http_actor).await?;
    runtime.spawn(c8y_http_proxy_actor).await?;
    runtime.spawn(fs_watch_actor).await?;
    runtime.spawn(config_actor).await?;
    runtime.spawn(log_actor).await?;

    runtime.run_to_completion().await?;
    Ok(())
}
