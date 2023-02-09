use crate::child_device::FirmwareOperationRequest;
use crate::child_device::FirmwareOperationResponse;
use crate::common::mark_pending_firmware_operation_failed;
use crate::common::ActiveOperationState;
use crate::common::FirmwareEntry;
use crate::error::FirmwareManagementError;
use crate::firmware_manager::DEFAULT_OPERATION_TIMEOUT;
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
use sha256::digest;
use sha256::try_digest;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
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
#[cfg(not(test))]
pub const FILE_TRANSFER_CACHE_PATH: &str = "/var/tedge/cache";
#[cfg(test)]
pub const FILE_TRANSFER_ROOT_PATH: &str = "/tmp/file-transfer";
#[cfg(test)]
pub const FILE_TRANSFER_CACHE_PATH: &str = "/tmp/cache";

pub struct FirmwareDownloadManager {
    tedge_device_id: String,
    mqtt_publisher: UnboundedSender<Message>,
    http_client: Arc<Mutex<dyn C8YHttpProxy>>,
    local_http_host: String,
    tmp_dir: PathBuf,
    pub operation_timer: Timers<(String, String), ActiveOperationState>,
    pub url_map: HashMap<String, String>,
}

impl FirmwareDownloadManager {
    pub fn new(
        tedge_device_id: String,
        mqtt_publisher: UnboundedSender<Message>,
        http_client: Arc<Mutex<dyn C8YHttpProxy>>,
        local_http_host: String,
        tmp_dir: PathBuf,
    ) -> Self {
        FirmwareDownloadManager {
            tedge_device_id,
            mqtt_publisher,
            http_client,
            local_http_host,
            tmp_dir,
            operation_timer: Timers::new(),
            url_map: HashMap::new(),
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
                    let failed_reason = format!("{err}");
                    mark_pending_firmware_operation_failed(
                        self.mqtt_publisher.clone(),
                        child_id,
                        ActiveOperationState::Pending,
                        failed_reason,
                    )
                    .await
                    .unwrap_or_else(|_| error!("Failed to update the operation status to failed."));
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
        let url_hash = digest(smartrest_request.url.as_str());

        // <tedge-cache-root>/<url_hash>
        let cache_dest = PathBuf::from(FILE_TRANSFER_CACHE_PATH).join(&url_hash);
        let cache_dest_str = format!("{FILE_TRANSFER_CACHE_PATH}/{url_hash}");

        // <tedge-file-transfer-root>/<child-id>/firmware_update/<url_hash>
        let transfer_dest = PathBuf::from(FILE_TRANSFER_ROOT_PATH)
            .join(child_id)
            .join("firmware_update")
            .join(&url_hash);

        let http_transfer_url = format!(
            "http://{}/tedge/file-transfer/{child_id}/firmware_update/{url_hash}",
            &self.local_http_host
        );

        // If dir already exists, these calls do nothing.
        create_parent_dirs(&transfer_dest)?;
        create_parent_dirs(&cache_dest)?;

        if cache_dest.is_file() {
            info!("The file is already available in {cache_dest_str}. Skip to download.");
        } else {
            match self
                .http_client
                .lock()
                .await
                .download_file(&smartrest_request.url, &url_hash, &self.tmp_dir)
                .await
            {
                Ok(tmp_file_path) => {
                    move_file(&tmp_file_path, &cache_dest)?;
                    info!("Downloading a file is successful. Stored in {cache_dest_str}");
                }
                Err(err) => {
                    error!(
                        "Downloading a file from {} failed.",
                        smartrest_request.url.as_str()
                    );
                    return Err(err.into());
                }
            }
        }

        // Create a symlink if it doesn't exist yet.
        if !transfer_dest.is_file() {
            unix_fs::symlink(&cache_dest, &transfer_dest)?;
        }

        // Add a pair of local url and external url to the hashmap.
        self.url_map
            .insert(http_transfer_url.clone(), smartrest_request.url.clone());

        let file_sha256 = try_digest(transfer_dest.as_path())?;
        let firmware_entry = FirmwareEntry::new(
            &smartrest_request.name,
            &smartrest_request.version,
            &smartrest_request.url,
            &file_sha256,
        );
        let firmware_op_req = FirmwareOperationRequest::new(child_id, firmware_entry);
        let firmware_update_req_msg = Message::new(
            &firmware_op_req.operation_request_topic(),
            firmware_op_req.operation_request_payload(http_transfer_url.as_ref())?,
        );

        self.mqtt_publisher.send(firmware_update_req_msg).await?;
        info!(
            "Firmware update request for name \"{}\" sent to child device \"{}\"",
            &smartrest_request.name, &child_id
        );

        // A key for timer. Once operation ID is possible to use, better to replace it.
        let operation_hash = digest(format!(
            "{}{}{}{}",
            &child_id, &smartrest_request.name, &smartrest_request.version, &smartrest_request.url
        ));

        info!("The operation hash={operation_hash}");
        self.operation_timer.start_timer(
            (child_id.to_string(), operation_hash),
            ActiveOperationState::Pending,
            DEFAULT_OPERATION_TIMEOUT,
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
        let firmware_name = &child_device_payload.name;
        let firmware_version = &child_device_payload.version;
        let file_transfer_url = &child_device_payload.url;
        info!("Firmware update response received. Detais: child={child_id}, name={firmware_name}, version={firmware_version}, url={file_transfer_url}");

        let external_server_url = self.url_map.get(file_transfer_url).ok_or(
            FirmwareManagementError::InvalidLocalURL {
                url: file_transfer_url.clone(),
            },
        )?;

        let operation_hash = digest(format!(
            "{}{}{}{}",
            &child_id, &firmware_name, &firmware_version, &external_server_url
        ));

        let current_operation_state = self
            .operation_timer
            .current_value(&(child_id.to_string(), operation_hash.clone()));

        let mut mapped_responses = vec![];
        if current_operation_state != Some(&ActiveOperationState::Executing) {
            let executing_status_payload = DownloadFirmwareStatusMessage::status_executing()?;
            mapped_responses.push(Message::new(&c8y_child_topic, executing_status_payload));
        }

        match child_device_payload.status {
            OperationStatus::Successful => {
                self.operation_timer
                    .stop_timer((child_id.to_string(), operation_hash));

                let update_firmware_state_message = format!(
                    "115,{},{},{}",
                    &child_id, child_device_payload.version, &external_server_url
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
                self.operation_timer.stop_timer((child_id, operation_hash));

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
                    (child_id, operation_hash),
                    ActiveOperationState::Executing,
                    DEFAULT_OPERATION_TIMEOUT,
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

fn create_parent_dirs(path: &Path) -> Result<(), FirmwareManagementError> {
    if let Some(dest_dir) = path.parent() {
        if !dest_dir.exists() {
            fs::create_dir_all(dest_dir)?;
        }
    }
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
