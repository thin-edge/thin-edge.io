use crate::child_device::FirmwareOperationRequest;
use crate::child_device::FirmwareOperationResponse;
use crate::common::create_parent_dirs;
use crate::common::mark_pending_firmware_operation_failed;
use crate::common::ActiveOperationState;
use crate::common::FirmwareOperationEntry;
use crate::common::PersistentStore;
use crate::error::FirmwareManagementError;
use c8y_api::http_proxy::C8YHttpProxy;
use c8y_api::smartrest::error::SmartRestSerializerError;
use c8y_api::smartrest::smartrest_deserializer::SmartRestFirmwareRequest;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRest;
use c8y_api::smartrest::smartrest_serializer::SmartRestSerializer;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToExecuting;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToFailed;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToSuccessful;
use c8y_api::smartrest::smartrest_serializer::TryIntoOperationStatusMessage;
use mqtt_channel::Message;
use mqtt_channel::SinkExt;
use mqtt_channel::Topic;
use mqtt_channel::UnboundedSender;
use nanoid::nanoid;
use sha256::digest;
use sha256::try_digest;
use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tedge_api::OperationStatus;
use tedge_utils::timers::Timers;
use tokio::sync::Mutex;
use tracing::error;
use tracing::info;
use tracing::warn;

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

pub struct FirmwareDownloadManager {
    tedge_device_id: String,
    mqtt_publisher: UnboundedSender<Message>,
    http_client: Arc<Mutex<dyn C8YHttpProxy>>,
    local_http_host: String,
    tmp_dir: PathBuf,
    pub operation_timer: Timers<(String, String), ActiveOperationState>,
    pub timeout_sec: Duration,
}

impl FirmwareDownloadManager {
    pub fn new(
        tedge_device_id: String,
        mqtt_publisher: UnboundedSender<Message>,
        http_client: Arc<Mutex<dyn C8YHttpProxy>>,
        local_http_host: String,
        tmp_dir: PathBuf,
        timeout_sec: Duration,
    ) -> Self {
        FirmwareDownloadManager {
            tedge_device_id,
            mqtt_publisher,
            http_client,
            local_http_host,
            tmp_dir,
            operation_timer: Timers::new(),
            timeout_sec,
        }
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
                    mark_pending_firmware_operation_failed(
                        self.mqtt_publisher.clone(),
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
            info!("Hit the cache={cache_dest_str}. File download is skipped.");
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
        let message = Message::new(&request.get_topic(), request.get_json_payload()?);

        self.mqtt_publisher.send(message).await?;
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
}

fn move_file(src: &Path, dest: &Path) -> Result<(), FirmwareManagementError> {
    fs::copy(src, dest).map_err(|_| FirmwareManagementError::FileCopyFailed {
        src: src.to_path_buf(),
        dest: dest.to_path_buf(),
    })?;

    Ok(())
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
