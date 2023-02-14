use crate::child_device::get_child_id_from_child_topic;
use crate::child_device::FirmwareOperationRequest;
use crate::child_device::FirmwareOperationResponse;
use crate::common::mark_pending_firmware_operation_failed;
use crate::common::ActiveOperationState;
use crate::common::FirmwareOperationEntry;
use crate::common::PersistentStore;
use crate::download::FirmwareDownloadManager;
use crate::error::FirmwareManagementError;
use c8y_api::http_proxy::C8YHttpProxy;
use c8y_api::smartrest::message::collect_smartrest_messages;
use c8y_api::smartrest::message::get_smartrest_template_id;
use c8y_api::smartrest::smartrest_deserializer::SmartRestFirmwareRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::Connection;
use mqtt_channel::Message;
use mqtt_channel::MqttError;
use mqtt_channel::SinkExt;
use mqtt_channel::StreamExt;
use mqtt_channel::TopicFilter;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tedge_api::health::health_check_topics;
use tedge_api::health::health_status_down_message;
use tedge_api::health::send_health_status;
use tedge_utils::file::get_filename;
use tokio::sync::Mutex;
use tracing::error;
use tracing::info;
use tracing::warn;

pub const PLUGIN_SERVICE_NAME: &str = "c8y-firmware-plugin";
const FIRMWARE_UPDATE_RESPONSE_TOPICS: &str = "tedge/+/commands/res/firmware_update";

pub struct FirmwareManager {
    mqtt_client: Connection,
    c8y_request_topics: TopicFilter,
    health_check_topics: TopicFilter,
    firmware_update_response_topics: TopicFilter,
    firmware_download_manager: FirmwareDownloadManager,
}

impl FirmwareManager {
    pub async fn new(
        tedge_device_id: String,
        mqtt_port: u16,
        http_client: Arc<Mutex<dyn C8YHttpProxy>>,
        local_http_host: String,
        tmp_dir: PathBuf,
        timeout_sec: Duration,
    ) -> Result<Self, anyhow::Error> {
        let mqtt_client = Self::create_mqtt_client(mqtt_port).await?;

        let c8y_request_topics = C8yTopic::SmartRestRequest.into();
        let health_check_topics = health_check_topics(PLUGIN_SERVICE_NAME);
        let firmware_update_response_topics =
            TopicFilter::new_unchecked(FIRMWARE_UPDATE_RESPONSE_TOPICS);

        let firmware_download_manager = FirmwareDownloadManager::new(
            tedge_device_id,
            mqtt_client.published.clone(),
            http_client.clone(),
            local_http_host,
            tmp_dir,
            timeout_sec,
        );

        Ok(FirmwareManager {
            mqtt_client,
            c8y_request_topics,
            health_check_topics,
            firmware_update_response_topics,
            firmware_download_manager,
        })
    }

    pub async fn startup(&mut self) -> Result<(), anyhow::Error> {
        self.resend_operations_to_child_device().await?;
        self.get_pending_operations_from_cloud().await?;
        send_health_status(&mut self.mqtt_client.published, PLUGIN_SERVICE_NAME).await;
        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), anyhow::Error> {
        info!("Ready to serve the firmware request.");
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
                Some(((child_id, op_id), op_state)) = self.firmware_download_manager.operation_timer.next_timed_out_entry() => {
                    let failure_reason = format!("Child device {child_id} did not respond within the timeout interval of {}sec. Operation ID={op_id}",
                        self.firmware_download_manager.timeout_sec.as_secs());
                    info!(failure_reason);
                    mark_pending_firmware_operation_failed(self.mqtt_client.published.clone(), &child_id, op_state,failure_reason).await?;
                }
            }
        }
    }

    async fn process_mqtt_message(&mut self, message: Message) -> Result<(), anyhow::Error> {
        if self.health_check_topics.accept(&message) {
            send_health_status(&mut self.mqtt_client.published, PLUGIN_SERVICE_NAME).await;
            return Ok(());
        } else if self.firmware_update_response_topics.accept(&message) {
            self.handle_child_device_firmware_operation_response(&message)
                .await?
        } else if self.c8y_request_topics.accept(&message) {
            for smartrest_message in collect_smartrest_messages(message.payload_str()?) {
                let result = match get_smartrest_template_id(smartrest_message.as_str()).as_str() {
                    "515" => {
                        match SmartRestFirmwareRequest::from_smartrest(smartrest_message.as_str()) {
                            Ok(firmware_request) => {
                                self.firmware_download_manager
                                    .handle_firmware_download_request(firmware_request)
                                    .await
                            }
                            Err(_) => {
                                error!(
                                    "Incorrect c8y_Firmware SmartREST payload: {smartrest_message}"
                                );
                                Ok(())
                            }
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

    pub async fn handle_child_device_firmware_operation_response(
        &mut self,
        message: &Message,
    ) -> Result<(), anyhow::Error> {
        match FirmwareOperationResponse::try_from(message) {
            Ok(response) => {
                let smartrest_responses = self
                    .firmware_download_manager
                    .handle_child_device_firmware_update_response(&response)?;

                for smartrest_response in smartrest_responses {
                    self.mqtt_client.published.send(smartrest_response).await?
                }

                Ok(())
            }
            Err(err) => {
                let child_id = get_child_id_from_child_topic(&message.topic.name)?;

                mark_pending_firmware_operation_failed(
                    self.mqtt_client.published.clone(),
                    child_id,
                    ActiveOperationState::Pending,
                    err.to_string(),
                )
                .await
            }
        }
    }

    async fn resend_operations_to_child_device(&mut self) -> Result<(), anyhow::Error> {
        let dir_path = PersistentStore::get_dir_path();
        if !dir_path.is_dir() {
            // Do nothing if the persistent store directory does not exist yet.
            return Ok(());
        }

        for entry in fs::read_dir(dir_path)? {
            let file_path = entry?.path();
            let operation_id = get_filename(file_path.clone()).ok_or(
                FirmwareManagementError::PersistentStoreError {
                    path: file_path.clone(),
                },
            )?;

            if file_path.is_file() {
                if let Err(err) = PersistentStore::has_expected_permission(operation_id.as_str()) {
                    warn!("{err}");
                    continue;
                }

                let operation_entry =
                    FirmwareOperationEntry::read_from_file(&file_path)?.increment_attempt();
                operation_entry.overwrite_file()?;

                let request = FirmwareOperationRequest::new(operation_entry.clone());
                let message = Message::new(&request.get_topic(), request.get_json_payload()?);
                self.mqtt_client.published.send(message).await?;
                info!(
                    "Firmware update request is resent. operation_id={}, child={}",
                    operation_entry.operation_id, operation_entry.child_id
                );

                self.firmware_download_manager.operation_timer.start_timer(
                    (operation_entry.child_id, operation_entry.operation_id),
                    ActiveOperationState::Pending,
                    self.firmware_download_manager.timeout_sec,
                );
            }
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

    async fn get_pending_operations_from_cloud(&mut self) -> Result<(), MqttError> {
        let msg = Message::new(&C8yTopic::SmartRestResponse.to_topic()?, "500");
        self.mqtt_client.published.send(msg).await?;
        Ok(())
    }
}
