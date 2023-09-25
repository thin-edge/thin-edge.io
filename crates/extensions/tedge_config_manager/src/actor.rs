use async_trait::async_trait;
use http::status::StatusCode;
use log::debug;
use log::error;
use log::info;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::ClientMessageBox;
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
use tedge_http_ext::HttpError;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpRequestBuilder;
use tedge_http_ext::HttpResponseExt;
use tedge_http_ext::HttpResult;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;

use super::config::PluginConfig;
use super::error::ConfigManagementError;
use super::ConfigManagerConfig;
use super::DEFAULT_PLUGIN_CONFIG_FILE_NAME;

type MqttTopic = String;
pub type ConfigDownloadResult = (MqttTopic, DownloadResult);
pub type ConfigDownloadRequest = (MqttTopic, DownloadRequest);

fan_in_message_type!(ConfigInput[MqttMessage, FsWatchEvent, ConfigDownloadResult] : Debug);
fan_in_message_type!(ConfigOutput[MqttMessage, ConfigDownloadRequest]: Debug);

pub struct ConfigManagerActor {
    config: ConfigManagerConfig,
    plugin_config: PluginConfig,
    pending_downloads: HashMap<String, ConfigUpdateCmdPayload>,
    input_receiver: LoggingReceiver<ConfigInput>,
    mqtt_publisher: LoggingSender<MqttMessage>,
    http_proxy: ClientMessageBox<HttpRequest, HttpResult>,
    download_sender: DynSender<ConfigDownloadRequest>,
}

#[async_trait]
impl Actor for ConfigManagerActor {
    fn name(&self) -> &str {
        "ConfigManager"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        self.reload_supported_config_types().await?;

        while let Some(event) = self.input_receiver.recv().await {
            let result = match event {
                ConfigInput::MqttMessage(message) => self.process_mqtt_message(message).await,
                ConfigInput::FsWatchEvent(event) => self.process_file_watch_events(event).await,
                ConfigInput::ConfigDownloadResult((topic, result)) => {
                    self.process_downloaded_config(&topic, result).await
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
        http_proxy: ClientMessageBox<HttpRequest, HttpResult>,
        download_sender: DynSender<ConfigDownloadRequest>,
    ) -> Self {
        ConfigManagerActor {
            config,
            plugin_config,
            pending_downloads: HashMap::new(),
            input_receiver,
            mqtt_publisher,
            http_proxy,
            download_sender,
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
                CommandStatus::Successful | CommandStatus::Failed => {}
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
                CommandStatus::Successful | CommandStatus::Failed => {}
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
        match self.execute_config_snapshot_request(&request).await {
            Ok(path) => {
                request.successful(path);
                info!(
                    "Config snapshot request processed for config type: {}.",
                    request.config_type
                );
                self.publish_command_status(topic, &ConfigOperation::Snapshot(request))
                    .await?;
            }
            Err(error) => {
                let error_message = format!("Handling of operation failed with {}", error);
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
        request: &ConfigSnapshotCmdPayload,
    ) -> Result<String, ConfigManagementError> {
        let file_entry = self
            .plugin_config
            .get_file_entry_from_type(&request.config_type)?;

        let config_content = std::fs::read_to_string(&file_entry.path)?;

        self.upload_config_file(&request.tedge_url, config_content)
            .await?;

        info!(
            "The configuration upload for '{}' is successful.",
            request.config_type
        );

        Ok(file_entry.path)
    }

    async fn upload_config_file(
        &mut self,
        upload_url: &str,
        config_content: String,
    ) -> Result<(), ConfigManagementError> {
        let req_builder = HttpRequestBuilder::put(upload_url)
            .header("Content-Type", "text/plain")
            .body(config_content);

        let request = req_builder.build()?;

        let http_result = match self.http_proxy.await_response(request).await? {
            Ok(response) => match response.status() {
                StatusCode::OK | StatusCode::CREATED => Ok(response),
                code => Err(HttpError::HttpStatusError(code)),
            },
            Err(err) => Err(err),
        };

        let _ = http_result.error_for_status()?;

        Ok(())
    }

    async fn handle_config_update_request(
        &mut self,
        topic: &Topic,
        mut request: ConfigUpdateCmdPayload,
    ) -> Result<(), ChannelError> {
        match self.execute_config_update_request(topic, &request).await {
            Ok(_) => {
                self.pending_downloads.insert(topic.name.clone(), request);
            }
            Err(error) => {
                let error_message = format!("Handling of operation failed with {}", error);
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
        if let Some(mut request) = self.pending_downloads.remove(topic) {
            let topic = Topic::new_unchecked(topic);
            match result {
                Ok(response) => {
                    request.successful(response.file_path.as_path().to_str().unwrap_or_default());
                    info!(
                        "Config update request processed for config type: {}.",
                        request.config_type
                    );
                    self.publish_command_status(&topic, &ConfigOperation::Update(request))
                        .await?;
                }
                Err(err) => {
                    let error_message = format!("Handling of operation failed with {}", err);
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
