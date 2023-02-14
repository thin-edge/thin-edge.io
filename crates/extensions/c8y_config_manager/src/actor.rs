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
use anyhow::Result;
use async_trait::async_trait;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigDownloadRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigUploadRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::smartrest_serializer::TryIntoOperationStatusMessage;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::handle::C8YHttpProxy;
use log::error;
use mqtt_channel::Message;
use mqtt_channel::Topic;
use tedge_actors::fan_in_message_type;
use tedge_actors::futures::channel::mpsc;
use tedge_actors::futures::StreamExt;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageBox;
use tedge_actors::RuntimeRequest;
use tedge_api::health::get_health_status_message;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_timer_ext::SetTimeout;
use tedge_timer_ext::Timeout;
use tedge_utils::paths::PathsError;

pub type OperationTimer = SetTimeout<ChildConfigOperationKey>;
pub type OperationTimeout = Timeout<ChildConfigOperationKey>;

fan_in_message_type!(ConfigInput[MqttMessage, FsWatchEvent, OperationTimeout, RuntimeRequest] : Debug);
fan_in_message_type!(ConfigOutput[MqttMessage, OperationTimer] : Debug);

pub struct ConfigManagerActor {
    config: ConfigManagerConfig,
    config_upload_manager: ConfigUploadManager,
    config_download_manager: ConfigDownloadManager,
}

impl ConfigManagerActor {
    pub fn new(config: ConfigManagerConfig) -> Self {
        let config_upload_manager = ConfigUploadManager::new(config.clone());

        let config_download_manager = ConfigDownloadManager::new(config.clone());

        ConfigManagerActor {
            config,
            config_upload_manager,
            config_download_manager,
        }
    }

    pub async fn process_mqtt_message(
        &mut self,
        message: Message,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), anyhow::Error> {
        if self.config.health_check_topics.accept(&message) {
            let message = get_health_status_message("c8y-configuration-plugin").await;
            message_box.send(message.into()).await?;
            return Ok(());
        } else if self.config.config_snapshot_response_topics.accept(&message) {
            self.handle_child_device_config_operation_response(&message, message_box)
                .await?;
        } else if self.config.config_update_response_topics.accept(&message) {
            self.handle_child_device_config_operation_response(&message, message_box)
                .await?;
        } else if self.config.c8y_request_topics.accept(&message) {
            self.process_smartrest_message(message, message_box).await?;
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
        message: Message,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), anyhow::Error> {
        let payload = message.payload_str()?;
        for smartrest_message in payload.split('\n') {
            let result: Result<(), anyhow::Error> = match smartrest_message
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
                            .handle_config_download_request(config_download_request, message_box)
                            .await
                        {
                            Self::fail_config_operation_in_c8y(
                                ConfigOperation::Update,
                                None,
                                ActiveOperationState::Pending,
                                format!("Failed due to {}", err),
                                message_box,
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
                            .handle_config_upload_request(config_upload_request, message_box)
                            .await
                        {
                            Self::fail_config_operation_in_c8y(
                                ConfigOperation::Snapshot,
                                None,
                                ActiveOperationState::Pending,
                                format!("Failed due to {}", err),
                                message_box,
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
        message: &Message,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), anyhow::Error> {
        match ConfigOperationResponse::try_from(message) {
            Ok(config_response) => {
                let smartrest_responses = match &config_response {
                    ConfigOperationResponse::Update { .. } => {
                        self.config_download_manager
                            .handle_child_device_config_update_response(
                                &config_response,
                                message_box,
                            )
                            .await?
                    }
                    ConfigOperationResponse::Snapshot { .. } => {
                        self.config_upload_manager
                            .handle_child_device_config_snapshot_response(
                                &config_response,
                                message_box,
                            )
                            .await?
                    }
                };

                for smartrest_response in smartrest_responses {
                    message_box.send(smartrest_response.into()).await?
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
                    message_box,
                )
                .await
            }
        }
    }

    pub async fn process_file_watch_events(
        &mut self,
        event: FsWatchEvent,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), anyhow::Error> {
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
                    message_box.send(message.into()).await?;
                } else {
                    // this is a child device
                    let plugin_config = PluginConfig::new(&path);
                    let message = plugin_config.to_supported_config_types_message_for_child(
                        &parent_dir_name.to_string_lossy(),
                    )?;
                    message_box.send(message.into()).await?;
                }
            }
        }

        Ok(())
    }

    pub async fn process_operation_timeout(
        &mut self,
        timeout: OperationTimeout,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), anyhow::Error> {
        match timeout.event.operation_type {
            ConfigOperation::Snapshot => {
                self.config_upload_manager
                    .process_operation_timeout(timeout, message_box)
                    .await
            }
            ConfigOperation::Update => {
                self.config_download_manager
                    .process_operation_timeout(timeout, message_box)
                    .await
            }
        }
    }

    async fn publish_supported_config_types(
        &mut self,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), anyhow::Error> {
        let message = self
            .config
            .plugin_config
            .to_supported_config_types_message()?;
        message_box.send(message.into()).await.unwrap();
        Ok(())
    }

    async fn get_pending_operations_from_cloud(
        &mut self,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), anyhow::Error> {
        // Get pending operations
        let message = Message::new(&C8yTopic::SmartRestResponse.to_topic()?, "500");
        message_box.send(message.into()).await?;
        Ok(())
    }

    pub async fn fail_config_operation_in_c8y(
        config_operation: ConfigOperation,
        child_id: Option<String>,
        op_state: ActiveOperationState,
        failure_reason: String,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), anyhow::Error> {
        // Fail the operation in the cloud by sending EXECUTING and FAILED responses back to back
        let executing_msg;
        let failed_msg;

        if let Some(child_id) = child_id {
            let c8y_child_topic =
                Topic::new_unchecked(&C8yTopic::ChildSmartRestResponse(child_id).to_string());

            match config_operation {
                ConfigOperation::Snapshot => {
                    executing_msg = Message::new(
                        &c8y_child_topic,
                        UploadConfigFileStatusMessage::status_executing()?,
                    );
                    failed_msg = Message::new(
                        &c8y_child_topic,
                        UploadConfigFileStatusMessage::status_failed(failure_reason)?,
                    );
                }
                ConfigOperation::Update => {
                    executing_msg = Message::new(
                        &c8y_child_topic,
                        DownloadConfigFileStatusMessage::status_executing()?,
                    );
                    failed_msg = Message::new(
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
    type MessageBox = ConfigManagerMessageBox;

    fn name(&self) -> &str {
        "ConfigManager"
    }

    async fn run(mut self, mut message_box: Self::MessageBox) -> Result<(), ChannelError> {
        self.publish_supported_config_types(&mut message_box)
            .await?;
        self.get_pending_operations_from_cloud(&mut message_box)
            .await?;

        while let Some(event) = message_box.recv().await {
            match event {
                ConfigInput::MqttMessage(message) => {
                    self.process_mqtt_message(message, &mut message_box).await?;
                }
                ConfigInput::FsWatchEvent(event) => {
                    self.process_file_watch_events(event, &mut message_box)
                        .await?;
                }
                ConfigInput::OperationTimeout(timeout) => {
                    self.process_operation_timeout(timeout, &mut message_box)
                        .await?;
                }
                ConfigInput::RuntimeRequest(RuntimeRequest::Shutdown) => break,
            }
        }
        Ok(())
    }
}

pub struct ConfigManagerMessageBox {
    pub input_receiver: mpsc::Receiver<ConfigInput>,
    pub mqtt_publisher: DynSender<MqttMessage>,
    pub c8y_http_proxy: C8YHttpProxy,
    timer_sender: DynSender<SetTimeout<ChildConfigOperationKey>>,
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
}

impl ConfigManagerMessageBox {
    pub fn new(
        events: mpsc::Receiver<ConfigInput>,
        mqtt_publisher: DynSender<MqttMessage>,
        c8y_http_proxy: C8YHttpProxy,
        timer_sender: DynSender<SetTimeout<ChildConfigOperationKey>>,
        signal_receiver: mpsc::Receiver<RuntimeRequest>,
    ) -> ConfigManagerMessageBox {
        ConfigManagerMessageBox {
            input_receiver: events,
            mqtt_publisher,
            c8y_http_proxy,
            timer_sender,
            signal_receiver,
        }
    }

    pub async fn recv(&mut self) -> Option<ConfigInput> {
        tokio::select! {
            Some(event) = self.input_receiver.next() => {
                self.log_input(&event);
                Some(event)
            }
            Some(runtime_request) = self.signal_receiver.next() => {
                self.log_input(&runtime_request);
                Some(ConfigInput::RuntimeRequest(runtime_request))
            }
            else => None
        }
    }

    pub async fn send(&mut self, message: ConfigOutput) -> Result<(), ChannelError> {
        match message {
            ConfigOutput::MqttMessage(message) => self.mqtt_publisher.send(message).await,
            ConfigOutput::OperationTimer(message) => self.timer_sender.send(message).await,
        }
    }
}

impl MessageBox for ConfigManagerMessageBox {
    type Input = ConfigInput;
    type Output = MqttMessage;

    fn turn_logging_on(&mut self, _on: bool) {
        todo!()
    }

    fn name(&self) -> &str {
        "C8Y-Config-Manager"
    }

    fn logging_is_on(&self) -> bool {
        // FIXME this mailbox recv and send method are not used making logging ineffective.
        false
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum ConfigOperation {
    Snapshot,
    Update,
}

impl TryFrom<&Message> for ConfigOperation {
    type Error = ConfigManagementError;

    fn try_from(message: &Message) -> Result<Self, Self::Error> {
        let operation_name = get_operation_name_from_child_topic(&message.topic.name)?;

        if operation_name == "config_snapshot" {
            Ok(Self::Snapshot)
        } else if operation_name == "config_update" {
            Ok(Self::Update)
        } else {
            Err(ConfigManagementError::InvalidChildDeviceTopic {
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
