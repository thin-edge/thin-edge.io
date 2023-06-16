use super::child_device::get_child_id_from_child_topic;
use super::child_device::get_operation_name_from_child_topic;
use super::child_device::ChildConfigOperationKey;
use super::child_device::ConfigOperationResponse;
use super::download::ConfigDownloadManager;
use super::download::DownloadConfigFileStatusMessage;
use super::error::ConfigManagementError;
use super::plugin_config::PluginConfig;
use super::upload::ConfigUploadManager;
use super::upload::UploadConfigFileStatusMessage;
use super::ConfigManagerConfig;
use super::DEFAULT_PLUGIN_CONFIG_FILE_NAME;
use crate::child_device::InvalidChildDeviceTopicError;
use async_trait::async_trait;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigDownloadRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigUploadRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::smartrest_serializer::TryIntoOperationStatusMessage;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::handle::C8YHttpProxy;
use log::error;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::LoggingReceiver;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::Sender;
use tedge_actors::WrappedInput;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_timer_ext::SetTimeout;
use tedge_timer_ext::Timeout;
use tedge_utils::paths::PathsError;

pub type OperationTimer = SetTimeout<ChildConfigOperationKey>;
pub type OperationTimeout = Timeout<ChildConfigOperationKey>;

fan_in_message_type!(ConfigInput[MqttMessage, FsWatchEvent, OperationTimeout] : Debug);
fan_in_message_type!(ConfigOutput[MqttMessage, OperationTimer] : Debug);

pub struct ConfigManagerActor {
    config: ConfigManagerConfig,
    plugin_config: PluginConfig,
    config_upload_manager: ConfigUploadManager,
    config_download_manager: ConfigDownloadManager,
    messages: ConfigManagerMessageBox,
}

impl ConfigManagerActor {
    pub fn new(
        config: ConfigManagerConfig,
        plugin_config: PluginConfig,
        messages: ConfigManagerMessageBox,
    ) -> Self {
        let config_upload_manager = ConfigUploadManager::new(config.clone());

        let config_download_manager = ConfigDownloadManager::new(config.clone());

        ConfigManagerActor {
            config,
            plugin_config,
            config_upload_manager,
            config_download_manager,
            messages,
        }
    }

    pub async fn process_mqtt_message(
        &mut self,
        message: MqttMessage,
    ) -> Result<(), ConfigManagementError> {
        if self.config.c8y_request_topics.accept(&message) {
            self.process_smartrest_message(message).await?;
        } else if self.config.config_snapshot_response_topics.accept(&message) {
            self.handle_child_device_config_operation_response(&message)
                .await?;
        } else if self.config.config_update_response_topics.accept(&message) {
            self.handle_child_device_config_operation_response(&message)
                .await?;
        } else {
            error!(
                "Received unexpected message on topic: {}",
                message.topic.name
            );
        }
        Ok(())
    }

    pub async fn process_smartrest_message(
        &mut self,
        message: MqttMessage,
    ) -> Result<(), ConfigManagementError> {
        let payload = message.payload_str()?;
        for smartrest_message in payload.split('\n') {
            let result: Result<(), ConfigManagementError> = match smartrest_message
                .split(',')
                .next()
                .unwrap_or_default()
            {
                "524" => {
                    let maybe_config_download_request =
                        SmartRestConfigDownloadRequest::from_smartrest(smartrest_message);
                    if let Ok(config_download_request) = maybe_config_download_request {
                        if let Err(err) = self
                            .config_download_manager
                            .handle_config_download_request(
                                config_download_request,
                                &mut self.messages,
                            )
                            .await
                        {
                            Self::fail_config_operation_in_c8y(
                                ConfigOperation::Update,
                                None,
                                ActiveOperationState::Pending,
                                format!("Failed due to {}", err),
                                &mut self.messages,
                            )
                            .await?;
                        }
                    } else {
                        error!(
                            "Incorrect Download SmartREST payload: {}",
                            smartrest_message
                        );
                    }
                    Ok(())
                }
                "526" => {
                    // retrieve config file upload smartrest request from payload
                    let maybe_config_upload_request =
                        SmartRestConfigUploadRequest::from_smartrest(smartrest_message);

                    if let Ok(config_upload_request) = maybe_config_upload_request {
                        // handle the config file upload request
                        if let Err(err) = self
                            .config_upload_manager
                            .handle_config_upload_request(config_upload_request, &mut self.messages)
                            .await
                        {
                            Self::fail_config_operation_in_c8y(
                                ConfigOperation::Snapshot,
                                None,
                                ActiveOperationState::Pending,
                                format!("Failed due to {}", err),
                                &mut self.messages,
                            )
                            .await?;
                        }
                    } else {
                        error!("Incorrect Upload SmartREST payload: {}", smartrest_message);
                    }
                    Ok(())
                }
                _ => {
                    // Ignore operation messages not meant for this plugin
                    Ok(())
                }
            };

            if let Err(err) = result {
                error!("Handling of operation: '{smartrest_message}' failed with {err}");
            }
        }

        Ok(())
    }

    pub async fn handle_child_device_config_operation_response(
        &mut self,
        message: &MqttMessage,
    ) -> Result<(), ConfigManagementError> {
        match ConfigOperationResponse::try_from(message) {
            Ok(config_response) => {
                let smartrest_responses = match &config_response {
                    ConfigOperationResponse::Update { .. } => {
                        self.config_download_manager
                            .handle_child_device_config_update_response(
                                &config_response,
                                &mut self.messages,
                            )
                            .await?
                    }
                    ConfigOperationResponse::Snapshot { .. } => {
                        self.config_upload_manager
                            .handle_child_device_config_snapshot_response(
                                &config_response,
                                &mut self.messages,
                            )
                            .await?
                    }
                };

                for smartrest_response in smartrest_responses {
                    self.messages.send(smartrest_response.into()).await?
                }

                Ok(())
            }
            Err(err) => {
                let config_operation = message.try_into()?;
                let child_id = get_child_id_from_child_topic(&message.topic.name)?;

                Self::fail_config_operation_in_c8y(
                    config_operation,
                    Some(child_id),
                    ActiveOperationState::Pending,
                    err.to_string(),
                    &mut self.messages,
                )
                .await
            }
        }
    }

    pub async fn process_file_watch_events(
        &mut self,
        event: FsWatchEvent,
    ) -> Result<(), ConfigManagementError> {
        let path = match event {
            FsWatchEvent::Modified(path) => path,
            FsWatchEvent::FileDeleted(path) => path,
            FsWatchEvent::FileCreated(path) => path,
            FsWatchEvent::DirectoryDeleted(_) => return Ok(()),
            FsWatchEvent::DirectoryCreated(_) => return Ok(()),
        };

        if let Some(file_name) = path.file_name() {
            // this if check is done to avoid matching on temporary files created by editors
            if file_name.eq(DEFAULT_PLUGIN_CONFIG_FILE_NAME) {
                let parent_dir_name = path.parent().and_then(|dir| dir.file_name()).ok_or(
                    PathsError::ParentDirNotFound {
                        path: path.as_os_str().into(),
                    },
                )?;

                if parent_dir_name.eq("c8y") {
                    let plugin_config = PluginConfig::new(&path);
                    let message = plugin_config.to_supported_config_types_message()?;
                    self.messages.send(message.into()).await?;
                } else {
                    // this is a child device
                    let plugin_config = PluginConfig::new(&path);
                    let message = plugin_config.to_supported_config_types_message_for_child(
                        &parent_dir_name.to_string_lossy(),
                    )?;
                    self.messages.send(message.into()).await?;
                }
            }
        }

        Ok(())
    }

    pub async fn process_operation_timeout(
        &mut self,
        timeout: OperationTimeout,
    ) -> Result<(), ConfigManagementError> {
        match timeout.event.operation_type {
            ConfigOperation::Snapshot => {
                self.config_upload_manager
                    .process_operation_timeout(timeout, &mut self.messages)
                    .await
            }
            ConfigOperation::Update => {
                self.config_download_manager
                    .process_operation_timeout(timeout, &mut self.messages)
                    .await
            }
        }
    }

    async fn publish_supported_config_types(&mut self) -> Result<(), ConfigManagementError> {
        let message = self.plugin_config.to_supported_config_types_message()?;
        self.messages.send(message.into()).await.unwrap();
        Ok(())
    }

    async fn get_pending_operations_from_cloud(&mut self) -> Result<(), ConfigManagementError> {
        // Get pending operations
        let message = MqttMessage::new(&C8yTopic::SmartRestResponse.to_topic()?, "500");
        self.messages.send(message.into()).await?;
        Ok(())
    }

    pub async fn fail_config_operation_in_c8y(
        config_operation: ConfigOperation,
        child_id: Option<String>,
        op_state: ActiveOperationState,
        failure_reason: String,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), ConfigManagementError> {
        // Fail the operation in the cloud by sending EXECUTING and FAILED responses back to back
        let executing_msg;
        let failed_msg;

        if let Some(child_id) = child_id {
            let c8y_child_topic =
                Topic::new_unchecked(&C8yTopic::ChildSmartRestResponse(child_id).to_string());

            match config_operation {
                ConfigOperation::Snapshot => {
                    executing_msg = MqttMessage::new(
                        &c8y_child_topic,
                        UploadConfigFileStatusMessage::status_executing()?,
                    );
                    failed_msg = MqttMessage::new(
                        &c8y_child_topic,
                        UploadConfigFileStatusMessage::status_failed(failure_reason)?,
                    );
                }
                ConfigOperation::Update => {
                    executing_msg = MqttMessage::new(
                        &c8y_child_topic,
                        DownloadConfigFileStatusMessage::status_executing()?,
                    );
                    failed_msg = MqttMessage::new(
                        &c8y_child_topic,
                        DownloadConfigFileStatusMessage::status_failed(failure_reason)?,
                    );
                }
            }
        } else {
            match config_operation {
                ConfigOperation::Snapshot => {
                    executing_msg = UploadConfigFileStatusMessage::executing()?;
                    failed_msg = UploadConfigFileStatusMessage::failed(failure_reason)?;
                }
                ConfigOperation::Update => {
                    executing_msg = DownloadConfigFileStatusMessage::executing()?;
                    failed_msg = UploadConfigFileStatusMessage::failed(failure_reason)?;
                }
            };
        }

        if op_state == ActiveOperationState::Pending {
            message_box.send(executing_msg.into()).await?;
        }
        message_box.send(failed_msg.into()).await?;

        Ok(())
    }
}

#[async_trait]
impl Actor for ConfigManagerActor {
    fn name(&self) -> &str {
        "ConfigManager"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        self.publish_supported_config_types().await?;
        self.get_pending_operations_from_cloud().await?;

        while let Some(event) = self.messages.recv().await {
            let result = match event {
                ConfigInput::MqttMessage(message) => self.process_mqtt_message(message).await,
                ConfigInput::FsWatchEvent(event) => self.process_file_watch_events(event).await,
                ConfigInput::OperationTimeout(timeout) => {
                    self.process_operation_timeout(timeout).await
                }
            };

            if let Err(err) = result {
                error!("Error processing event: {err:?}");
            }
        }

        Ok(())
    }
}

pub struct ConfigManagerMessageBox {
    input_receiver: LoggingReceiver<ConfigInput>,
    pub mqtt_publisher: LoggingSender<MqttMessage>,
    pub c8y_http_proxy: C8YHttpProxy,
    timer_sender: LoggingSender<SetTimeout<ChildConfigOperationKey>>,
}

impl ConfigManagerMessageBox {
    pub fn new(
        input_receiver: LoggingReceiver<ConfigInput>,
        mqtt_publisher: LoggingSender<MqttMessage>,
        c8y_http_proxy: C8YHttpProxy,
        timer_sender: LoggingSender<SetTimeout<ChildConfigOperationKey>>,
    ) -> ConfigManagerMessageBox {
        ConfigManagerMessageBox {
            input_receiver,
            mqtt_publisher,
            c8y_http_proxy,
            timer_sender,
        }
    }

    pub async fn send(&mut self, message: ConfigOutput) -> Result<(), ChannelError> {
        match message {
            ConfigOutput::MqttMessage(message) => self.mqtt_publisher.send(message).await,
            ConfigOutput::OperationTimer(message) => self.timer_sender.send(message).await,
        }
    }
}

#[async_trait]
impl MessageReceiver<ConfigInput> for ConfigManagerMessageBox {
    async fn try_recv(&mut self) -> Result<Option<ConfigInput>, RuntimeRequest> {
        self.input_receiver.try_recv().await
    }

    async fn recv_message(&mut self) -> Option<WrappedInput<ConfigInput>> {
        self.input_receiver.recv_message().await
    }

    async fn recv(&mut self) -> Option<ConfigInput> {
        self.input_receiver.recv().await
    }

    async fn recv_signal(&mut self) -> Option<RuntimeRequest> {
        self.input_receiver.recv_signal().await
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum ConfigOperation {
    Snapshot,
    Update,
}

impl TryFrom<&MqttMessage> for ConfigOperation {
    type Error = InvalidChildDeviceTopicError;

    fn try_from(message: &MqttMessage) -> Result<Self, Self::Error> {
        let operation_name = get_operation_name_from_child_topic(&message.topic.name)?;

        if operation_name == "config_snapshot" {
            Ok(Self::Snapshot)
        } else if operation_name == "config_update" {
            Ok(Self::Update)
        } else {
            Err(InvalidChildDeviceTopicError {
                topic: message.topic.name.clone(),
            })
        }
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum ActiveOperationState {
    Pending,
    Executing,
}
