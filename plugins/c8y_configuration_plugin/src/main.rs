mod config;
mod smartrest;

use crate::config::PluginConfig;
use c8y_smartrest::topic::C8yTopic;
use mqtt_channel::{SinkExt, StreamExt};
use std::path::PathBuf;
use tedge_config::{get_tedge_config, ConfigSettingAccessor, MqttPortSetting};
use tracing::{debug, error, info, instrument, warn};

const CONFIG_ROOT_PATH: &str = "/etc/tedge/c8y";

async fn create_mqtt_client() -> Result<mqtt_channel::Connection, anyhow::Error> {
    let tedge_config = get_tedge_config()?;
    let mqtt_port = tedge_config.query(MqttPortSetting)?.into();
    let mqtt_config = mqtt_channel::Config::default()
        .with_port(mqtt_port)
        .with_subscriptions(mqtt_channel::TopicFilter::new_unchecked(
            C8yTopic::SmartRestRequest.as_str(),
        ));

    let mqtt_client = mqtt_channel::Connection::new(&mqtt_config).await?;
    Ok(mqtt_client)
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Create required clients
    let mut mqtt_client = create_mqtt_client().await?;

    let plugin_config = PluginConfig::new(PathBuf::from(CONFIG_ROOT_PATH));

    // Publish supported configuration types
    let msg = plugin_config.to_message()?;
    let () = mqtt_client.published.send(msg).await?;

    // mqtt loop
    while let Some(message) = mqtt_client.received.next().await {
        debug!("Received {:?}", message);
        match message.payload_str()?.split_at(3).0 {
            "524" => {
                debug!("{}", message.payload_str()?);
                todo!() // c8y_DownloadConfigFile
            }
            "526" => {
                debug!("{}", message.payload_str()?);
                todo!() // c8y_UploadConfigFile
            }
            _ => {}
        }
    }

    mqtt_client.close().await;

    Ok(())
}
