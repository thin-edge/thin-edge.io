use crate::download::FirmwareDownloadManager;
use crate::entry::FirmwareEntry;
use crate::error::FirmwareManagementError;
use c8y_api::http_proxy::C8YHttpProxy;
use c8y_api::smartrest::message::collect_smartrest_messages;
use c8y_api::smartrest::smartrest_deserializer::SmartRestFirmwareRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::Connection;
use mqtt_channel::Message;
use mqtt_channel::SinkExt;
use mqtt_channel::StreamExt;
use mqtt_channel::TopicFilter;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tedge_api::health::health_check_topics;
use tedge_api::health::health_status_down_message;
use tedge_api::health::send_health_status;
use tedge_utils::notify::fs_notify_stream;
use tedge_utils::notify::FsEvent;
use tedge_utils::notify::NotifyStream;
use tokio::sync::Mutex;
use tracing::error;
use tracing::info;

pub const DEFAULT_PLUGIN_CONFIG_FILE_NAME: &str = "c8y-firmware-plugin.toml";
pub const DEFAULT_OPERATION_DIR_NAME: &str = "c8y/";
pub const PLUGIN_SERVICE_NAME: &str = "c8y-firmware-plugin";
pub const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(10); //TODO: Make this configurable?

const FIRMWARE_UPDATE_RESPONSE_TOPICS: &str = "tedge/+/commands/res/firmware_update";

pub struct FirmwareManager {
    mqtt_client: Connection,
    c8y_request_topics: TopicFilter,
    health_check_topics: TopicFilter,
    firmware_update_response_topics: TopicFilter,
    firmware_download_manager: FirmwareDownloadManager,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum ActiveOperationState {
    Pending,
    Executing,
}

impl FirmwareManager {
    pub async fn new(
        tedge_device_id: impl ToString,
        mqtt_port: u16,
        http_client: Arc<Mutex<dyn C8YHttpProxy>>,
        local_http_host: impl ToString,
        tmp_dir: PathBuf,
        config_dir: PathBuf,
    ) -> Result<Self, anyhow::Error> {
        let mqtt_client = Self::create_mqtt_client(mqtt_port).await?;

        let firmware_download_manager = FirmwareDownloadManager::new(
            tedge_device_id.to_string(),
            mqtt_client.published.clone(),
            http_client.clone(),
            local_http_host.to_string(),
            config_dir.clone(),
            tmp_dir.clone(),
        );

        let health_check_topics = health_check_topics(PLUGIN_SERVICE_NAME);

        let mut firmware_manager = FirmwareManager {
            mqtt_client,
            c8y_request_topics: C8yTopic::SmartRestRequest.into(),
            health_check_topics,
            firmware_update_response_topics: TopicFilter::new_unchecked(
                FIRMWARE_UPDATE_RESPONSE_TOPICS,
            ),
            firmware_download_manager,
        };

        Ok(firmware_manager)
    }

    pub async fn run(&mut self) -> Result<(), anyhow::Error> {
        // Now the configuration plugin is done with the initialization and ready for processing the messages
        send_health_status(&mut self.mqtt_client.published, PLUGIN_SERVICE_NAME).await;

        info!("Ready to serve the firmware request");

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
            }
        }
    }

    async fn process_mqtt_message(&mut self, message: Message) -> Result<(), anyhow::Error> {
        if self.health_check_topics.accept(&message) {
            send_health_status(&mut self.mqtt_client.published, PLUGIN_SERVICE_NAME).await;
            return Ok(());
        } else if self.c8y_request_topics.accept(&message) {
            for smartrest_message in collect_smartrest_messages(message.payload_str()?) {
                let result = match smartrest_message.split(',').next().unwrap_or_default() {
                    "515" => {
                        if let Ok(firmware_request) =
                            SmartRestFirmwareRequest::from_smartrest(smartrest_message.as_str())
                        {
                            self.firmware_download_manager
                                .handle_firmware_download_request(firmware_request)
                                .await
                        } else {
                            error!("Incorrect SmartREST payload: {smartrest_message}");
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

    async fn create_mqtt_client(mqtt_port: u16) -> Result<Connection, anyhow::Error> {
        let mut topic_filter = TopicFilter::new_unchecked(&C8yTopic::SmartRestRequest.to_string());
        topic_filter.add_all(health_check_topics(PLUGIN_SERVICE_NAME));
        topic_filter.add_all(TopicFilter::new_unchecked(FIRMWARE_UPDATE_RESPONSE_TOPICS));

        let mqtt_config = mqtt_channel::Config::default()
            .with_session_name(PLUGIN_SERVICE_NAME)
            .with_port(mqtt_port)
            .with_subscriptions(topic_filter)
            .with_last_will_message(health_status_down_message(PLUGIN_SERVICE_NAME));

        let mqtt_client = Connection::new(&mqtt_config).await?;
        Ok(mqtt_client)
    }
}
