use super::download::ConfigDownloadManager;
use super::plugin_config::PluginConfig;
use super::upload::ConfigUploadManager;
use super::ConfigManagerConfig;
use super::DEFAULT_PLUGIN_CONFIG_FILE_NAME;
use crate::c8y_http_proxy::handle::C8YHttpProxy;
use crate::file_system_ext::FsWatchEvent;
use anyhow::Result;
use async_trait::async_trait;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigDownloadRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigUploadRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::Message;
use tedge_actors::fan_in_message_type;
use tedge_actors::mpsc;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageBox;
use tedge_actors::StreamExt;
use tedge_api::health::get_health_status_message;
use tedge_mqtt_ext::MqttMessage;
use tedge_utils::paths::PathsError;
use tracing::error;

fan_in_message_type!(ConfigInput[MqttMessage, FsWatchEvent] : Debug);

pub struct ConfigManagerActor {
    config: ConfigManagerConfig,
    config_upload_manager: ConfigUploadManager,
    config_download_manager: ConfigDownloadManager,
}

impl ConfigManagerActor {
    pub async fn new(config: ConfigManagerConfig) -> Self {
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
            message_box.send(message).await?;
            return Ok(());
        } else if self.config.c8y_request_topics.accept(&message) {
            let payload = message.payload_str()?;
            for smartrest_message in payload.split('\n') {
                let result = match smartrest_message.split(',').next().unwrap_or_default() {
                    "524" => {
                        let maybe_config_download_request =
                            SmartRestConfigDownloadRequest::from_smartrest(smartrest_message);
                        if let Ok(config_download_request) = maybe_config_download_request {
                            self.config_download_manager
                                .handle_config_download_request(
                                    config_download_request,
                                    message_box,
                                )
                                .await
                                .unwrap();
                            Ok(())
                        } else {
                            error!(
                                "Incorrect Download SmartREST payload: {}",
                                smartrest_message
                            );
                            Ok(())
                        }
                    }
                    "526" => {
                        // retrieve config file upload smartrest request from payload
                        let maybe_config_upload_request =
                            SmartRestConfigUploadRequest::from_smartrest(smartrest_message);

                        if let Ok(config_upload_request) = maybe_config_upload_request {
                            // handle the config file upload request
                            self.config_upload_manager
                                .handle_config_upload_request(config_upload_request, message_box)
                                .await
                        } else {
                            error!("Incorrect Upload SmartREST payload: {}", smartrest_message);
                            Ok(())
                        }
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
        } else {
            error!(
                "Received unexpected message on topic: {}",
                message.topic.name
            );
        }
        Ok(())
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
                    message_box.send(message).await?;
                } else {
                    // this is a child device
                    let plugin_config = PluginConfig::new(&path);
                    let message = plugin_config.to_supported_config_types_message_for_child(
                        &parent_dir_name.to_string_lossy(),
                    )?;
                    message_box.send(message).await?;
                }
            }
        }

        Ok(())
    }

    async fn publish_supported_config_types(
        &mut self,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), anyhow::Error> {
        let message = self
            .config
            .plugin_config
            .to_supported_config_types_message()
            .unwrap();
        message_box.send(message).await.unwrap();
        Ok(())
    }

    async fn get_pending_operations_from_cloud(
        &mut self,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<(), anyhow::Error> {
        // Get pending operations
        let message = Message::new(&C8yTopic::SmartRestResponse.to_topic()?, "500");
        message_box.send(message).await?;
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
            .await
            .unwrap();
        self.get_pending_operations_from_cloud(&mut message_box)
            .await
            .unwrap();

        while let Some(event) = message_box.recv().await {
            match event {
                ConfigInput::MqttMessage(message) => {
                    self.process_mqtt_message(message, &mut message_box)
                        .await
                        .unwrap();
                }
                ConfigInput::FsWatchEvent(event) => {
                    self.process_file_watch_events(event, &mut message_box)
                        .await
                        .unwrap();
                }
            }
        }
        Ok(())
    }
}

pub struct ConfigManagerMessageBox {
    pub events: mpsc::Receiver<ConfigInput>,
    pub mqtt_publisher: DynSender<MqttMessage>,
    pub c8y_http_proxy: C8YHttpProxy,
}

impl ConfigManagerMessageBox {
    pub fn new(
        events: mpsc::Receiver<ConfigInput>,
        mqtt_publisher: DynSender<MqttMessage>,
        c8y_http_proxy: C8YHttpProxy,
    ) -> ConfigManagerMessageBox {
        ConfigManagerMessageBox {
            events,
            mqtt_publisher,
            c8y_http_proxy,
        }
    }

    async fn recv(&mut self) -> Option<ConfigInput> {
        tokio::select! {
            Some(message) = self.events.next() => {
                match message {
                    ConfigInput::MqttMessage(message) => {
                        Some(ConfigInput::MqttMessage(message))
                    },
                    ConfigInput::FsWatchEvent(message) => {
                        Some(ConfigInput::FsWatchEvent(message))
                    }
                }
            },
            else => None,
        }
    }

    async fn send(&mut self, message: MqttMessage) -> Result<(), ChannelError> {
        self.mqtt_publisher.send(message).await
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
