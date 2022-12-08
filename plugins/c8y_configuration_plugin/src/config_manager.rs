use crate::child_device::get_child_id_from_child_topic;
use crate::child_device::ConfigOperationResponse;
use crate::config::PluginConfig;
use crate::download::ConfigDownloadManager;
use crate::download::DownloadConfigFileStatusMessage;
use crate::operation::ConfigOperation;
use crate::topic::ConfigOperationResponseTopic;
use crate::upload::ConfigUploadManager;
use crate::upload::UploadConfigFileStatusMessage;

use anyhow::Result;
use c8y_api::http_proxy::C8YHttpProxy;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigDownloadRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigUploadRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::smartrest_serializer::TryIntoOperationStatusMessage;
use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::Connection;
use mqtt_channel::Message;
use mqtt_channel::MqttError;
use mqtt_channel::SinkExt;
use mqtt_channel::StreamExt;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use tedge_api::health::get_health_status_down_message;
use tokio::sync::Mutex;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tedge_api::health::health_check_topics;
use tedge_api::health::send_health_status;
use tedge_utils::notify::fs_notify_stream;
use tedge_utils::paths::PathsError;

use tedge_utils::notify::FsEvent;
use tedge_utils::notify::NotifyStream;
use tracing::error;
use tracing::info;

pub const DEFAULT_PLUGIN_CONFIG_FILE_NAME: &str = "c8y-configuration-plugin.toml";
pub const DEFAULT_OPERATION_DIR_NAME: &str = "c8y/";
pub const DEFAULT_PLUGIN_CONFIG_TYPE: &str = "c8y-configuration-plugin";
pub const CONFIG_CHANGE_TOPIC: &str = "tedge/configuration_change";
pub const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(10); //TODO: Make this configurable?

pub struct ConfigManager {
    plugin_config: PluginConfig,
    mqtt_client: Connection,
    c8y_request_topics: TopicFilter,
    health_check_topics: TopicFilter,
    config_snapshot_response_topics: TopicFilter,
    config_update_response_topics: TopicFilter,
    fs_notification_stream: NotifyStream,
    config_upload_manager: ConfigUploadManager,
    config_download_manager: ConfigDownloadManager,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum ActiveOperationState {
    Pending,
    Executing,
}

impl ConfigManager {
    pub async fn new(
        tedge_device_id: impl ToString,
        mqtt_port: u16,
        http_client: Arc<Mutex<dyn C8YHttpProxy>>,
        local_http_host: impl ToString,
        tmp_dir: PathBuf,
        config_dir: PathBuf,
    ) -> Result<Self, anyhow::Error> {
        // `config_file_dir` expands to: /etc/tedge/c8y or `config-dir`/c8y
        let config_file_dir = config_dir.join(DEFAULT_OPERATION_DIR_NAME);
        let plugin_config =
            PluginConfig::new(&config_file_dir.join(DEFAULT_PLUGIN_CONFIG_FILE_NAME));

        let mqtt_client = Self::create_mqtt_client(mqtt_port).await?;

        let c8y_request_topics: TopicFilter = C8yTopic::SmartRestRequest.into();
        let health_check_topics = health_check_topics("c8y-configuration-plugin");
        let config_snapshot_response_topics: TopicFilter =
            ConfigOperationResponseTopic::SnapshotResponse.into();
        let config_update_response_topics: TopicFilter =
            ConfigOperationResponseTopic::UpdateResponse.into();

        // we watch `config_file_path` for any change to a file named `DEFAULT_PLUGIN_CONFIG_FILE_NAME`
        let fs_notification_stream = fs_notify_stream(&[(
            &config_file_dir,
            Some(DEFAULT_PLUGIN_CONFIG_FILE_NAME.to_string()),
            &[
                FsEvent::Modified,
                FsEvent::FileDeleted,
                FsEvent::FileCreated,
            ],
        )])?;

        let config_upload_manager = ConfigUploadManager::new(
            tedge_device_id.to_string(),
            mqtt_client.published.clone(),
            http_client.clone(),
            local_http_host.to_string(),
            config_dir.clone(),
        );

        let config_download_manager = ConfigDownloadManager::new(
            tedge_device_id.to_string(),
            mqtt_client.published.clone(),
            http_client.clone(),
            local_http_host.to_string(),
            config_dir.clone(),
            tmp_dir.clone(),
        );

        let mut config_manager = ConfigManager {
            plugin_config,
            mqtt_client,
            c8y_request_topics,
            health_check_topics,
            config_snapshot_response_topics,
            config_update_response_topics,
            fs_notification_stream,
            config_upload_manager,
            config_download_manager,
        };

        // Publish supported configuration types
        config_manager.publish_supported_config_types().await?;

        Ok(config_manager)
    }

    pub async fn run(&mut self) -> Result<(), anyhow::Error> {
        self.get_pending_operations_from_cloud().await?;

        // Now the configuration plugin is done with the initialization and ready for processing the messages
        send_health_status(&mut self.mqtt_client.published, "c8y-configuration-plugin").await;

        loop {
            tokio::select! {
                message = self.mqtt_client.received.next() => {
                    if let Some(message) = message {
                        let topic = message.topic.name.clone();
                        if let Err(err) = self.process_mqtt_message(
                            message,
                        )
                        .await {
                            error!("Processing the message received on {topic} failed with {err}");
                        }
                    } else {
                        // message is None and the connection has been closed
                        return Ok(())
                    }
                }
                Some(((child_id, config_type), op_state)) = self.config_upload_manager.operation_timer.next_timed_out_entry() => {
                    info!("Config snapshot request for config type: {config_type} on child device: {child_id} timed-out");
                    self.fail_pending_config_operation_in_c8y(
                        ConfigOperation::Snapshot,
                        child_id.clone(),
                        op_state,
                        format!("Timeout due to lack of response from child device: {child_id} for config type: {config_type}"),
                    ).await?;
                }
                Some(((child_id, config_type), op_state)) = self.config_download_manager.operation_timer.next_timed_out_entry() => {
                    info!("Config update request for config type: {config_type} on child device: {child_id} timed-out");
                    self.fail_pending_config_operation_in_c8y(
                        ConfigOperation::Update,
                        child_id.clone(),
                        op_state,
                        format!("Timeout due to lack of response from child device: {} for config type: {}", child_id, config_type),
                    ).await?;
                }
                Some((path, mask)) = self.fs_notification_stream.rx.recv() => {
                    match mask {
                        FsEvent::Modified | FsEvent::FileDeleted | FsEvent::FileCreated => {
                            match path.file_name() {
                                Some(file_name) => {
                                    // this if check is done to avoid matching on temporary files created by editors
                                    if file_name.eq(DEFAULT_PLUGIN_CONFIG_FILE_NAME) {
                                        let parent_dir_name = path.parent().and_then(|dir| dir.file_name()).ok_or(PathsError::ParentDirNotFound {path: path.as_os_str().into()})?;

                                        if parent_dir_name.eq("c8y") {
                                            let plugin_config = PluginConfig::new(&path);
                                            let message = plugin_config.to_supported_config_types_message()?;
                                            self.mqtt_client.published.send(message).await?;
                                        } else {
                                            // this is a child device
                                            let plugin_config = PluginConfig::new(&path);
                                            let message = plugin_config.to_supported_config_types_message_for_child(&parent_dir_name.to_string_lossy())?;
                                            self.mqtt_client.published.send(message).await?;
                                        }
                                    }
                                },
                                None => {}
                            }
                        },
                        _ => {
                            // ignore other FsEvent(s)
                        }
                    }
                }
                // (child_id, config_type) = self.unfinished_config_upload_child_op_timers
            }
        }
    }

    async fn create_mqtt_client(mqtt_port: u16) -> Result<mqtt_channel::Connection, anyhow::Error> {
        let mut topic_filter =
            mqtt_channel::TopicFilter::new_unchecked(&C8yTopic::SmartRestRequest.to_string());
        topic_filter.add_all(health_check_topics("c8y-configuration-plugin"));

        topic_filter.add_all(ConfigOperationResponseTopic::SnapshotResponse.into());
        topic_filter.add_all(ConfigOperationResponseTopic::UpdateResponse.into());

        let mqtt_config = mqtt_channel::Config::default()
            .with_session_name("c8y-configuration-plugin")
            .with_port(mqtt_port)
            .with_subscriptions(topic_filter)
            .with_last_will_message(get_health_status_down_message("c8y-configuration-plugin"));

        let mqtt_client = mqtt_channel::Connection::new(&mqtt_config).await?;
        Ok(mqtt_client)
    }

    async fn process_mqtt_message(&mut self, message: Message) -> Result<(), anyhow::Error> {
        if self.health_check_topics.accept(&message) {
            send_health_status(&mut self.mqtt_client.published, "c8y-configuration-plugin").await;
            return Ok(());
        } else if self.config_snapshot_response_topics.accept(&message) {
            self.handle_child_device_config_operation_response(&message)
                .await?;
        } else if self.config_update_response_topics.accept(&message) {
            info!("config update response");
            self.handle_child_device_config_operation_response(&message)
                .await?;
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

    pub async fn handle_child_device_config_operation_response(
        &mut self,
        message: &Message,
    ) -> Result<(), anyhow::Error> {
        match ConfigOperationResponse::try_from(message) {
            Ok(config_response) => {
                let smartrest_responses = match &config_response {
                    ConfigOperationResponse::Update { .. } => self
                        .config_download_manager
                        .handle_child_device_config_update_response(&config_response)?,
                    ConfigOperationResponse::Snapshot { .. } => {
                        self.config_upload_manager
                            .handle_child_device_config_snapshot_response(&config_response)
                            .await?
                    }
                };

                for smartrest_response in smartrest_responses {
                    self.mqtt_client.published.send(smartrest_response).await?
                }

                Ok(())
            }
            Err(err) => {
                let config_operation = message.try_into()?;
                let child_id = get_child_id_from_child_topic(&message.topic.name)?;

                self.fail_pending_config_operation_in_c8y(
                    config_operation,
                    child_id,
                    ActiveOperationState::Pending,
                    err.to_string(),
                )
                .await
            }
        }
    }

    pub async fn fail_pending_config_operation_in_c8y(
        &mut self,
        config_operation: ConfigOperation,
        child_id: String,
        op_state: ActiveOperationState,
        failure_reason: String,
    ) -> Result<(), anyhow::Error> {
        // Fail the operation in the cloud by sending EXECUTING and FAILED responses back to back
        let c8y_child_topic =
            Topic::new_unchecked(&C8yTopic::ChildSmartRestResponse(child_id).to_string());

        let (executing_msg, failed_msg) = match config_operation {
            ConfigOperation::Snapshot => {
                let executing_msg = Message::new(
                    &c8y_child_topic,
                    UploadConfigFileStatusMessage::status_executing()?,
                );
                let failed_msg = Message::new(
                    &c8y_child_topic,
                    UploadConfigFileStatusMessage::status_failed(failure_reason)?,
                );
                (executing_msg, failed_msg)
            }
            ConfigOperation::Update => {
                let executing_msg = Message::new(
                    &c8y_child_topic,
                    DownloadConfigFileStatusMessage::status_executing()?,
                );
                let failed_msg = Message::new(
                    &c8y_child_topic,
                    DownloadConfigFileStatusMessage::status_failed(failure_reason)?,
                );
                (executing_msg, failed_msg)
            }
        };

        if op_state == ActiveOperationState::Pending {
            self.mqtt_client.published.send(executing_msg).await?;
        }

        self.mqtt_client.published.send(failed_msg).await?;

        Ok(())
    }

    async fn publish_supported_config_types(&mut self) -> Result<(), MqttError> {
        let message = self.plugin_config.to_supported_config_types_message()?;
        self.mqtt_client.published.send(message).await?;
        Ok(())
    }

    async fn get_pending_operations_from_cloud(&mut self) -> Result<(), MqttError> {
        // Get pending operations
        let msg = Message::new(&C8yTopic::SmartRestResponse.to_topic()?, "500");
        self.mqtt_client.published.send(msg).await?;
        Ok(())
    }
}
