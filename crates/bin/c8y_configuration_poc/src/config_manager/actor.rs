use super::download::ConfigDownloadManager;
use super::plugin_config::PluginConfig;
use super::upload::ConfigUploadManager;
use super::ConfigManagerConfig;
use super::DEFAULT_PLUGIN_CONFIG_FILE_NAME;
use crate::c8y_http_proxy::handle::C8YHttpProxy;
use crate::c8y_http_proxy::messages::C8YRestRequest;
use crate::c8y_http_proxy::messages::C8YRestResult;
use crate::file_system_ext::FsWatchEvent;
use anyhow::Result;
use async_trait::async_trait;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigDownloadRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigUploadRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::Message;
use mqtt_channel::TopicFilter;
use tedge_actors::fan_in_message_type;
use tedge_actors::mpsc;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageBox;
use tedge_actors::StreamExt;
use tedge_api::health::get_health_status_message;
use tedge_api::health::health_check_topics;
use tedge_mqtt_ext::MqttMessage;
use tedge_utils::paths::PathsError;
use tracing::error;

fan_in_message_type!(ConfigInputAndResponse[MqttMessage, FsWatchEvent, C8YRestResult] : Debug);
fan_in_message_type!(ConfigInput[MqttMessage, FsWatchEvent] : Debug);
fan_in_message_type!(ConfigOutput[MqttMessage, C8YRestRequest] : Debug);

pub struct ConfigManagerActor {
    plugin_config: PluginConfig,
    mqtt_publisher: DynSender<MqttMessage>,
    c8y_request_topics: TopicFilter,
    health_check_topics: TopicFilter,
    config_upload_manager: ConfigUploadManager,
    config_download_manager: ConfigDownloadManager,
}

impl ConfigManagerActor {
    pub async fn new(
        config: ConfigManagerConfig,
        mqtt_publisher: DynSender<MqttMessage>,
        c8y_upload_http_proxy: C8YHttpProxy,
        c8y_download_http_proxy: C8YHttpProxy,
    ) -> Self {
        let config_upload_manager = ConfigUploadManager::new(
            config.clone(),
            mqtt_publisher.clone(),
            c8y_upload_http_proxy,
        );

        let config_download_manager = ConfigDownloadManager::new(
            config.clone(),
            mqtt_publisher.clone(),
            c8y_download_http_proxy,
        );

        let c8y_request_topics: TopicFilter = C8yTopic::SmartRestRequest.into();
        let health_check_topics = health_check_topics("c8y-configuration-plugin");

        ConfigManagerActor {
            plugin_config: config.plugin_config,
            mqtt_publisher: mqtt_publisher.clone(),
            c8y_request_topics,
            health_check_topics,
            config_upload_manager,
            config_download_manager,
        }
    }

    pub async fn process_mqtt_message(&mut self, message: Message) -> Result<(), anyhow::Error> {
        if self.health_check_topics.accept(&message) {
            let message = get_health_status_message("c8y-configuration-plugin").await;
            self.mqtt_publisher.send(message).await?;
            return Ok(());
        } else if self.c8y_request_topics.accept(&message) {
            let payload = message.payload_str()?;
            for smartrest_message in payload.split('\n') {
                let result = match smartrest_message.split(',').next().unwrap_or_default() {
                    "524" => {
                        let maybe_config_download_request =
                            SmartRestConfigDownloadRequest::from_smartrest(smartrest_message);
                        if let Ok(config_download_request) = maybe_config_download_request {
                            self.config_download_manager
                                .handle_config_download_request(config_download_request)
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
                                .handle_config_upload_request(config_upload_request)
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
                    self.mqtt_publisher.send(message).await?;
                } else {
                    // this is a child device
                    let plugin_config = PluginConfig::new(&path);
                    let message = plugin_config.to_supported_config_types_message_for_child(
                        &parent_dir_name.to_string_lossy(),
                    )?;
                    self.mqtt_publisher.send(message).await?;
                }
            }
        }

        Ok(())
    }

    async fn publish_supported_config_types(&mut self) -> Result<(), anyhow::Error> {
        let message = self
            .plugin_config
            .to_supported_config_types_message()
            .unwrap();
        self.mqtt_publisher.send(message).await.unwrap();
        Ok(())
    }

    async fn get_pending_operations_from_cloud(&mut self) -> Result<(), anyhow::Error> {
        // Get pending operations
        let msg = Message::new(&C8yTopic::SmartRestResponse.to_topic()?, "500");
        self.mqtt_publisher.send(msg).await?;
        Ok(())
    }
}

#[async_trait]
impl Actor for ConfigManagerActor {
    type MessageBox = ConfigManagerMessageBox;

    fn name(&self) -> &str {
        "ConfigManager"
    }

    async fn run(mut self, mut messages: Self::MessageBox) -> Result<(), ChannelError> {
        self.publish_supported_config_types().await.unwrap();
        self.get_pending_operations_from_cloud().await.unwrap();

        while let Some(event) = messages.events.next().await {
            match event {
                ConfigInput::MqttMessage(message) => {
                    self.process_mqtt_message(message).await.unwrap();
                }
                ConfigInput::FsWatchEvent(event) => {
                    self.process_file_watch_events(event).await.unwrap();
                }
            }
        }
        Ok(())
    }
}

pub struct ConfigManagerMessageBox {
    pub events: mpsc::Receiver<ConfigInput>,
    pub http_responses: mpsc::Receiver<C8YRestResult>,
    pub http_requests: DynSender<C8YRestRequest>,
    pub mqtt_requests: DynSender<MqttMessage>,
}

impl ConfigManagerMessageBox {
    pub fn new(
        events: mpsc::Receiver<ConfigInput>,
        http_responses: mpsc::Receiver<C8YRestResult>,
        http_con: DynSender<C8YRestRequest>,
        mqtt_con: DynSender<MqttMessage>,
    ) -> ConfigManagerMessageBox {
        ConfigManagerMessageBox {
            events,
            http_responses,
            http_requests: http_con,
            mqtt_requests: mqtt_con,
        }
    }
}

impl MessageBox for ConfigManagerMessageBox {
    type Input = ConfigInputAndResponse;
    type Output = ConfigOutput;

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
