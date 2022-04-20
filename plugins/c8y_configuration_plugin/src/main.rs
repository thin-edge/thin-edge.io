mod config;
mod download;
mod error;
mod smartrest;

use crate::config::PluginConfig;
use crate::download::handle_config_download_request;
use anyhow::Result;
use c8y_api::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use c8y_smartrest::smartrest_deserializer::SmartRestConfigDownloadRequest;
use c8y_smartrest::{
    smartrest_deserializer::SmartRestConfigUploadRequest,
    smartrest_serializer::{
        CumulocitySupportedOperations, SmartRestSerializer, SmartRestSetOperationToExecuting,
        SmartRestSetOperationToFailed, SmartRestSetOperationToSuccessful,
    },
    topic::C8yTopic,
};
use mqtt_channel::{Connection, Message, SinkExt, StreamExt};
use std::{
    fs::read_to_string,
    path::{Path, PathBuf},
};
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

/// returns a c8y message specifying to set the upload config file operation status to executing.
///
/// example message: '501,c8y_UploadConfigFile'
pub fn get_upload_config_file_executing_message() -> Result<Message, anyhow::Error> {
    let topic = C8yTopic::SmartRestResponse.to_topic()?;
    let smartrest_set_operation_status =
        SmartRestSetOperationToExecuting::new(CumulocitySupportedOperations::C8yUploadConfigFile)
            .to_smartrest()?;
    Ok(Message::new(&topic, smartrest_set_operation_status))
}

/// returns a c8y SmartREST message indicating the success of the upload config file operation.
///
/// example message: '503,c8y_UploadConfigFile,https://{c8y.url}/etc...'
pub fn get_upload_config_file_successful_message(
    binary_upload_event_url: &str,
) -> Result<Message, anyhow::Error> {
    let topic = C8yTopic::SmartRestResponse.to_topic()?;
    let smartrest_set_operation_status =
        SmartRestSetOperationToSuccessful::new(CumulocitySupportedOperations::C8yUploadConfigFile)
            .with_response_parameter(binary_upload_event_url)
            .to_smartrest()?;

    Ok(Message::new(&topic, smartrest_set_operation_status))
}

/// returns a c8y SmartREST message indicating the failure of the upload config file operation.
///
/// example message: '503,c8y_UploadConfigFile,https://{c8y.url}/etc...'
pub fn get_upload_config_file_failure_message(
    failure_reason: String,
) -> Result<Message, anyhow::Error> {
    let topic = C8yTopic::SmartRestResponse.to_topic()?;
    let smartrest_set_operation_status = SmartRestSetOperationToFailed::new(
        CumulocitySupportedOperations::C8yUploadConfigFile,
        failure_reason,
    )
    .to_smartrest()?;

    Ok(Message::new(&topic, smartrest_set_operation_status))
}

async fn handle_config_upload_request(
    config_upload_request: SmartRestConfigUploadRequest,
    mqtt_client: &mut Connection,
    http_client: &mut JwtAuthHttpProxy,
) -> Result<()> {
    // set config upload request to executing
    let msg = get_upload_config_file_executing_message()?;
    let () = mqtt_client.published.send(msg).await?;

    let upload_result = upload_config_file(
        Path::new(config_upload_request.config_type.as_str()),
        http_client,
    )
    .await;
    match upload_result {
        Ok(upload_event_url) => {
            let successful_message = get_upload_config_file_successful_message(&upload_event_url)?;
            let () = mqtt_client.published.send(successful_message).await?;
        }
        Err(err) => {
            let failed_message = get_upload_config_file_failure_message(err.to_string())?;
            let () = mqtt_client.published.send(failed_message).await?;
        }
    }

    Ok(())
}

async fn upload_config_file(
    config_file_path: &Path,
    http_client: &mut JwtAuthHttpProxy,
) -> Result<String> {
    // read the config file contents
    let config_content = read_to_string(config_file_path)?;

    // upload config file
    let upload_event_url = http_client
        .upload_config_file(config_file_path, &config_content)
        .await?;

    Ok(upload_event_url)
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
                    debug!("{}", payload);
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
                    debug!("{}", payload);
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
