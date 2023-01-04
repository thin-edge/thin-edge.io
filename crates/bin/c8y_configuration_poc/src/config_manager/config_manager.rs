use crate::c8y_http_proxy::handle::C8YHttpProxy;
use crate::file_system_ext::FsWatchEvent;
use crate::mqtt_ext::MqttMessage;
// use super::download::ConfigDownloadManager;
// use super::download::DownloadConfigFileStatusMessage;
use super::plugin_config::PluginConfig;
use super::upload::ConfigUploadManager;
use super::ConfigManagerConfig;
use anyhow::Result;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigDownloadRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigUploadRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::Message;
use mqtt_channel::TopicFilter;
use tedge_actors::DynSender;
use tedge_api::health::get_health_status_message;
use tedge_api::health::health_check_topics;
use tedge_utils::paths::PathsError;
use tracing::error;

pub const DEFAULT_PLUGIN_CONFIG_FILE_NAME: &str = "c8y-configuration-plugin.toml";
pub const DEFAULT_OPERATION_DIR_NAME: &str = "c8y/";
pub const DEFAULT_PLUGIN_CONFIG_TYPE: &str = "c8y-configuration-plugin";

pub struct ConfigManager {
    plugin_config: PluginConfig,
    mqtt_publisher: DynSender<MqttMessage>,
    c8y_request_topics: TopicFilter,
    health_check_topics: TopicFilter,
    config_upload_manager: ConfigUploadManager,
    // config_download_manager: ConfigDownloadManager,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum ActiveOperationState {
    Pending,
    Executing,
}

impl ConfigManager {
    pub async fn new(
        config: ConfigManagerConfig,
        mqtt_publisher: DynSender<MqttMessage>,
        c8y_http_proxy: C8YHttpProxy,
    ) -> Result<Self, anyhow::Error> {
        let config_upload_manager =
            ConfigUploadManager::new(config.clone(), mqtt_publisher.clone(), c8y_http_proxy);

        // let config_download_manager = ConfigDownloadManager::new(
        //     config.clone(),
        //     mqtt_publisher.clone(),
        //     c8y_http_req_sender.clone(),
        //     c8y_http_res_receiver,
        // );

        let c8y_request_topics: TopicFilter = C8yTopic::SmartRestRequest.into();
        let health_check_topics = health_check_topics("c8y-configuration-plugin");

        let mut config_manager = ConfigManager {
            plugin_config: config.plugin_config.clone(),
            mqtt_publisher: mqtt_publisher.clone(),
            c8y_request_topics,
            health_check_topics,
            config_upload_manager,
        };

        // Publish supported configuration types
        config_manager.publish_supported_config_types().await?;

        Ok(config_manager)
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
                            // self.config_download_manager
                            //     .handle_config_download_request(config_download_request)
                            //     .await
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

        match path.file_name() {
            Some(file_name) => {
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
            None => {}
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
