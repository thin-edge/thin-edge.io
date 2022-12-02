use crate::{
    child_device::{
        try_cleanup_config_file_from_file_transfer_repositoy, ConfigOperationMessage,
        ConfigOperationRequest, ConfigOperationResponse,
    },
    config_manager::{
        ActiveOperationState, DEFAULT_OPERATION_DIR_NAME, DEFAULT_OPERATION_TIMEOUT,
        DEFAULT_PLUGIN_CONFIG_FILE_NAME,
    },
    error::{ChildDeviceConfigManagementError, ConfigManagementError},
    PluginConfig,
};
use anyhow::Result;
use c8y_api::http_proxy::C8YHttpProxy;
use c8y_api::smartrest::error::SmartRestSerializerError;
use c8y_api::smartrest::smartrest_serializer::{SmartRest, TryIntoOperationStatusMessage};
use c8y_api::smartrest::{
    smartrest_deserializer::SmartRestConfigUploadRequest,
    smartrest_serializer::{
        CumulocitySupportedOperations, SmartRestSerializer, SmartRestSetOperationToExecuting,
        SmartRestSetOperationToFailed, SmartRestSetOperationToSuccessful,
    },
};
use tedge_api::OperationStatus;

use mqtt_channel::{Message, SinkExt, Topic, UnboundedSender};
use tedge_utils::{
    file::{create_directory_with_user_group, create_file_with_user_group},
    timers::Timers,
};
use tokio::sync::Mutex;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tracing::{error, info, warn};

pub struct UploadConfigFileStatusMessage {}

impl TryIntoOperationStatusMessage for UploadConfigFileStatusMessage {
    // returns a c8y message specifying to set the upload config file operation status to executing.
    // example message: '501,c8y_UploadConfigFile'
    fn status_executing() -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToExecuting::new(CumulocitySupportedOperations::C8yUploadConfigFile)
            .to_smartrest()
    }

    // returns a c8y SmartREST message indicating the success of the upload config file operation.
    // example message: '503,c8y_UploadConfigFile,https://{c8y.url}/etc...'
    fn status_successful(parameter: Option<String>) -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToSuccessful::new(CumulocitySupportedOperations::C8yUploadConfigFile)
            .with_response_parameter(parameter.unwrap_or_else(|| "".to_string()).as_str())
            .to_smartrest()
    }

    // returns a c8y SmartREST message indicating the failure of the upload config file operation.
    // example message: '502,c8y_UploadConfigFile,"failure reason"'
    fn status_failed(failure_reason: String) -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToFailed::new(
            CumulocitySupportedOperations::C8yUploadConfigFile,
            failure_reason,
        )
        .to_smartrest()
    }
}

pub struct ConfigUploadManager {
    tedge_device_id: String,
    mqtt_publisher: UnboundedSender<Message>,
    http_client: Arc<Mutex<dyn C8YHttpProxy>>,
    local_http_host: String,
    config_dir: PathBuf,
    pub operation_timer: Timers<(String, String), ActiveOperationState>,
}

impl ConfigUploadManager {
    pub fn new(
        tedge_device_id: String,
        mqtt_publisher: UnboundedSender<Message>,
        http_client: Arc<Mutex<dyn C8YHttpProxy>>,
        local_http_host: String,
        config_dir: PathBuf,
    ) -> Self {
        ConfigUploadManager {
            tedge_device_id,
            mqtt_publisher,
            http_client,
            local_http_host,
            config_dir,
            operation_timer: Timers::new(),
        }
    }

    pub async fn handle_config_upload_request(
        &mut self,
        config_upload_request: SmartRestConfigUploadRequest,
    ) -> Result<()> {
        info!(
            "Received c8y_UploadConfigFile request for config type: {} from device: {}",
            config_upload_request.config_type, config_upload_request.device
        );

        if config_upload_request.device == self.tedge_device_id {
            self.handle_config_upload_request_tedge_device(config_upload_request)
                .await
        } else {
            self.handle_config_upload_request_child_device(config_upload_request)
                .await
        }
    }

    pub async fn handle_config_upload_request_tedge_device(
        &mut self,
        config_upload_request: SmartRestConfigUploadRequest,
    ) -> Result<()> {
        // set config upload request to executing
        let msg = UploadConfigFileStatusMessage::executing()?;
        self.mqtt_publisher.send(msg).await?;

        let config_file_path = self
            .config_dir
            .join(DEFAULT_OPERATION_DIR_NAME)
            .join(DEFAULT_PLUGIN_CONFIG_FILE_NAME);
        let plugin_config = PluginConfig::new(&config_file_path);

        let upload_result = {
            match plugin_config.get_file_entry_from_type(&config_upload_request.config_type) {
                Ok(file_entry) => {
                    let config_file_path = file_entry.path;
                    self.upload_config_file(
                        Path::new(config_file_path.as_str()),
                        &config_upload_request.config_type,
                    )
                    .await
                }
                Err(err) => Err(err.into()),
            }
        };

        let target_config_type = &config_upload_request.config_type;

        match upload_result {
            Ok(upload_event_url) => {
                info!("The configuration upload for '{target_config_type}' is successful.");

                let successful_message =
                    UploadConfigFileStatusMessage::successful(Some(upload_event_url))?;
                self.mqtt_publisher.send(successful_message).await?;
            }
            Err(err) => {
                error!("The configuration upload for '{target_config_type}' failed.",);

                let failed_message = UploadConfigFileStatusMessage::failed(err.to_string())?;
                self.mqtt_publisher.send(failed_message).await?;
            }
        }

        Ok(())
    }

    pub async fn upload_config_file(
        &self,
        config_file_path: &Path,
        config_type: &str,
    ) -> Result<String> {
        // upload config file
        let upload_event_url = self
            .http_client
            .lock()
            .await
            .upload_config_file(config_file_path, config_type, None)
            .await?;

        Ok(upload_event_url)
    }

    /// Map the c8y_UploadConfigFile request into a tedge/commands/req/config_snapshot command for the child device.
    /// The child device is expected to upload the config fie snapshot to the file transfer service.
    /// A unique URL path for this config file, from the file transfer service, is shared with the child device in the command.
    /// The child device can use this URL to upload the config file snapshot to the file transfer service.
    pub async fn handle_config_upload_request_child_device(
        &mut self,
        config_upload_request: SmartRestConfigUploadRequest,
    ) -> Result<()> {
        let child_id = config_upload_request.device;
        let config_type = config_upload_request.config_type;

        let plugin_config = PluginConfig::new(Path::new(&format!(
            "{}/c8y/{child_id}/c8y-configuration-plugin.toml",
            self.config_dir.display()
        )));

        match plugin_config.get_file_entry_from_type(&config_type) {
            Ok(file_entry) => {
                let config_management = ConfigOperationRequest::Snapshot {
                    child_id: child_id.clone(),
                    file_entry: file_entry.clone(),
                };

                let msg = Message::new(
                    &config_management.operation_request_topic(),
                    config_management.operation_request_payload(&self.local_http_host)?,
                );
                self.mqtt_publisher.send(msg).await?;
                info!("Config snapshot request for config type: {config_type} sent to child device: {child_id}");

                self.operation_timer.start_timer(
                    (child_id, config_type),
                    ActiveOperationState::Pending,
                    DEFAULT_OPERATION_TIMEOUT,
                );
            }
            Err(ConfigManagementError::InvalidRequestedConfigType { config_type }) => {
                warn!(
                    "Ignoring the config management request for unknown config type: {config_type}"
                );
            }
            Err(err) => return Err(err)?,
        }

        Ok(())
    }

    pub async fn handle_child_device_config_snapshot_response(
        &mut self,
        config_response: &ConfigOperationResponse,
    ) -> Result<Vec<Message>, ChildDeviceConfigManagementError> {
        let payload = config_response.get_payload();
        let c8y_child_topic = Topic::new_unchecked(&config_response.get_child_topic());
        let config_dir = self.config_dir.display();
        let child_id = config_response.get_child_id();
        let config_type = config_response.get_config_type();

        info!("Config snapshot response received for type: {config_type} from child: {child_id}");

        let operation_key = (child_id, config_type);
        let mut mapped_responses = vec![];

        if let Some(operation_status) = payload.status {
            let current_operation_state = self.operation_timer.current_value(&operation_key);
            if current_operation_state != Some(&ActiveOperationState::Executing) {
                let executing_status_payload = UploadConfigFileStatusMessage::status_executing()?;
                mapped_responses.push(Message::new(&c8y_child_topic, executing_status_payload));
            }

            match operation_status {
                OperationStatus::Successful => {
                    self.operation_timer.stop_timer(operation_key);

                    match self
                        .handle_child_device_successful_config_snapshot_response(config_response)
                        .await
                    {
                        Ok(message) => mapped_responses.push(message),
                        Err(err) => {
                            let failed_status_payload =
                                UploadConfigFileStatusMessage::status_failed(err.to_string())?;
                            mapped_responses
                                .push(Message::new(&c8y_child_topic, failed_status_payload));
                        }
                    }
                }
                OperationStatus::Failed => {
                    self.operation_timer.stop_timer(operation_key);

                    if let Some(error_message) = &payload.reason {
                        let failed_status_payload = UploadConfigFileStatusMessage::status_failed(
                            error_message.to_string(),
                        )?;
                        mapped_responses
                            .push(Message::new(&c8y_child_topic, failed_status_payload));
                    } else {
                        let default_error_message =
                            String::from("No fail reason provided by child device.");
                        let failed_status_payload =
                            UploadConfigFileStatusMessage::status_failed(default_error_message)?;
                        mapped_responses
                            .push(Message::new(&c8y_child_topic, failed_status_payload));
                    }
                }
                OperationStatus::Executing => {
                    self.operation_timer.start_timer(
                        operation_key,
                        ActiveOperationState::Executing,
                        DEFAULT_OPERATION_TIMEOUT,
                    );
                }
            }
            Ok(mapped_responses)
        } else {
            if &config_response.get_config_type() == "c8y-configuration-plugin" {
                // create directories
                create_directory_with_user_group(
                    format!("{}/c8y/{}", config_dir, config_response.get_child_id()),
                    "tedge",
                    "tedge",
                    0o755,
                )?;
                create_directory_with_user_group(
                    format!(
                        "{}/operations/c8y/{}",
                        config_dir,
                        config_response.get_child_id()
                    ),
                    "tedge",
                    "tedge",
                    0o755,
                )?;
                create_file_with_user_group(
                    format!(
                        "{}/operations/c8y/{}/c8y_DownloadConfigFile",
                        config_dir,
                        config_response.get_child_id()
                    ),
                    "tedge",
                    "tedge",
                    0o755,
                    None,
                )?;
                create_file_with_user_group(
                    format!(
                        "{}/operations/c8y/{}/c8y_UploadConfigFile",
                        config_dir,
                        config_response.get_child_id()
                    ),
                    "tedge",
                    "tedge",
                    0o755,
                    None,
                )?;
                // copy to /etc/c8y
                let path_from = &format!(
                    "/var/tedge/file-transfer/{}/c8y-configuration-plugin",
                    config_response.get_child_id()
                );
                let path_from = Path::new(path_from);
                let path_to = &format!(
                    "{}/c8y/{}/c8y-configuration-plugin.toml",
                    config_dir,
                    config_response.get_child_id()
                );
                let path_to = Path::new(path_to);
                std::fs::rename(path_from, path_to)?;
            }
            // send 119
            let child_plugin_config = PluginConfig::new(Path::new(&format!(
                "{}/c8y/{}/c8y-configuration-plugin.toml",
                config_dir,
                config_response.get_child_id()
            )));

            // Publish supported configuration types for child devices
            let message = child_plugin_config
                .to_supported_config_types_message_for_child(&config_response.get_child_id())?;
            Ok(vec![message])
        }
    }

    pub async fn handle_child_device_successful_config_snapshot_response(
        &self,
        config_response: &ConfigOperationResponse,
    ) -> Result<Message, anyhow::Error> {
        let c8y_child_topic = Topic::new_unchecked(&config_response.get_child_topic());

        let uploaded_config_file_path = config_response.file_transfer_repository_full_path();

        let c8y_upload_event_url = self
            .http_client
            .lock()
            .await
            .upload_config_file(
                Path::new(&uploaded_config_file_path),
                &config_response.get_config_type(),
                Some(config_response.get_child_id()),
            )
            .await?;

        // Cleanup the child uploaded file after uploading it to cloud
        try_cleanup_config_file_from_file_transfer_repositoy(config_response);

        info!("Marking the c8y_UploadConfigFile operation as successful with the Cumulocity URL for the uploaded file: {c8y_upload_event_url}");
        let successful_status_payload =
            UploadConfigFileStatusMessage::status_successful(Some(c8y_upload_event_url))?;
        let message = Message::new(&c8y_child_topic, successful_status_payload);

        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mqtt_channel::Topic;

    #[test]
    fn get_smartrest_executing() {
        let message = UploadConfigFileStatusMessage::executing().unwrap();
        assert_eq!(message.topic, Topic::new("c8y/s/us").unwrap());
        assert_eq!(message.payload_str().unwrap(), "501,c8y_UploadConfigFile\n");
    }

    #[test]
    fn get_smartrest_successful() {
        let message =
            UploadConfigFileStatusMessage::successful(Some("https://{c8y.url}/etc".to_string()))
                .unwrap();
        assert_eq!(message.topic, Topic::new("c8y/s/us").unwrap());
        assert_eq!(
            message.payload_str().unwrap(),
            "503,c8y_UploadConfigFile,https://{c8y.url}/etc\n"
        );
    }

    #[test]
    fn get_smartrest_failed() {
        let message = UploadConfigFileStatusMessage::failed("failed reason".to_string()).unwrap();
        assert_eq!(message.topic, Topic::new("c8y/s/us").unwrap());
        assert_eq!(
            message.payload_str().unwrap(),
            "502,c8y_UploadConfigFile,\"failed reason\"\n"
        );
    }
}
