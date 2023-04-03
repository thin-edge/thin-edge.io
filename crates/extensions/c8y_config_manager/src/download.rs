use crate::child_device::ChildConfigOperationKey;
use crate::child_device::DEFAULT_OPERATION_TIMEOUT;
use crate::plugin_config::InvalidConfigTypeError;

use super::actor::ActiveOperationState;
use super::actor::ConfigManagerActor;
use super::actor::ConfigManagerMessageBox;
use super::actor::ConfigOperation;
use super::actor::OperationTimeout;
use super::child_device::try_cleanup_config_file_from_file_transfer_repositoy;
use super::child_device::ConfigOperationMessage;
use super::child_device::ConfigOperationRequest;
use super::child_device::ConfigOperationResponse;
use super::error::ConfigManagementError;
use super::plugin_config::FileEntry;
use super::plugin_config::PluginConfig;
use super::ConfigManagerConfig;
use super::DEFAULT_PLUGIN_CONFIG_FILE_NAME;
use c8y_api::smartrest::error::SmartRestSerializerError;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigDownloadRequest;
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
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use tedge_actors::Sender;
use tedge_api::OperationStatus;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_timer_ext::SetTimeout;
use tedge_utils::file::PermissionEntry;

pub const CONFIG_CHANGE_TOPIC: &str = "tedge/configuration_change";

pub struct ConfigDownloadManager {
    config: ConfigManagerConfig,
    active_child_ops: HashMap<ChildConfigOperationKey, ActiveOperationState>,
}

impl ConfigDownloadManager {
    pub fn new(config: ConfigManagerConfig) -> Self {
        let active_child_ops = HashMap::new();
        ConfigDownloadManager {
            config,
            active_child_ops,
        }
    }

    pub async fn handle_config_download_request(
        &mut self,
        smartrest_request: SmartRestConfigDownloadRequest,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), ConfigManagementError> {
        info!(
            "Received c8y_DownloadConfigFile request for config type: {} from device: {}",
            smartrest_request.config_type, smartrest_request.device
        );

        if smartrest_request.device == self.config.device_id {
            self.handle_config_download_request_tedge_device(smartrest_request, message_box)
                .await
        } else {
            self.handle_config_download_request_child_device(smartrest_request, message_box)
                .await
        }
    }

    pub async fn handle_config_download_request_tedge_device(
        &mut self,
        smartrest_request: SmartRestConfigDownloadRequest,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), ConfigManagementError> {
        let executing_message = DownloadConfigFileStatusMessage::executing()?;
        message_box.mqtt_publisher.send(executing_message).await?;

        let target_config_type = smartrest_request.config_type.clone();
        let mut target_file_entry = FileEntry::default();

        let plugin_config = PluginConfig::new(&self.config.plugin_config_path);
        let download_result = match plugin_config.get_file_entry_from_type(&target_config_type) {
            Ok(file_entry) => {
                target_file_entry = file_entry;
                self.download_config_file(
                    smartrest_request.url.as_str(),
                    PathBuf::from(&target_file_entry.path),
                    target_file_entry.file_permissions,
                    message_box,
                )
                .await
            }
            Err(err) => Err(err.into()),
        };

        match download_result {
            Ok(_) => {
                info!("The configuration download for '{target_config_type}' is successful.");

                let successful_message = DownloadConfigFileStatusMessage::successful(None)?;
                message_box.mqtt_publisher.send(successful_message).await?;

                let notification_message = get_file_change_notification_message(
                    &target_file_entry.path,
                    &target_config_type,
                );
                message_box
                    .mqtt_publisher
                    .send(notification_message)
                    .await?;
                Ok(())
            }
            Err(err) => {
                error!("The configuration download for '{target_config_type}' failed.",);

                let failed_message = DownloadConfigFileStatusMessage::failed(err.to_string())?;
                message_box.mqtt_publisher.send(failed_message).await?;
                Err(err)
            }
        }
    }

    async fn download_config_file(
        &mut self,
        download_url: &str,
        file_path: PathBuf,
        file_permissions: PermissionEntry,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), ConfigManagementError> {
        message_box
            .c8y_http_proxy
            .download_file(download_url, file_path, file_permissions)
            .await?;

        Ok(())
    }

    /// Map the c8y_DownloadConfigFile request into a tedge/commands/req/config_update command for the child device.
    /// The config file update is shared with the child device via the file transfer service.
    /// The configuration update is downloaded from Cumulocity and is uploaded to the file transfer service,
    /// so that it can be shared with a child device.
    /// A unique URL path for this config file, from the file transfer service, is shared with the child device in the command.
    /// The child device can use this URL to download the config file update from the file transfer service.
    pub async fn handle_config_download_request_child_device(
        &mut self,
        smartrest_request: SmartRestConfigDownloadRequest,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), ConfigManagementError> {
        let child_id = smartrest_request.device;
        let config_type = smartrest_request.config_type;

        let operation_key = ChildConfigOperationKey {
            child_id: child_id.clone(),
            operation_type: ConfigOperation::Update,
            config_type: config_type.clone(),
        };

        let plugin_config = PluginConfig::new(Path::new(&format!(
            "{}/c8y/{child_id}/{DEFAULT_PLUGIN_CONFIG_FILE_NAME}",
            self.config.config_dir.display(),
        )));

        match plugin_config.get_file_entry_from_type(&config_type) {
            Ok(file_entry) => {
                let config_management = ConfigOperationRequest::Update {
                    child_id: child_id.clone(),
                    file_entry,
                };

                if let Err(err) = self
                    .download_config_file(
                        smartrest_request.url.as_str(),
                        config_management.file_transfer_repository_full_path(
                            self.config.file_transfer_dir.clone(),
                        ),
                        PermissionEntry::new(None, None, None), //no need to change ownership of downloaded file
                        message_box,
                    )
                    .await
                {
                    // Fail the operation in the cloud if the file download itself fails
                    // by sending EXECUTING and FAILED responses back to back

                    let failure_reason = format!(
                        "Downloading the config file update from {} failed with {}",
                        smartrest_request.url, err
                    );
                    ConfigManagerActor::fail_config_operation_in_c8y(
                        ConfigOperation::Update,
                        Some(child_id),
                        ActiveOperationState::Pending,
                        failure_reason,
                        message_box,
                    )
                    .await?;
                } else {
                    let config_update_req_msg = MqttMessage::new(
                        &config_management.operation_request_topic(),
                        config_management
                            .operation_request_payload(&self.config.tedge_http_host)?,
                    );
                    message_box.send(config_update_req_msg.into()).await?;
                    info!("Config update request for config type: {config_type} sent to child device: {child_id}");

                    self.active_child_ops
                        .insert(operation_key.clone(), ActiveOperationState::Pending);

                    // Start the timer for operation timeout
                    message_box
                        .send(SetTimeout::new(DEFAULT_OPERATION_TIMEOUT, operation_key).into())
                        .await?;
                }
            }
            Err(InvalidConfigTypeError { config_type }) => {
                warn!(
                    "Ignoring the config operation request for unknown config type: {config_type}"
                );
            }
        }

        Ok(())
    }

    pub async fn handle_child_device_config_update_response(
        &mut self,
        config_response: &ConfigOperationResponse,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<Vec<MqttMessage>, ConfigManagementError> {
        let c8y_child_topic = Topic::new_unchecked(&config_response.get_child_topic());
        let child_device_payload = config_response.get_payload();
        let child_id = config_response.get_child_id();
        let config_type = config_response.get_config_type();
        let operation_key = ChildConfigOperationKey {
            child_id: child_id.clone(),
            operation_type: ConfigOperation::Update,
            config_type: config_type.clone(),
        };

        info!("Config update response received for type: {config_type} from child: {child_id}");

        let mut mapped_responses = vec![];
        if let Some(operation_status) = child_device_payload.status {
            let current_operation_state = self.active_child_ops.get(&operation_key);
            if current_operation_state != Some(&ActiveOperationState::Executing) {
                let executing_status_payload = DownloadConfigFileStatusMessage::status_executing()?;
                mapped_responses.push(MqttMessage::new(&c8y_child_topic, executing_status_payload));
            }

            match operation_status {
                OperationStatus::Successful => {
                    self.active_child_ops.remove(&operation_key);

                    // Cleanup the downloaded file after the operation completes
                    try_cleanup_config_file_from_file_transfer_repositoy(
                        self.config.file_transfer_dir.clone(),
                        config_response,
                    );
                    let successful_status_payload =
                        DownloadConfigFileStatusMessage::status_successful(None)?;
                    mapped_responses.push(MqttMessage::new(
                        &c8y_child_topic,
                        successful_status_payload,
                    ));
                }
                OperationStatus::Failed => {
                    self.active_child_ops.remove(&operation_key);

                    // Cleanup the downloaded file after the operation completes
                    try_cleanup_config_file_from_file_transfer_repositoy(
                        self.config.file_transfer_dir.clone(),
                        config_response,
                    );
                    if let Some(error_message) = &child_device_payload.reason {
                        let failed_status_payload =
                            DownloadConfigFileStatusMessage::status_failed(error_message.clone())?;
                        mapped_responses
                            .push(MqttMessage::new(&c8y_child_topic, failed_status_payload));
                    } else {
                        let default_error_message =
                            String::from("No fail reason provided by child device.");
                        let failed_status_payload =
                            DownloadConfigFileStatusMessage::status_failed(default_error_message)?;
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
            Err(ConfigManagementError::EmptyOperationStatus(c8y_child_topic))
        }
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
            operation_type: ConfigOperation::Update,
            config_type: config_type.clone(),
        };

        if let Some(operation_state) = self.active_child_ops.remove(&operation_key) {
            ConfigManagerActor::fail_config_operation_in_c8y(
                ConfigOperation::Update,
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

pub fn get_file_change_notification_message(file_path: &str, config_type: &str) -> MqttMessage {
    let notification = json!({ "path": file_path }).to_string();
    let topic = Topic::new(format!("{CONFIG_CHANGE_TOPIC}/{config_type}").as_str())
        .unwrap_or_else(|_err| {
            warn!("The type cannot be used as a part of the topic name. Using {CONFIG_CHANGE_TOPIC} instead.");
            Topic::new_unchecked(CONFIG_CHANGE_TOPIC)
        });
    MqttMessage::new(&topic, notification)
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
