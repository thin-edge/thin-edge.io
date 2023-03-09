use crate::download::DownloadRequest;
use crate::download::DownloadResponse;
use crate::error::FirmwareManagementError;
use crate::message::get_child_id_from_child_topic;
use crate::message::FirmwareOperationRequest;
use crate::message::FirmwareOperationResponse;
use crate::FirmwareManagementError::DirectoryNotFound;
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
use futures::channel::mpsc::UnboundedReceiver;
use futures::channel::mpsc::UnboundedSender;
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
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tedge_api::health::health_check_topics;
use tedge_api::health::health_status_down_message;
use tedge_api::health::health_status_up_message;
use tedge_api::health::send_health_status;
use tedge_api::OperationStatus;
use tedge_utils::file::create_file_with_mode;
use tedge_utils::file::overwrite_file;
use tedge_utils::timers::Timers;
use tracing::error;
use tracing::info;
use tracing::warn;

// TODO! We should make it configurable by tedge config later.
pub const PERSISTENT_DIR_PATH: &str = "/var/tedge";

pub const CACHE_DIR_NAME: &str = "cache";
pub const FILE_TRANSFER_DIR_NAME: &str = "file-transfer";
pub const PERSISTENT_STORE_DIR_NAME: &str = "firmware";

const PLUGIN_SERVICE_NAME: &str = "c8y-firmware-plugin";
const FIRMWARE_UPDATE_RESPONSE_TOPICS: &str = "tedge/+/commands/res/firmware_update";

pub struct FirmwareManager {
    mqtt_client: Connection,
    c8y_request_topics: TopicFilter,
    health_check_topics: TopicFilter,
    firmware_update_response_topics: TopicFilter,
    tedge_device_id: String,
    download_req_sndr: UnboundedSender<DownloadRequest>,
    download_res_rcvr: UnboundedReceiver<DownloadResponse>,
    local_http_host: String,
    persistent_dir: PathBuf,
    cache_dir: PathBuf,
    operation_timer: Timers<(String, String), ActiveOperationState>,
    timeout_sec: Duration,
    reqs_pending_download: HashMap<String, SmartRestFirmwareRequest>,
}

impl FirmwareManager {
    // TODO: merge some of the function arguments
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        tedge_device_id: String,
        mqtt_host: String,
        mqtt_port: u16,
        download_req_sndr: UnboundedSender<DownloadRequest>,
        download_res_rcvr: UnboundedReceiver<DownloadResponse>,
        local_http_host: String,
        persistent_dir: PathBuf,
        timeout_sec: Duration,
    ) -> Result<Self, FirmwareManagementError> {
        let mqtt_client = Self::create_mqtt_client(mqtt_host, mqtt_port).await?;

        let c8y_request_topics = C8yTopic::SmartRestRequest.into();
        let health_check_topics = health_check_topics(PLUGIN_SERVICE_NAME);
        let firmware_update_response_topics =
            TopicFilter::new_unchecked(FIRMWARE_UPDATE_RESPONSE_TOPICS);

        let cache_dir = persistent_dir.join(CACHE_DIR_NAME);

        Ok(FirmwareManager {
            mqtt_client,
            c8y_request_topics,
            health_check_topics,
            firmware_update_response_topics,
            tedge_device_id,
            download_req_sndr,
            download_res_rcvr,
            local_http_host,
            persistent_dir,
            cache_dir,
            operation_timer: Timers::new(),
            timeout_sec,
            reqs_pending_download: HashMap::new(),
        })
    }

    pub async fn init(&mut self) -> Result<(), FirmwareManagementError> {
        self.resend_operations_to_child_device().await?;
        self.get_pending_operations_from_cloud().await?;
        send_health_status(&mut self.mqtt_client.published, PLUGIN_SERVICE_NAME).await;
        Ok(())
    }

    pub async fn run(mut self) -> Result<(), FirmwareManagementError> {
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
                Some(((child_id, op_id), _)) = self.operation_timer.next_timed_out_entry() => {
                    let failure_reason = format!("Child device {child_id} did not respond within the timeout interval of {}sec. Operation ID={op_id}",
                        self.timeout_sec.as_secs());
                    self.fail_operation_in_cloud(&child_id, Some(&op_id), &failure_reason).await?;
                }
                Some(download_response) = self.download_res_rcvr.next() => {
                    let operation_id = download_response.id;
                    if let Some(req_in_progress) = self.reqs_pending_download.remove(&operation_id) {
                        let child_id = req_in_progress.device.clone();
                        match download_response.result {
                            Ok(downloaded_firmware) => {
                                info!("Firmware successfully downloaded to {downloaded_firmware:?}");
                                if let Err(err) = self
                                    .handle_firmware_update_request_with_downloaded_file(req_in_progress, operation_id.clone(), downloaded_firmware)
                                    .await
                                {
                                    self.fail_operation_in_cloud(&child_id, Some(&operation_id), &err.to_string())
                                        .await?;
                                }
                            }
                            Err(err) => {
                                let firmware_url = req_in_progress.url;
                                let failure_reason = format!("Download from {firmware_url} failed with {err}");
                                self.fail_operation_in_cloud(&child_id, Some(&operation_id), &failure_reason).await?;
                            }
                        }
                    } else {
                        error!("Unexpected: Download completed for unknown operation: {operation_id}");
                    }
               }
            }
        }
    }

    async fn process_mqtt_message(
        &mut self,
        message: Message,
    ) -> Result<(), FirmwareManagementError> {
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
    ) -> Result<(), FirmwareManagementError> {
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

    async fn handle_firmware_download_request(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
    ) -> Result<(), FirmwareManagementError> {
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
            return Ok(());
        }

        let child_id = smartrest_request.device.as_str();

        if let Err(err) = self
            .validate_same_request_in_progress(smartrest_request.clone())
            .await
        {
            return match err {
                FirmwareManagementError::RequestAlreadyAddressed => {
                    warn!("Skip the received c8y_Firmware operation as the same operation is already in progress.");
                    Ok(())
                }
                _ => {
                    self.fail_operation_in_cloud(&child_id, None, &err.to_string())
                        .await?;
                    Err(err)
                }
            };
        }

        let op_id = nanoid!();
        if let Err(err) = self
            .handle_firmware_download_request_child_device(smartrest_request.clone(), op_id.clone())
            .await
        {
            self.fail_operation_in_cloud(&child_id, Some(&op_id), &err.to_string())
                .await?;
        }

        Ok(())
    }

    async fn handle_firmware_download_request_child_device(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
        operation_id: String,
    ) -> Result<(), FirmwareManagementError> {
        let firmware_url = smartrest_request.url.as_str();
        let file_cache_key = digest(firmware_url);

        // <tedge-persistent-root>/cache/<file_cache_key>
        let cache_dest_path = self.get_cache_file_path(&file_cache_key)?;

        if cache_dest_path.is_file() {
            info!(
                "Hit the file cache={}. File download is skipped.",
                cache_dest_path.display()
            );
            self.handle_firmware_update_request_with_downloaded_file(
                smartrest_request,
                operation_id,
                cache_dest_path,
            )
            .await?;
        } else {
            let download_req = DownloadRequest::new(&operation_id, firmware_url, &file_cache_key);

            info!(
                "Awaiting firmware download for op_id: {} from url: {}",
                operation_id, firmware_url
            );
            // Send a request to the DownloadManager to download the file asynchornously
            self.download_req_sndr.send(download_req).await?;
            self.reqs_pending_download
                .insert(operation_id, smartrest_request);
        }

        Ok(())
    }

    async fn handle_firmware_update_request_with_downloaded_file(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
        operation_id: String,
        downloaded_firmware: PathBuf,
    ) -> Result<(), FirmwareManagementError> {
        let child_id = smartrest_request.device.as_str();
        let firmware_url = smartrest_request.url.as_str();
        let file_cache_key = digest(firmware_url);
        // <tedge-persistent-root>/cache/<file_cache_key>
        let cache_dest_path = self.get_cache_file_path(&file_cache_key)?;

        // If the downloaded firmware is not already in the cache, move it there
        if !downloaded_firmware.starts_with(&self.cache_dir) {
            move_file(&downloaded_firmware, &cache_dest_path)?;
        }

        // <tedge-persistent-root>/file-transfer/<child-id>/firmware_update/<file_cache_key>
        let file_transfer_dest_path =
            self.create_file_transfer_symlink(child_id, &file_cache_key, &cache_dest_path)?;
        let file_transfer_url = format!(
            "http://{}/tedge/file-transfer/{child_id}/firmware_update/{file_cache_key}",
            &self.local_http_host
        );
        let file_sha256 = try_digest(file_transfer_dest_path.as_path())?;

        let operation_entry = FirmwareOperationEntry {
            operation_id: operation_id.clone(),
            child_id: child_id.to_string(),
            name: smartrest_request.name.to_string(),
            version: smartrest_request.version.to_string(),
            server_url: firmware_url.to_string(),
            file_transfer_url: file_transfer_url.clone(),
            sha256: file_sha256.to_string(),
            attempt: 1,
        };

        self.create_operation_status_file(operation_entry.clone())?;

        self.send_firmware_update_request(operation_entry).await?;

        self.operation_timer.start_timer(
            (child_id.to_string(), operation_id),
            ActiveOperationState::Pending,
            self.timeout_sec,
        );

        Ok(())
    }

    async fn handle_child_device_firmware_update_response(
        &mut self,
        response: &FirmwareOperationResponse,
    ) -> Result<(), FirmwareManagementError> {
        let c8y_child_topic = Topic::new_unchecked(&response.get_child_topic());
        let child_device_payload = response.get_payload();
        let child_id = response.get_child_id();
        let operation_id = child_device_payload.operation_id.as_str();
        let status = child_device_payload.status;
        info!("Firmware update response received. Details: id={operation_id}, child={child_id}, status={status:?}");

        let current_operation_state = self
            .operation_timer
            .current_value(&(child_id.to_string(), operation_id.to_string()));

        match current_operation_state {
            Some(&ActiveOperationState::Executing) => {}
            Some(&ActiveOperationState::Pending) => {
                let executing_status_payload = DownloadFirmwareStatusMessage::status_executing()?;
                self.mqtt_client
                    .published
                    .send(Message::new(&c8y_child_topic, executing_status_payload))
                    .await?;
            }
            None => {
                info!("Received a response from {child_id} for unknown request {operation_id}.");
                return Ok(());
            }
        }

        let persistent_store = PersistentStore::from(self.persistent_dir.clone());
        let status_file_path = persistent_store.get_file_path(operation_id);
        let operation_entry = FirmwareOperationEntry::read_from_file(status_file_path.as_path())?;

        match status {
            OperationStatus::Successful => {
                self.operation_timer
                    .stop_timer((child_id, operation_id.to_string()));
                fs::remove_file(status_file_path)?;

                let update_firmware_state_message = format!(
                    "115,{},{},{}",
                    operation_entry.name, operation_entry.version, operation_entry.server_url
                );
                self.mqtt_client
                    .published
                    .send(Message::new(
                        &c8y_child_topic,
                        update_firmware_state_message,
                    ))
                    .await?;

                let successful_status_payload =
                    DownloadFirmwareStatusMessage::status_successful(None)?;
                self.mqtt_client
                    .published
                    .send(Message::new(&c8y_child_topic, successful_status_payload))
                    .await?;
            }
            OperationStatus::Failed => {
                self.operation_timer
                    .stop_timer((child_id, operation_id.to_string()));
                fs::remove_file(status_file_path)?;

                let text = "No failure reason provided by child device.".to_string();
                let failed_status_payload = DownloadFirmwareStatusMessage::status_failed(
                    child_device_payload.reason.as_ref().unwrap_or(&text).into(),
                )?;
                self.mqtt_client
                    .published
                    .send(Message::new(&c8y_child_topic, failed_status_payload))
                    .await?;
            }
            OperationStatus::Executing => {
                self.operation_timer.start_timer(
                    (child_id, operation_id.to_string()),
                    ActiveOperationState::Executing,
                    self.timeout_sec,
                );
            }
        }

        Ok(())
    }

    async fn handle_child_device_firmware_operation_response(
        &mut self,
        message: &Message,
    ) -> Result<(), FirmwareManagementError> {
        let child_id = get_child_id_from_child_topic(&message.topic.name)?;

        match FirmwareOperationResponse::try_from(message) {
            Ok(response) => {
                if let Err(err) = self
                    .handle_child_device_firmware_update_response(&response)
                    .await
                {
                    self.fail_operation_in_cloud(
                        child_id,
                        Some(response.get_payload().operation_id.as_str()),
                        &err.to_string(),
                    )
                    .await?;
                }
            }
            Err(err) => {
                // Ignore bad responses. Eventually, timeout will fail an operation.
                error!("Received a firmware update response with invalid payload for child {child_id}. Error: {err}");
            }
        }
        Ok(())
    }

    async fn resend_operations_to_child_device(&mut self) -> Result<(), FirmwareManagementError> {
        let persistent_store = PersistentStore::from(self.persistent_dir.clone());
        let dir_path = persistent_store.clone().get_dir_path();
        if !dir_path.is_dir() {
            // Do nothing if the persistent store directory does not exist yet.
            return Ok(());
        }

        for entry in fs::read_dir(dir_path)? {
            let file_path = entry?.path();
            if file_path.is_file() {
                let operation_entry =
                    FirmwareOperationEntry::read_from_file(&file_path)?.increment_attempt();
                operation_entry.overwrite_file(persistent_store.clone())?;

                let request = FirmwareOperationRequest::from(operation_entry.clone());
                self.mqtt_client.published.send(request.try_into()?).await?;
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

    async fn create_mqtt_client(
        mqtt_host: String,
        mqtt_port: u16,
    ) -> Result<Connection, MqttError> {
        let mut topic_filter = TopicFilter::new_unchecked(&C8yTopic::SmartRestRequest.to_string());
        topic_filter.add_all(health_check_topics(PLUGIN_SERVICE_NAME));
        topic_filter.add_all(TopicFilter::new_unchecked(FIRMWARE_UPDATE_RESPONSE_TOPICS));

        let mqtt_config = mqtt_channel::Config::default()
            .with_session_name(PLUGIN_SERVICE_NAME)
            .with_host(mqtt_host)
            .with_port(mqtt_port)
            .with_subscriptions(topic_filter)
            .with_initial_message(|| health_status_up_message(PLUGIN_SERVICE_NAME))
            .with_last_will_message(health_status_down_message(PLUGIN_SERVICE_NAME));

        let mqtt_client = Connection::new(&mqtt_config).await?;
        Ok(mqtt_client)
    }

    async fn get_pending_operations_from_cloud(&mut self) -> Result<(), MqttError> {
        let msg = Message::new(&C8yTopic::SmartRestResponse.to_topic()?, "500");
        self.mqtt_client.published.send(msg).await?;
        Ok(())
    }

    async fn fail_operation_in_cloud(
        &mut self,
        child_id: impl ToString,
        op_id: Option<&str>,
        failure_reason: &str,
    ) -> Result<(), FirmwareManagementError> {
        error!(failure_reason);
        let op_state = if let Some(operation_id) = op_id {
            let persistent_store: PersistentStore = self.persistent_dir.clone().into();
            let status_file_path = persistent_store.get_file_path(operation_id);
            if status_file_path.exists() {
                fs::remove_file(status_file_path)?;
            }
            self.operation_timer
                .stop_timer((child_id.to_string(), operation_id.to_string()))
                .unwrap_or(ActiveOperationState::Pending)
        } else {
            ActiveOperationState::Pending
        };

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

    // This function can be removed once we start using operation ID from c8y.
    async fn validate_same_request_in_progress(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
    ) -> Result<(), FirmwareManagementError> {
        let persistent_store_dir_path = self.persistent_dir.join(PERSISTENT_STORE_DIR_NAME);
        validate_dir_exists(&persistent_store_dir_path)?;

        for entry in fs::read_dir(persistent_store_dir_path.clone())? {
            match entry {
                Ok(file_path) => match FirmwareOperationEntry::read_from_file(&file_path.path()) {
                    Ok(recorded_entry) => {
                        if recorded_entry.child_id == smartrest_request.device
                            && recorded_entry.name == smartrest_request.name
                            && recorded_entry.version == smartrest_request.version
                            && recorded_entry.server_url == smartrest_request.url
                        {
                            return Err(FirmwareManagementError::RequestAlreadyAddressed);
                        }
                    }
                    Err(err) => {
                        warn!("Error: {err} while reading the contents of persistent store directory {}", persistent_store_dir_path.display());
                        continue;
                    }
                },
                Err(err) => {
                    warn!(
                        "Error: {err} while reading the contents of persistent store directory {}",
                        persistent_store_dir_path.display()
                    );
                    continue;
                }
            }
        }
        Ok(())
    }

    fn get_cache_file_path(
        &self,
        file_cache_key: &str,
    ) -> Result<PathBuf, FirmwareManagementError> {
        validate_dir_exists(&self.cache_dir)?;
        Ok(self.cache_dir.join(file_cache_key))
    }

    fn create_file_transfer_symlink(
        &self,
        child_id: &str,
        file_cache_key: &str,
        original_path: &Path,
    ) -> Result<PathBuf, FirmwareManagementError> {
        let file_transfer_dir_path = self.persistent_dir.join(FILE_TRANSFER_DIR_NAME);
        validate_dir_exists(&file_transfer_dir_path)?;

        let symlink_dir_path = file_transfer_dir_path
            .join(child_id)
            .join("firmware_update");
        let symlink_path = symlink_dir_path.join(file_cache_key);

        if !symlink_path.is_symlink() {
            fs::create_dir_all(symlink_dir_path)?;
            unix_fs::symlink(original_path, &symlink_path)?;
        }
        Ok(symlink_path)
    }

    fn create_operation_status_file(
        &self,
        operation_entry: FirmwareOperationEntry,
    ) -> Result<(), FirmwareManagementError> {
        let persistent_store = PersistentStore::from(self.persistent_dir.clone());
        operation_entry.create_file(persistent_store)?;
        Ok(())
    }

    async fn send_firmware_update_request(
        &mut self,
        operation_entry: FirmwareOperationEntry,
    ) -> Result<(), FirmwareManagementError> {
        let request = FirmwareOperationRequest::from(operation_entry.clone());
        self.mqtt_client.published.send(request.try_into()?).await?;
        info!(
            "Firmware update request is sent. operation_id={}, child={}",
            operation_entry.operation_id, operation_entry.child_id
        );
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum ActiveOperationState {
    Pending,
    Executing,
}

#[derive(Debug, PartialEq, Eq, Clone)]
struct PersistentStore {
    persistent_dir: PathBuf,
}

impl From<PathBuf> for PersistentStore {
    fn from(path: PathBuf) -> Self {
        Self {
            persistent_dir: path,
        }
    }
}

impl PersistentStore {
    fn get_dir_path(&self) -> PathBuf {
        self.persistent_dir.join(PERSISTENT_STORE_DIR_NAME)
    }

    fn get_file_path(&self, op_id: &str) -> PathBuf {
        self.get_dir_path().join(op_id)
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
    fn create_file(
        &self,
        persistent_store: PersistentStore,
    ) -> Result<(), FirmwareManagementError> {
        let path = persistent_store.get_file_path(&self.operation_id);
        let content = serde_json::to_string(self)?;
        create_file_with_mode(path, Some(content.as_str()), 0o644)
            .map_err(FirmwareManagementError::FromFileError)
    }

    fn overwrite_file(
        &self,
        persistent_store: PersistentStore,
    ) -> Result<(), FirmwareManagementError> {
        let path = persistent_store.get_file_path(&self.operation_id);
        let content = serde_json::to_string(self)?;
        overwrite_file(&path, &content).map_err(FirmwareManagementError::FromFileError)
    }

    fn increment_attempt(self) -> Self {
        Self {
            attempt: self.attempt + 1,
            ..self
        }
    }

    fn read_from_file(path: &Path) -> Result<Self, FirmwareManagementError> {
        let bytes = fs::read(path)?;
        serde_json::from_slice(&bytes).map_err(FirmwareManagementError::FromSerdeJsonError)
    }
}

struct DownloadFirmwareStatusMessage {}

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

fn validate_dir_exists(dir_path: &Path) -> Result<(), FirmwareManagementError> {
    if dir_path.exists() {
        Ok(())
    } else {
        Err(DirectoryNotFound {
            path: dir_path.to_path_buf(),
        })
    }
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

    #[test]
    fn persistent_store_path() {
        let persistent_store = PersistentStore::from(PathBuf::from("/some/dir"));
        assert_eq!(
            persistent_store.get_dir_path().to_str().unwrap(),
            "/some/dir/firmware"
        );
        assert_eq!(
            persistent_store.get_file_path("op-id").to_str().unwrap(),
            "/some/dir/firmware/op-id"
        )
    }
}
