use crate::child_device::try_cleanup_config_file_from_file_transfer_repositoy;
use crate::child_device::ChildConfigOperationKey;
use crate::child_device::ConfigOperationMessage;
use crate::child_device::DEFAULT_OPERATION_TIMEOUT;
use crate::plugin_config::InvalidConfigTypeError;

use super::actor::ActiveOperationState;
use super::actor::ConfigManagerActor;
use super::actor::ConfigManagerMessageBox;
use super::actor::ConfigOperation;
use super::actor::OperationTimeout;
use super::child_device::ConfigOperationRequest;
use super::child_device::ConfigOperationResponse;
use super::error::ConfigManagementError;
use super::plugin_config::PluginConfig;
use super::ConfigManagerConfig;
use c8y_api::smartrest::error::SmartRestSerializerError;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigUploadRequest;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRest;
use c8y_api::smartrest::smartrest_serializer::SmartRestSerializer;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToExecuting;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToFailed;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToSuccessful;
use c8y_api::smartrest::smartrest_serializer::TryIntoOperationStatusMessage;
use log::error;
use log::info;
use log::warn;
use std::collections::HashMap;
use std::path::Path;
use tedge_actors::Sender;
use tedge_api::OperationStatus;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_timer_ext::SetTimeout;
use tedge_utils::file::create_directory_with_user_group;
use tedge_utils::file::create_file_with_user_group;
use tedge_utils::file::move_file;
use tedge_utils::file::FileError;
use tedge_utils::file::PermissionEntry;

pub struct ConfigUploadManager {
    config: ConfigManagerConfig,
    active_child_ops: HashMap<ChildConfigOperationKey, ActiveOperationState>,
}

impl ConfigUploadManager {
    pub fn new(config: ConfigManagerConfig) -> Self {
        let active_child_ops = HashMap::new();
        ConfigUploadManager {
            config,
            active_child_ops,
        }
    }

    pub async fn handle_config_upload_request(
        &mut self,
        config_upload_request: SmartRestConfigUploadRequest,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), ConfigManagementError> {
        info!(
            "Received c8y_UploadConfigFile request for config type: {} from device: {}",
            config_upload_request.config_type, config_upload_request.device
        );

        if config_upload_request.device == self.config.device_id {
            self.handle_config_upload_request_tedge_device(config_upload_request, message_box)
                .await
        } else {
            self.handle_config_upload_request_child_device(config_upload_request, message_box)
                .await
        }
    }

    pub async fn handle_config_upload_request_tedge_device(
        &mut self,
        config_upload_request: SmartRestConfigUploadRequest,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), ConfigManagementError> {
        // set config upload request to executing
        let msg = UploadConfigFileStatusMessage::executing()?;
        message_box.mqtt_publisher.send(msg).await?;

        let plugin_config = PluginConfig::new(&self.config.plugin_config_path);

        let upload_result =
            match plugin_config.get_file_entry_from_type(&config_upload_request.config_type) {
                Ok(file_entry) => {
                    let config_file_path = file_entry.path;
                    self.upload_config_file(
                        Path::new(config_file_path.as_str()),
                        &config_upload_request.config_type,
                        None,
                        message_box,
                    )
                    .await
                }
                Err(err) => Err(err.into()),
            };

        let target_config_type = &config_upload_request.config_type;

        match upload_result {
            Ok(upload_event_url) => {
                info!("The configuration upload for '{target_config_type}' is successful.");

                let successful_message =
                    UploadConfigFileStatusMessage::successful(Some(upload_event_url))?;
                message_box.mqtt_publisher.send(successful_message).await?;
            }
            Err(err) => {
                error!("The configuration upload for '{target_config_type}' failed.",);

                let failed_message = UploadConfigFileStatusMessage::failed(err.to_string())?;
                message_box.mqtt_publisher.send(failed_message).await?;
            }
        }

        Ok(())
    }

    /// Map the c8y_UploadConfigFile request into a tedge/commands/req/config_snapshot command for the child device.
    /// The child device is expected to upload the config fie snapshot to the file transfer service.
    /// A unique URL path for this config file, from the file transfer service, is shared with the child device in the command.
    /// The child device can use this URL to upload the config file snapshot to the file transfer service.
    pub async fn handle_config_upload_request_child_device(
        &mut self,
        config_upload_request: SmartRestConfigUploadRequest,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), ConfigManagementError> {
        let child_id = config_upload_request.device;
        let config_type = config_upload_request.config_type;
        let operation_key = ChildConfigOperationKey {
            child_id: child_id.clone(),
            operation_type: ConfigOperation::Snapshot,
            config_type: config_type.clone(),
        };

        let plugin_config = PluginConfig::new(Path::new(&format!(
            "{}/c8y/{child_id}/c8y-configuration-plugin.toml",
            self.config.config_dir.display()
        )));

        match plugin_config.get_file_entry_from_type(&config_type) {
            Ok(file_entry) => {
                let config_management = ConfigOperationRequest::Snapshot {
                    child_id: child_id.clone(),
                    file_entry: file_entry.clone(),
                };

                let msg = MqttMessage::new(
                    &config_management.operation_request_topic(),
                    config_management.operation_request_payload(&self.config.tedge_http_host)?,
                );
                message_box.send(msg.into()).await?;
                info!("Config snapshot request for config type: {config_type} sent to child device: {child_id}");

                self.active_child_ops
                    .insert(operation_key.clone(), ActiveOperationState::Pending);

                // Start the timer for operation timeout
                message_box
                    .send(SetTimeout::new(DEFAULT_OPERATION_TIMEOUT, operation_key).into())
                    .await?;
            }
            Err(InvalidConfigTypeError { config_type }) => {
                warn!(
                    "Ignoring the config management request for unknown config type: {config_type}"
                );
            }
        }

        Ok(())
    }

    pub async fn handle_child_device_config_snapshot_response(
        &mut self,
        config_response: &ConfigOperationResponse,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<Vec<MqttMessage>, ConfigManagementError> {
        let payload = config_response.get_payload();
        let c8y_child_topic = Topic::new_unchecked(&config_response.get_child_topic());
        let config_dir = self.config.config_dir.display();
        let child_id = config_response.get_child_id();
        let config_type = config_response.get_config_type();
        let operation_key = ChildConfigOperationKey {
            child_id: child_id.clone(),
            operation_type: ConfigOperation::Snapshot,
            config_type: config_type.clone(),
        };

        info!("Config snapshot response received for type: {config_type} from child: {child_id}");

        let mut mapped_responses = vec![];
        if let Some(operation_status) = payload.status {
            let current_operation_state = self.active_child_ops.get(&operation_key);
            if current_operation_state != Some(&ActiveOperationState::Executing) {
                let executing_status_payload = UploadConfigFileStatusMessage::status_executing()?;
                mapped_responses.push(MqttMessage::new(&c8y_child_topic, executing_status_payload));
            }

            match operation_status {
                OperationStatus::Successful => {
                    self.active_child_ops.remove(&operation_key);

                    match self
                        .handle_child_device_successful_config_snapshot_response(
                            config_response,
                            message_box,
                        )
                        .await
                    {
                        Ok(message) => mapped_responses.push(message),
                        Err(err) => {
                            let failed_status_payload =
                                UploadConfigFileStatusMessage::status_failed(err.to_string())?;
                            mapped_responses
                                .push(MqttMessage::new(&c8y_child_topic, failed_status_payload));
                        }
                    }
                }
                OperationStatus::Failed => {
                    self.active_child_ops.remove(&operation_key);

                    if let Some(error_message) = &payload.reason {
                        let failed_status_payload = UploadConfigFileStatusMessage::status_failed(
                            error_message.to_string(),
                        )?;
                        mapped_responses
                            .push(MqttMessage::new(&c8y_child_topic, failed_status_payload));
                    } else {
                        let default_error_message =
                            String::from("No failure reason provided by child device.");
                        let failed_status_payload =
                            UploadConfigFileStatusMessage::status_failed(default_error_message)?;
                        mapped_responses
                            .push(MqttMessage::new(&c8y_child_topic, failed_status_payload));
                    }
                }
                OperationStatus::Executing => {
                    self.active_child_ops
                        .insert(operation_key.clone(), ActiveOperationState::Executing);

                    // Reset the timer
                    message_box
                        .send(SetTimeout::new(DEFAULT_OPERATION_TIMEOUT, operation_key).into())
                        .await?;
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
                    "{}/{}/c8y-configuration-plugin",
                    self.config.file_transfer_dir.display(),
                    config_response.get_child_id()
                );
                let path_from = Path::new(path_from);
                let path_to = &format!(
                    "{}/c8y/{}/c8y-configuration-plugin.toml",
                    config_dir,
                    config_response.get_child_id()
                );
                let path_to = Path::new(path_to);
                move_file(path_from, path_to, PermissionEntry::default())
                    .await
                    .map_err(FileError::from)?;
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
        &mut self,
        config_response: &ConfigOperationResponse,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<MqttMessage, ConfigManagementError> {
        let c8y_child_topic = Topic::new_unchecked(&config_response.get_child_topic());

        let uploaded_config_file_path = config_response
            .file_transfer_repository_full_path(self.config.file_transfer_dir.clone());

        let c8y_upload_event_url = self
            .upload_config_file(
                Path::new(&uploaded_config_file_path),
                &config_response.get_config_type(),
                Some(config_response.get_child_id()),
                message_box,
            )
            .await?;

        // Cleanup the child uploaded file after uploading it to cloud
        try_cleanup_config_file_from_file_transfer_repositoy(
            self.config.file_transfer_dir.clone(),
            config_response,
        );

        info!("Marking the c8y_UploadConfigFile operation as successful with the Cumulocity URL for the uploaded file: {c8y_upload_event_url}");
        let successful_status_payload =
            UploadConfigFileStatusMessage::status_successful(Some(c8y_upload_event_url))?;
        let message = MqttMessage::new(&c8y_child_topic, successful_status_payload);

        Ok(message)
    }

    pub async fn upload_config_file(
        &mut self,
        config_file_path: &Path,
        config_type: &str,
        child_device_id: Option<String>,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<String, ConfigManagementError> {
        let url = message_box
            .c8y_http_proxy
            .upload_config_file(config_file_path, config_type, child_device_id)
            .await?;
        Ok(url)
    }

    pub async fn process_operation_timeout(
        &mut self,
        timeout: OperationTimeout,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), ConfigManagementError> {
        let child_id = timeout.event.child_id;
        let config_type = timeout.event.config_type;
        let operation_key = ChildConfigOperationKey {
            child_id: child_id.clone(),
            operation_type: ConfigOperation::Snapshot,
            config_type: config_type.clone(),
        };

        if let Some(operation_state) = self.active_child_ops.remove(&operation_key) {
            ConfigManagerActor::fail_config_operation_in_c8y(
                ConfigOperation::Snapshot,
                Some(child_id.clone()),
                operation_state,
                format!("Timeout due to lack of response from child device: {child_id} for config type: {config_type}"),
                message_box,
            ).await
        } else {
            // Ignore the timeout as the operation has already completed.
            Ok(())
        }
    }
}

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
            .with_response_parameter(parameter.unwrap_or_default().as_str())
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
