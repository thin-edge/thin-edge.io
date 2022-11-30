mod config_ext;
mod file_system_ext;
mod http_ext;
mod mqtt_ext;

use crate::http_ext::HttpConfig;
use crate::mqtt_ext::MqttConfig;
use std::path::PathBuf;
use tedge_actors::Runtime;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime_events_logger = None;
    let runtime = Runtime::try_new(runtime_events_logger).await?;

    let config_actor_builder = config_ext::ConfigActorBuilder {
        mqtt_conf: MqttConfig::default(),
        http_conf: HttpConfig::default(),
        config_dir: PathBuf::from("/etc/tedge/"),
    };

    config_actor_builder
        .spawn_actor(runtime.get_handle())
        .await?;

    runtime.run_to_completion().await?;
    Ok(())
}
