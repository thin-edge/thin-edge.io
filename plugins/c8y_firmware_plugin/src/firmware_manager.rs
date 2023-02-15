use crate::error::FirmwareManagementError;
use crate::message::get_child_id_from_child_topic;
use crate::message::FirmwareOperationRequest;
use crate::message::FirmwareOperationResponse;
use c8y_api::http_proxy::C8YHttpProxy;
use c8y_api::smartrest::error::SmartRestSerializerError;
use c8y_api::smartrest::message::collect_smartrest_messages;
use c8y_api::smartrest::message::get_smartrest_template_id;
use c8y_api::smartrest::smartrest_deserializer::SmartRestFirmwareRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRest;
use c8y_api::smartrest::smartrest_serializer::SmartRestSerializer;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToExecuting;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToFailed;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToSuccessful;
use c8y_api::smartrest::smartrest_serializer::TryIntoOperationStatusMessage;
use c8y_api::smartrest::topic::C8yTopic;
use mqtt_channel::Connection;
use mqtt_channel::Message;
use mqtt_channel::MqttError;
use mqtt_channel::SinkExt;
use mqtt_channel::StreamExt;
use mqtt_channel::Topic;
use mqtt_channel::TopicFilter;
use nanoid::nanoid;
use serde::Deserialize;
use serde::Serialize;
use sha256::digest;
use sha256::try_digest;
use std::fs;
use std::os::unix::fs as unix_fs;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tedge_api::health::health_check_topics;
use tedge_api::health::health_status_down_message;
use tedge_api::health::send_health_status;
use tedge_api::OperationStatus;
use tedge_utils::file::create_file_with_user_group;
use tedge_utils::file::get_filename;
use tedge_utils::file::get_gid_by_name;
use tedge_utils::file::get_metadata;
use tedge_utils::file::get_uid_by_name;
use tedge_utils::file::overwrite_file;
use tedge_utils::timers::Timers;
use tokio::sync::Mutex;
use tracing::error;
use tracing::info;
use tracing::warn;

pub const PLUGIN_SERVICE_NAME: &str = "c8y-firmware-plugin";
const FIRMWARE_UPDATE_RESPONSE_TOPICS: &str = "tedge/+/commands/res/firmware_update";

#[cfg(not(test))]
use tedge_config::DEFAULT_FILE_TRANSFER_ROOT_PATH;
#[cfg(not(test))]
pub const FILE_TRANSFER_ROOT_PATH: &str = DEFAULT_FILE_TRANSFER_ROOT_PATH;
#[cfg(test)]
pub const FILE_TRANSFER_ROOT_PATH: &str = "/tmp/file-transfer";
#[cfg(not(test))]
pub const FILE_CACHE_DIR_PATH: &str = "/var/tedge/cache";
#[cfg(test)]
pub const FILE_CACHE_DIR_PATH: &str = "/tmp/cache";
#[cfg(not(test))]
pub const FIRMWARE_OPERATION_DIR_PATH: &str = "/var/tedge/firmware";
#[cfg(test)]
pub const FIRMWARE_OPERATION_DIR_PATH: &str = "/tmp/firmware";

pub struct FirmwareManager {
    mqtt_client: Connection,
    c8y_request_topics: TopicFilter,
    health_check_topics: TopicFilter,
    firmware_update_response_topics: TopicFilter,
    tedge_device_id: String,
    http_client: Arc<Mutex<dyn C8YHttpProxy>>,
    local_http_host: String,
    tmp_dir: PathBuf,
    operation_timer: Timers<(String, String), ActiveOperationState>,
    timeout_sec: Duration,
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

        Ok(FirmwareManager {
            mqtt_client,
            c8y_request_topics,
            health_check_topics,
            firmware_update_response_topics,
            tedge_device_id,
            http_client,
            local_http_host,
            tmp_dir,
            operation_timer: Timers::new(),
            timeout_sec,
        })
    }

    pub async fn init(&mut self) -> Result<(), anyhow::Error> {
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
                Some(((child_id, op_id), op_state)) = self.operation_timer.next_timed_out_entry() => {
                    let failure_reason = format!("Child device {child_id} did not respond within the timeout interval of {}sec. Operation ID={op_id}",
                        self.timeout_sec.as_secs());
                    info!(failure_reason);
                    self.fail_pending_operation_in_cloud(&child_id, Some(&op_id), op_state,failure_reason).await?;
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
            self.handle_firmware_update_smartrest_request(&message)
                .await?
        } else {
            error!(
                "Received unexpected message on topic: {}",
                message.topic.name
            );
        }
        Ok(())
    }

    async fn handle_firmware_update_smartrest_request(
        &mut self,
        message: &Message,
    ) -> Result<(), anyhow::Error> {
        for smartrest_message in collect_smartrest_messages(message.payload_str()?) {
            let result = match get_smartrest_template_id(smartrest_message.as_str()).as_str() {
                "515" => {
                    match SmartRestFirmwareRequest::from_smartrest(smartrest_message.as_str()) {
                        Ok(firmware_request) => {
                            self.handle_firmware_download_request(firmware_request)
                                .await
                        }
                        Err(_) => {
                            error!("Incorrect c8y_Firmware SmartREST payload: {smartrest_message}");
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
        Ok(())
    }

    pub async fn handle_firmware_download_request(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
    ) -> Result<(), anyhow::Error> {
        info!(
            "Handling c8y_Firmware operation: device={}, name={}, version={}, url={}",
            smartrest_request.device,
            smartrest_request.name,
            smartrest_request.version,
            smartrest_request.url,
        );

        if smartrest_request.device == self.tedge_device_id {
            warn!("c8y-firmware-plugin does not support firmware operation for the main tedge device. \
            Please define a custom operation handler for the c8y_Firmware operation.");
            Ok(())
        } else {
            let child_id = smartrest_request.device.clone();
            match self
                .handle_firmware_download_request_child_device(smartrest_request)
                .await
            {
                Ok(_) => Ok(()),
                Err(err) => {
                    self.fail_pending_operation_in_cloud(
                        child_id,
                        None,
                        ActiveOperationState::Pending,
                        format!("{err}"),
                    )
                    .await
                    .unwrap_or_else(|_| {
                        error!("Failed to publish the operation update status message.")
                    });
                    Err(err)
                }
            }
        }
    }

    pub async fn handle_firmware_download_request_child_device(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
    ) -> Result<(), anyhow::Error> {
        let child_id = smartrest_request.device.as_str();
        let firmware_name = smartrest_request.name.as_str();
        let firmware_version = smartrest_request.version.as_str();
        let firmware_url = smartrest_request.url.as_str();
        let file_cache_key = digest(firmware_url);

        // <tedge-cache-root>/<file_cache_key>
        let cache_dest = PathBuf::from(FILE_CACHE_DIR_PATH).join(&file_cache_key);
        let cache_dest_str = format!("{FILE_CACHE_DIR_PATH}/{file_cache_key}");

        // <tedge-file-transfer-root>/<child-id>/firmware_update/<file_cache_key>
        let transfer_dest = PathBuf::from(FILE_TRANSFER_ROOT_PATH)
            .join(child_id)
            .join("firmware_update")
            .join(&file_cache_key);

        let file_transfer_url = format!(
            "http://{}/tedge/file-transfer/{child_id}/firmware_update/{file_cache_key}",
            &self.local_http_host
        );

        // If dir already exists, these calls do nothing.
        create_parent_dirs(&transfer_dest)?;
        create_parent_dirs(&cache_dest)?;

        if cache_dest.is_file() {
            info!("Hit the file cache={cache_dest_str}. File download is skipped.");
        } else {
            match self
                .http_client
                .lock()
                .await
                .download_file(firmware_url, &file_cache_key, &self.tmp_dir)
                .await
            {
                Ok(tmp_file_path) => {
                    info!("Successfully downloaded from {firmware_url}");
                    move_file(&tmp_file_path, &cache_dest)?;
                }
                Err(err) => {
                    error!("Failed to download from {firmware_url}");
                    return Err(err.into());
                }
            }
        }

        // Create a symlink if it doesn't exist yet.
        if !transfer_dest.is_file() {
            unix_fs::symlink(&cache_dest, &transfer_dest)?;
        }

        // Create an operation status file
        let operation_id = nanoid!();
        let file_sha256 = try_digest(transfer_dest.as_path())?;

        let operation_entry = FirmwareOperationEntry {
            operation_id: operation_id.clone(),
            child_id: child_id.to_string(),
            name: firmware_name.to_string(),
            version: firmware_version.to_string(),
            server_url: firmware_url.to_string(),
            file_transfer_url: file_transfer_url.clone(),
            sha256: file_sha256.to_string(),
            attempt: 1,
        };
        operation_entry.create_file()?;

        let request = FirmwareOperationRequest::new(operation_entry);
        // TODO! Change it to "let message = request.try_into()";
        let message = Message::new(&request.get_topic(), request.get_json_payload()?);
        self.mqtt_client.published.send(message).await?;
        info!("Firmware update request is sent. operation_id={operation_id}, child={child_id}");

        self.operation_timer.start_timer(
            (child_id.to_string(), operation_id),
            ActiveOperationState::Pending,
            self.timeout_sec,
        );

        Ok(())
    }

    pub fn handle_child_device_firmware_update_response(
        &mut self,
        response: &FirmwareOperationResponse,
    ) -> Result<Vec<Message>, FirmwareManagementError> {
        let c8y_child_topic = Topic::new_unchecked(&response.get_child_topic());
        let child_device_payload = response.get_payload();
        let child_id = response.get_child_id();
        let operation_id = child_device_payload.operation_id.as_str();
        let status = child_device_payload.status;
        info!("Firmware update response received. Details: id={operation_id}, child={child_id}, status={status:?}");

        let status_file_path = PersistentStore::get_file_path(operation_id);
        if let Err(err) = PersistentStore::has_expected_permission(operation_id) {
            warn!("{err}");
            return Ok(vec![]);
        }

        let operation_entry = FirmwareOperationEntry::read_from_file(status_file_path.as_path())?;

        let current_operation_state = self
            .operation_timer
            .current_value(&(child_id.to_string(), operation_id.to_string()));

        let mut mapped_responses = vec![];
        match current_operation_state {
            Some(&ActiveOperationState::Executing) => {}
            Some(&ActiveOperationState::Pending) => {
                let executing_status_payload = DownloadFirmwareStatusMessage::status_executing()?;
                mapped_responses.push(Message::new(&c8y_child_topic, executing_status_payload));
            }
            None => {
                info!("Received a response with mismatched info. Ignore this response.");
                return Ok(mapped_responses);
            }
        }

        match status {
            OperationStatus::Successful => {
                self.operation_timer
                    .stop_timer((child_id, operation_id.to_string()));
                fs::remove_file(status_file_path)?;

                let update_firmware_state_message = format!(
                    "115,{},{},{}",
                    operation_entry.name, operation_entry.version, operation_entry.server_url
                );
                mapped_responses.push(Message::new(
                    &c8y_child_topic,
                    update_firmware_state_message,
                ));

                let successful_status_payload =
                    DownloadFirmwareStatusMessage::status_successful(None)?;
                mapped_responses.push(Message::new(&c8y_child_topic, successful_status_payload));
            }
            OperationStatus::Failed => {
                self.operation_timer
                    .stop_timer((child_id, operation_id.to_string()));
                fs::remove_file(status_file_path)?;

                if let Some(error_message) = &child_device_payload.reason {
                    let failed_status_payload =
                        DownloadFirmwareStatusMessage::status_failed(error_message.clone())?;
                    mapped_responses.push(Message::new(&c8y_child_topic, failed_status_payload));
                } else {
                    let default_error_message =
                        String::from("No fail reason provided by child device.");
                    let failed_status_payload =
                        DownloadFirmwareStatusMessage::status_failed(default_error_message)?;
                    mapped_responses.push(Message::new(&c8y_child_topic, failed_status_payload));
                }
            }
            OperationStatus::Executing => {
                self.operation_timer.start_timer(
                    (child_id, operation_id.to_string()),
                    ActiveOperationState::Executing,
                    self.timeout_sec,
                );
            }
        }

        Ok(mapped_responses)
    }

    pub async fn handle_child_device_firmware_operation_response(
        &mut self,
        message: &Message,
    ) -> Result<(), anyhow::Error> {
        match FirmwareOperationResponse::try_from(message) {
            Ok(response) => {
                let smartrest_responses =
                    self.handle_child_device_firmware_update_response(&response)?;

                for smartrest_response in smartrest_responses {
                    self.mqtt_client.published.send(smartrest_response).await?
                }

                Ok(())
            }
            Err(err) => {
                // TODO: Why we need to send failure message to c8y in this case? Shouldn't we just ignore this response?
                let child_id = get_child_id_from_child_topic(&message.topic.name)?;

                self.fail_pending_operation_in_cloud(
                    child_id,
                    None,
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

                self.operation_timer.start_timer(
                    (operation_entry.child_id, operation_entry.operation_id),
                    ActiveOperationState::Pending,
                    self.timeout_sec,
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

    pub async fn fail_pending_operation_in_cloud(
        &mut self,
        child_id: impl ToString,
        op_id: Option<&str>,
        op_state: ActiveOperationState,
        failure_reason: impl ToString,
    ) -> Result<(), anyhow::Error> {
        if let Some(operation_id) = op_id {
            let status_file_path = PersistentStore::get_file_path(operation_id);
            if status_file_path.exists() {
                fs::remove_file(status_file_path)?;
            }
        }

        let c8y_child_topic = Topic::new_unchecked(
            &C8yTopic::ChildSmartRestResponse(child_id.to_string()).to_string(),
        );

        let executing_msg = Message::new(
            &c8y_child_topic,
            DownloadFirmwareStatusMessage::status_executing()?,
        );
        let failed_msg = Message::new(
            &c8y_child_topic,
            DownloadFirmwareStatusMessage::status_failed(failure_reason.to_string())?,
        );

        if op_state == ActiveOperationState::Pending {
            self.mqtt_client.published.send(executing_msg).await?;
        }

        self.mqtt_client.published.send(failed_msg).await?;

        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum ActiveOperationState {
    Pending,
    Executing,
}

pub struct PersistentStore;
impl PersistentStore {
    pub fn get_dir_path() -> PathBuf {
        PathBuf::from(FIRMWARE_OPERATION_DIR_PATH)
    }

    pub fn get_file_path(op_id: &str) -> PathBuf {
        PathBuf::from(FIRMWARE_OPERATION_DIR_PATH).join(op_id)
    }

    // TODO: Candidate to move to file.rs
    pub fn has_expected_permission(op_id: &str) -> Result<(), FirmwareManagementError> {
        let path = Self::get_file_path(op_id);

        let metadata = get_metadata(path.as_path())?;
        let file_uid = metadata.uid();
        let file_gid = metadata.gid();
        let tedge_uid = get_uid_by_name("tedge")?;
        let tedge_gid = get_gid_by_name("tedge")?;
        let root_uid = get_uid_by_name("root")?;
        let root_gid = get_gid_by_name("root")?;

        if (file_uid == tedge_uid || file_uid == root_uid)
            && (file_gid == tedge_gid || file_gid == root_gid)
            && format!("{:o}", metadata.permissions().mode()).contains("644")
        {
            Ok(())
        } else {
            Err(FirmwareManagementError::InvalidFilePermission {
                id: op_id.to_string(),
            })
        }
    }
}

#[derive(Debug, Eq, PartialEq, Default, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FirmwareOperationEntry {
    pub operation_id: String,
    pub child_id: String,
    pub name: String,
    pub version: String,
    pub server_url: String,
    pub file_transfer_url: String,
    pub sha256: String,
    pub attempt: usize,
}

impl FirmwareOperationEntry {
    pub fn create_file(&self) -> Result<(), FirmwareManagementError> {
        let path = PersistentStore::get_file_path(&self.operation_id);
        create_parent_dirs(&path)?;
        let content = serde_json::to_string(self)?;
        create_file_with_user_group(path, "tedge", "tedge", 0o644, Some(content.as_str()))
            .map_err(FirmwareManagementError::FromFileError)
    }

    pub fn overwrite_file(&self) -> Result<(), FirmwareManagementError> {
        let path = PersistentStore::get_file_path(&self.operation_id);
        let content = serde_json::to_string(self)?;
        overwrite_file(&path, &content).map_err(FirmwareManagementError::FromFileError)
    }

    pub fn increment_attempt(self) -> Self {
        Self {
            attempt: self.attempt + 1,
            ..self
        }
    }

    pub fn read_from_file(path: &Path) -> Result<Self, FirmwareManagementError> {
        let bytes = fs::read(path)?;
        serde_json::from_slice(&bytes).map_err(FirmwareManagementError::FromSerdeJsonError)
    }
}

pub struct DownloadFirmwareStatusMessage {}

impl TryIntoOperationStatusMessage for DownloadFirmwareStatusMessage {
    fn status_executing() -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToExecuting::new(CumulocitySupportedOperations::C8yFirmware)
            .to_smartrest()
    }

    fn status_successful(
        _parameter: Option<String>,
    ) -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToSuccessful::new(CumulocitySupportedOperations::C8yFirmware)
            .to_smartrest()
    }

    fn status_failed(failure_reason: String) -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToFailed::new(
            CumulocitySupportedOperations::C8yFirmware,
            failure_reason,
        )
        .to_smartrest()
    }
}

// TODO! Move to common crate
pub fn create_parent_dirs(path: &Path) -> Result<(), FirmwareManagementError> {
    if let Some(dest_dir) = path.parent() {
        if !dest_dir.exists() {
            fs::create_dir_all(dest_dir)?;
        }
    }
    Ok(())
}

// TODO! Remove it and use tedge_utils/move_file instead.
fn move_file(src: &Path, dest: &Path) -> Result<(), FirmwareManagementError> {
    fs::copy(src, dest).map_err(|_| FirmwareManagementError::FileCopyFailed {
        src: src.to_path_buf(),
        dest: dest.to_path_buf(),
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tedge_test_utils::fs::TempTedgeDir;

    #[test]
    fn read_entry_from_file() {
        let op_id = "op-id";
        let content = json!({
          "operation_id": op_id,
          "child_id": "child-id",
          "name": "fw-name",
          "version": "fw-version",
          "server_url": "server-url",
          "file_transfer_url": "file-transfer-url",
          "sha256": "abcd1234",
          "attempt": 1
        })
        .to_string();

        let ttd = TempTedgeDir::new();
        ttd.dir("firmware").file(op_id).with_raw_content(&content);
        let file_path = ttd.path().join("firmware").join(op_id);

        let entry = FirmwareOperationEntry::read_from_file(&file_path).unwrap();
        let expected_entry = FirmwareOperationEntry {
            operation_id: "op-id".to_string(),
            child_id: "child-id".to_string(),
            name: "fw-name".to_string(),
            version: "fw-version".to_string(),
            server_url: "server-url".to_string(),
            file_transfer_url: "file-transfer-url".to_string(),
            sha256: "abcd1234".to_string(),
            attempt: 1,
        };
        assert_eq!(entry, expected_entry);
    }
}
