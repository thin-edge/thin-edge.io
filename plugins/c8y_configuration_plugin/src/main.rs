mod config;
mod download;
mod error;
mod smartrest;
mod upload;

use crate::config::PluginConfig;
use crate::download::handle_config_download_request;
use crate::upload::handle_config_upload_request;
use anyhow::Result;
use c8y_api::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use c8y_smartrest::smartrest_deserializer::SmartRestConfigDownloadRequest;
use c8y_smartrest::{smartrest_deserializer::SmartRestConfigUploadRequest, topic::C8yTopic};
use mqtt_channel::{SinkExt, StreamExt};
use std::path::PathBuf;
use tedge_config::{get_tedge_config, ConfigSettingAccessor, MqttPortSetting};
use tracing::{debug, error};

const CONFIG_ROOT_PATH: &str = "/etc/tedge/c8y";

#[cfg(not(debug_assertions))]
const LOG_LEVEL_DEBUG: bool = false;

#[cfg(debug_assertions)]
const LOG_LEVEL_DEBUG: bool = false;

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

pub async fn create_http_client() -> Result<JwtAuthHttpProxy, anyhow::Error> {
    let config = get_tedge_config()?;
    let mut http_proxy = JwtAuthHttpProxy::try_new(&config).await?;
    let () = http_proxy.init().await?;
    Ok(http_proxy)
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tedge_utils::logging::initialise_tracing_subscriber(LOG_LEVEL_DEBUG);

    // Create required clients
    let mut mqtt_client = create_mqtt_client().await?;
    let mut http_client = create_http_client().await?;

    let plugin_config = PluginConfig::new(PathBuf::from(CONFIG_ROOT_PATH));

    // Publish supported configuration types
    let msg = plugin_config.to_message()?;
    let () = mqtt_client.published.send(msg).await?;

    // Mqtt message loop
    while let Some(message) = mqtt_client.received.next().await {
        debug!("Received {:?}", message);
        if let Ok(payload) = message.payload_str() {
            let result = match payload.split(',').next().unwrap_or_default() {
                "524" => {
                    let config_download_request =
                        SmartRestConfigDownloadRequest::from_smartrest(payload)?;

                    handle_config_download_request(
                        &plugin_config,
                        config_download_request,
                        &mut mqtt_client,
                        &mut http_client,
                    )
                    .await
                }
                "526" => {
                    // retrieve config file upload smartrest request from payload
                    let config_upload_request =
                        SmartRestConfigUploadRequest::from_smartrest(payload)?;

                    // handle the config file upload request
                    handle_config_upload_request(
                        config_upload_request,
                        &mut mqtt_client,
                        &mut http_client,
                    )
                    .await
                }
                _ => {
                    // Ignore operation messages not meant for this plugin
                    Ok(())
                }
            };

            if let Err(err) = result {
                error!("Handling of operation: '{}' failed with {}", payload, err);
            }
        }
    }

    mqtt_client.close().await;

    Ok(())
}
