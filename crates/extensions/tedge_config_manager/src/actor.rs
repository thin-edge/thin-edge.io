use async_trait::async_trait;
use camino::Utf8Path;
use log::debug;
use log::error;
use log::info;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::LoggingReceiver;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_api::messages::CommandStatus;
use tedge_api::messages::ConfigSnapshotCmdPayload;
use tedge_api::messages::ConfigUpdateCmdPayload;
use tedge_api::Jsonify;
use tedge_downloader_ext::DownloadRequest;
use tedge_downloader_ext::DownloadResult;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_uploader_ext::UploadRequest;
use tedge_uploader_ext::UploadResult;

use super::config::PluginConfig;
use super::error::ConfigManagementError;
use super::ConfigManagerConfig;
use super::DEFAULT_PLUGIN_CONFIG_FILE_NAME;

type MqttTopic = String;

pub type ConfigDownloadRequest = (MqttTopic, DownloadRequest);
pub type ConfigDownloadResult = (MqttTopic, DownloadResult);

pub type ConfigUploadRequest = (MqttTopic, UploadRequest);
pub type ConfigUploadResult = (MqttTopic, UploadResult);

fan_in_message_type!(ConfigInput[MqttMessage, FsWatchEvent, ConfigDownloadResult, ConfigUploadResult] : Debug);
fan_in_message_type!(ConfigOutput[MqttMessage, ConfigDownloadRequest, ConfigUploadRequest]: Debug);

pub struct ConfigManagerActor {
    config: ConfigManagerConfig,
    plugin_config: PluginConfig,
    pending_operations: HashMap<String, ConfigOperation>,
    input_receiver: LoggingReceiver<ConfigInput>,
    mqtt_publisher: LoggingSender<MqttMessage>,
    download_sender: DynSender<ConfigDownloadRequest>,
    upload_sender: DynSender<ConfigUploadRequest>,
}

#[async_trait]
impl Actor for ConfigManagerActor {
    fn name(&self) -> &str {
        "ConfigManager"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        self.reload_supported_config_types().await?;

        while let Some(event) = self.input_receiver.recv().await {
            let result = match event {
                ConfigInput::MqttMessage(message) => self.process_mqtt_message(message).await,
                ConfigInput::FsWatchEvent(event) => self.process_file_watch_events(event).await,
                ConfigInput::ConfigDownloadResult((topic, result)) => {
                    self.process_downloaded_config(&topic, result).await
                }
                ConfigInput::ConfigUploadResult((topic, result)) => {
                    self.process_uploaded_config(&topic, result).await
                }
            };

            if let Err(err) = result {
                error!("Error processing event: {err:?}");
            }
        }

        Ok(())
    }
}

impl ConfigManagerActor {
    pub fn new(
        config: ConfigManagerConfig,
        plugin_config: PluginConfig,
        input_receiver: LoggingReceiver<ConfigInput>,
        mqtt_publisher: LoggingSender<MqttMessage>,
        download_sender: DynSender<ConfigDownloadRequest>,
        upload_sender: DynSender<ConfigUploadRequest>,
    ) -> Self {
        ConfigManagerActor {
            config,
            plugin_config,
            pending_operations: HashMap::new(),
            input_receiver,
            mqtt_publisher,
            download_sender,
            upload_sender,
        }
    }

    async fn process_mqtt_message(&mut self, message: MqttMessage) -> Result<(), ChannelError> {
        match ConfigOperation::request_from_message(&self.config, &message) {
            Ok(Some(ConfigOperation::Snapshot(request))) => match request.status {
                CommandStatus::Init => {
                    info!("Config Snapshot received: {request:?}");
                    self.start_executing_config_request(
                        &message.topic,
                        ConfigOperation::Snapshot(request),
                    )
                    .await?;
                }
                CommandStatus::Executing => {
                    debug!("Executing log request: {request:?}");
                    self.handle_config_snapshot_request(&message.topic, request)
                        .await?;
                }
                CommandStatus::Successful | CommandStatus::Failed { .. } => {}
            },
            Ok(Some(ConfigOperation::Update(request))) => match request.status {
                CommandStatus::Init => {
                    info!("Config Snapshot received: {request:?}");
                    self.start_executing_config_request(
                        &message.topic,
                        ConfigOperation::Update(request),
                    )
                    .await?;
                }
                CommandStatus::Executing => {
                    debug!("Executing log request: {request:?}");
                    self.handle_config_update_request(&message.topic, request)
                        .await?;
                }
                CommandStatus::Successful | CommandStatus::Failed { .. } => {}
            },
            Ok(None) => {}
            Err(ConfigManagementError::InvalidTopicError) => {
                error!(
                    "Received unexpected message on topic: {}",
                    message.topic.name
                );
            }
            Err(err) => {
                error!("Incorrect log request payload: {}", err);
            }
        }
        Ok(())
    }

    async fn start_executing_config_request(
        &mut self,
        topic: &Topic,
        mut operation: ConfigOperation,
    ) -> Result<(), ChannelError> {
        match operation {
            ConfigOperation::Snapshot(ref mut request) => {
                request.executing();
            }
            ConfigOperation::Update(ref mut request) => {
                request.executing();
            }
        }
        self.publish_command_status(topic, &operation).await
    }

    async fn handle_config_snapshot_request(
        &mut self,
        topic: &Topic,
        mut request: ConfigSnapshotCmdPayload,
    ) -> Result<(), ChannelError> {
        match self.execute_config_snapshot_request(topic, &request).await {
            Ok(_) => {
                self.pending_operations
                    .insert(topic.name.clone(), ConfigOperation::Snapshot(request));
            }
            Err(error) => {
                let error_message = format!("Handling of operation failed with: {}", error);
                request.failed(&error_message);
                error!("{}", error_message);
                self.publish_command_status(topic, &ConfigOperation::Snapshot(request))
                    .await?;
            }
        }
        Ok(())
    }

    async fn execute_config_snapshot_request(
        &mut self,
        topic: &Topic,
        request: &ConfigSnapshotCmdPayload,
    ) -> Result<(), ConfigManagementError> {
        let file_entry = self
            .plugin_config
            .get_file_entry_from_type(&request.config_type)?;

        let upload_request =
            UploadRequest::new(&request.tedge_url, Utf8Path::new(&file_entry.path));

        info!(
            "Awaiting upload of config type: {} to url: {}",
            request.config_type, request.tedge_url
        );

        self.upload_sender
            .send((topic.name.clone(), upload_request))
            .await?;

        Ok(())
    }

    async fn process_uploaded_config(
        &mut self,
        topic: &str,
        result: UploadResult,
    ) -> Result<(), ChannelError> {
        if let Some(ConfigOperation::Snapshot(mut request)) = self.pending_operations.remove(topic)
        {
            let topic = Topic::new_unchecked(topic);
            match result {
                Ok(response) => {
                    request.successful(response.file_path.as_str());
                    info!(
                        "Config Snapshot request processed for config type: {}.",
                        request.config_type
                    );
                    self.publish_command_status(&topic, &ConfigOperation::Snapshot(request))
                        .await?;
                }
                Err(err) => {
                    let error_message = format!("Handling of operation failed with: {}", err);
                    request.failed(&error_message);
                    error!("{}", error_message);
                    self.publish_command_status(&topic, &ConfigOperation::Snapshot(request))
                        .await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_config_update_request(
        &mut self,
        topic: &Topic,
        mut request: ConfigUpdateCmdPayload,
    ) -> Result<(), ChannelError> {
        match self.execute_config_update_request(topic, &request).await {
            Ok(_) => {
                self.pending_operations
                    .insert(topic.name.clone(), ConfigOperation::Update(request));
            }
            Err(error) => {
                let error_message = format!("Handling of operation failed with: {}", error);
                request.failed(&error_message);
                error!("{}", error_message);
                self.publish_command_status(topic, &ConfigOperation::Update(request))
                    .await?;
            }
        }
        Ok(())
    }

    async fn execute_config_update_request(
        &mut self,
        topic: &Topic,
        request: &ConfigUpdateCmdPayload,
    ) -> Result<(), ConfigManagementError> {
        let file_entry = self
            .plugin_config
            .get_file_entry_from_type(&request.config_type)?;

        let download_request =
            DownloadRequest::new(&request.tedge_url, Path::new(&file_entry.path))
                .with_permission(file_entry.file_permissions);

        info!(
            "Awaiting download for config type: {} from url: {}",
            request.config_type, request.tedge_url
        );

        self.download_sender
            .send((topic.name.clone(), download_request))
            .await?;

        Ok(())
    }

    async fn process_downloaded_config(
        &mut self,
        topic: &str,
        result: DownloadResult,
    ) -> Result<(), ChannelError> {
        if let Some(ConfigOperation::Update(mut request)) = self.pending_operations.remove(topic) {
            let topic = Topic::new_unchecked(topic);
            match result {
                Ok(response) => {
                    request.successful(response.file_path.as_path().to_str().unwrap());
                    info!(
                        "Config Update request processed for config type: {}.",
                        request.config_type
                    );
                    self.publish_command_status(&topic, &ConfigOperation::Update(request))
                        .await?;
                }
                Err(err) => {
                    let error_message = format!("Handling of operation failed with: {}", err);
                    request.failed(&error_message);
                    error!("{}", error_message);
                    self.publish_command_status(&topic, &ConfigOperation::Update(request))
                        .await?;
                }
            }
        }
        Ok(())
    }

    async fn process_file_watch_events(&mut self, event: FsWatchEvent) -> Result<(), ChannelError> {
        let path = match event {
            FsWatchEvent::Modified(path) => path,
            FsWatchEvent::FileDeleted(path) => path,
            FsWatchEvent::FileCreated(path) => path,
            FsWatchEvent::DirectoryDeleted(_) => return Ok(()),
            FsWatchEvent::DirectoryCreated(_) => return Ok(()),
        };

        match path.file_name() {
            Some(path) if path.eq(DEFAULT_PLUGIN_CONFIG_FILE_NAME) => {
                self.reload_supported_config_types().await?;
                Ok(())
            }
            Some(_) => Ok(()),
            None => {
                error!(
                    "Path for {} does not exist",
                    DEFAULT_PLUGIN_CONFIG_FILE_NAME
                );
                Ok(())
            }
        }
    }

    async fn reload_supported_config_types(&mut self) -> Result<(), ChannelError> {
        self.plugin_config = PluginConfig::new(self.config.plugin_config_path.as_path());
        self.publish_supported_config_types().await
    }

    /// updates the config types
    async fn publish_supported_config_types(&mut self) -> Result<(), ChannelError> {
        let mut config_types = self.plugin_config.get_all_file_types();
        config_types.sort();
        let payload = json!({ "types": config_types }).to_string();
        for topic in self.config.config_reload_topics.patterns.iter() {
            let message =
                MqttMessage::new(&Topic::new_unchecked(topic), payload.clone()).with_retain();
            self.mqtt_publisher.send(message).await?;
        }
        Ok(())
    }

    async fn publish_command_status(
        &mut self,
        topic: &Topic,
        operation: &ConfigOperation,
    ) -> Result<(), ChannelError> {
        match operation.request_into_message(topic) {
            Ok(message) => self.mqtt_publisher.send(message).await?,
            Err(err) => error!("Fail to build a message {:?}: {err}", operation),
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ConfigOperation {
    Snapshot(ConfigSnapshotCmdPayload),
    Update(ConfigUpdateCmdPayload),
}

impl ConfigOperation {
    fn request_from_message(
        config: &ConfigManagerConfig,
        message: &MqttMessage,
    ) -> Result<Option<Self>, ConfigManagementError> {
        if message.payload_bytes().is_empty() {
            Ok(None)
        } else if config.config_snapshot_topic.accept(message) {
            Ok(Some(ConfigOperation::Snapshot(
                ConfigSnapshotCmdPayload::from_json(message.payload_str()?)?,
            )))
        } else if config.config_update_topic.accept(message) {
            Ok(Some(ConfigOperation::Update(
                ConfigUpdateCmdPayload::from_json(message.payload_str()?)?,
            )))
        } else {
            Err(ConfigManagementError::InvalidTopicError)
        }
    }

    fn request_into_message(&self, topic: &Topic) -> Result<MqttMessage, ConfigManagementError> {
        match self {
            ConfigOperation::Snapshot(request) => {
                Ok(MqttMessage::new(topic, request.to_json()).with_retain())
            }
            ConfigOperation::Update(request) => {
                Ok(MqttMessage::new(topic, request.to_json()).with_retain())
            }
        }
    }
}
