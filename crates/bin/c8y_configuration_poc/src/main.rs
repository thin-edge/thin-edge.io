mod config_ext;
mod file_system_ext;
mod mqtt_ext;

use crate::config_ext::{ConfigConfigManager, ConfigManager};
use crate::mqtt_ext::MqttConfig;
use std::path::PathBuf;
use tedge_actors::Runtime;
use tedge_http_ext::HttpConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime_events_logger = None;
    let mut runtime = Runtime::try_new(runtime_events_logger).await?;

    let mut http_actor =
        tedge_http_ext::HttpActorInstance::new(tedge_http_ext::HttpConfig::default())?;
    let config_actor = ConfigManager::new(
        ConfigConfigManager {
            mqtt_conf: MqttConfig {},
            http_conf: HttpConfig {},
            config_dir: PathBuf::from("/etc/tedge".to_string()),
        },
        &mut http_actor,
    );

    runtime.spawn(http_actor).await?;
    runtime.spawn(config_actor).await?;

    runtime.run_to_completion().await?;
    Ok(())
}
