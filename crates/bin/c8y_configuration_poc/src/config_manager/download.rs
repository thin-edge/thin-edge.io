use super::plugin_config::FileEntry;
use super::plugin_config::PluginConfig;
use super::ConfigManagerConfig;
use crate::c8y_http_proxy::handle::C8YHttpProxy;
use c8y_api::smartrest::error::SmartRestSerializerError;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigDownloadRequest;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRest;
use c8y_api::smartrest::smartrest_serializer::SmartRestSerializer;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToExecuting;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToFailed;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToSuccessful;
use c8y_api::smartrest::smartrest_serializer::TryIntoOperationStatusMessage;
use mqtt_channel::Message;
use mqtt_channel::Topic;
use serde_json::json;
use std::path::PathBuf;
use tedge_actors::DynSender;
use tedge_mqtt_ext::MqttMessage;
use tedge_utils::file::PermissionEntry;
use tracing::error;
use tracing::info;
use tracing::warn;

pub const CONFIG_CHANGE_TOPIC: &str = "tedge/configuration_change";

pub struct ConfigDownloadManager {
    config: ConfigManagerConfig,
    mqtt_publisher: DynSender<MqttMessage>,
    c8y_http_proxy: C8YHttpProxy,
}

impl ConfigDownloadManager {
    pub fn new(
        config: ConfigManagerConfig,
        mqtt_publisher: DynSender<MqttMessage>,
        c8y_http_proxy: C8YHttpProxy,
    ) -> Self {
        ConfigDownloadManager {
            config,
            mqtt_publisher,
            c8y_http_proxy,
        }
    }

    pub async fn handle_config_download_request(
        &mut self,
        smartrest_request: SmartRestConfigDownloadRequest,
    ) -> Result<(), anyhow::Error> {
        info!(
            "Received c8y_DownloadConfigFile request for config type: {} from device: {}",
            smartrest_request.config_type, smartrest_request.device
        );

        self.handle_config_download_request_tedge_device(smartrest_request)
            .await
    }

    pub async fn handle_config_download_request_tedge_device(
        &mut self,
        smartrest_request: SmartRestConfigDownloadRequest,
    ) -> Result<(), anyhow::Error> {
        let executing_message = DownloadConfigFileStatusMessage::executing()?;
        self.mqtt_publisher.send(executing_message).await?;

        let target_config_type = smartrest_request.config_type.clone();
        let mut target_file_entry = FileEntry::default();

        let plugin_config = PluginConfig::new(&self.config.plugin_config_path);
        let download_result = {
            match plugin_config.get_file_entry_from_type(&target_config_type) {
                Ok(file_entry) => {
                    target_file_entry = file_entry;
                    self.download_config_file(
                        smartrest_request.url.as_str(),
                        PathBuf::from(&target_file_entry.path),
                        target_file_entry.file_permissions,
                    )
                    .await
                }
                Err(err) => Err(err.into()),
            }
        };

        match download_result {
            Ok(_) => {
                info!("The configuration download for '{target_config_type}' is successful.");

                let successful_message = DownloadConfigFileStatusMessage::successful(None)?;
                self.mqtt_publisher.send(successful_message).await?;

                let notification_message = get_file_change_notification_message(
                    &target_file_entry.path,
                    &target_config_type,
                );
                self.mqtt_publisher.send(notification_message).await?;
                Ok(())
            }
            Err(err) => {
                error!("The configuration download for '{target_config_type}' failed.",);

                let failed_message = DownloadConfigFileStatusMessage::failed(err.to_string())?;
                self.mqtt_publisher.send(failed_message).await?;
                Err(err)
            }
        }
    }

    async fn download_config_file(
        &mut self,
        download_url: &str,
        file_path: PathBuf,
        file_permissions: PermissionEntry,
    ) -> Result<(), anyhow::Error> {
        self.c8y_http_proxy
            .download_file(download_url, file_path, file_permissions)
            .await
            .unwrap();

        Ok(())
    }
}

pub fn get_file_change_notification_message(file_path: &str, config_type: &str) -> Message {
    let notification = json!({ "path": file_path }).to_string();
    let topic = Topic::new(format!("{CONFIG_CHANGE_TOPIC}/{config_type}").as_str())
        .unwrap_or_else(|_err| {
            warn!("The type cannot be used as a part of the topic name. Using {CONFIG_CHANGE_TOPIC} instead.");
            Topic::new_unchecked(CONFIG_CHANGE_TOPIC)
        });
    Message::new(&topic, notification)
}

pub struct DownloadConfigFileStatusMessage {}

impl TryIntoOperationStatusMessage for DownloadConfigFileStatusMessage {
    fn status_executing() -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToExecuting::new(CumulocitySupportedOperations::C8yDownloadConfigFile)
            .to_smartrest()
    }

    fn status_successful(
        _parameter: Option<String>,
    ) -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToSuccessful::new(CumulocitySupportedOperations::C8yDownloadConfigFile)
            .to_smartrest()
    }

    fn status_failed(failure_reason: String) -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToFailed::new(
            CumulocitySupportedOperations::C8yDownloadConfigFile,
            failure_reason,
        )
        .to_smartrest()
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test() {}
}
