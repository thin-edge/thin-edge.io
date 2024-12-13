use crate::error::FirmwareManagementError;
use crate::message::FirmwareOperationRequest;
use crate::message::FirmwareOperationResponse;
use crate::mpsc;
use crate::operation::FirmwareOperationEntry;
use crate::operation::OperationKey;
use crate::FirmwareManagerConfig;
use c8y_api::smartrest::message_ids::GET_PENDING_OPERATIONS;
use c8y_api::smartrest::smartrest_deserializer::SmartRestFirmwareRequest;
use c8y_api::smartrest::smartrest_serializer::fail_operation_with_name;
use c8y_api::smartrest::smartrest_serializer::set_operation_executing_with_name;
use c8y_api::smartrest::smartrest_serializer::succeed_operation_with_name_no_parameters;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::topic::C8yTopic;
use camino::Utf8PathBuf;
use log::error;
use log::info;
use sha256::digest;
use sha256::try_digest;
use std::fs;
use std::os::unix;
use std::path::Path;
use std::sync::Arc;
use tedge_actors::futures::StreamExt;
use tedge_actors::ClientMessageBox;
use tedge_actors::CloneSender;
use tedge_actors::DynSender;
use tedge_actors::Sender;
use tedge_api::OperationStatus;
use tedge_downloader_ext::DownloadRequest;
use tedge_downloader_ext::DownloadResult;
use tedge_mqtt_ext::MqttMessage;
use tedge_utils::file::move_file;
use tedge_utils::file::FileError;
use tedge_utils::file::PermissionEntry;
use tokio::time::timeout;

pub type IdDownloadResult = (String, DownloadResult);
pub type IdDownloadRequest = (String, DownloadRequest);

#[derive(Debug)]
pub struct OperationOutcome {
    pub operation: OperationKey,
    pub result: Result<(), FirmwareManagementError>,
}

/// A worker handle end-to-end a single firmware update request for a child device.
///
/// Such a worker is created by the main actor for each request
/// and the process is spawned in the background.
pub(crate) struct FirmwareManagerWorker {
    pub(crate) config: Arc<FirmwareManagerConfig>,
    executing: bool,
    mqtt_publisher: DynSender<MqttMessage>,
    download_sender: ClientMessageBox<IdDownloadRequest, IdDownloadResult>,
    progress_sender: DynSender<OperationOutcome>,
}

impl Clone for FirmwareManagerWorker {
    fn clone(&self) -> Self {
        FirmwareManagerWorker {
            config: self.config.clone(),
            executing: false,
            mqtt_publisher: self.mqtt_publisher.sender_clone(),
            download_sender: self.download_sender.clone(),
            progress_sender: self.progress_sender.sender_clone(),
        }
    }
}

impl FirmwareManagerWorker {
    pub(crate) fn new(
        config: FirmwareManagerConfig,
        mqtt_publisher: DynSender<MqttMessage>,
        download_sender: ClientMessageBox<IdDownloadRequest, IdDownloadResult>,
        progress_sender: DynSender<OperationOutcome>,
    ) -> Self {
        FirmwareManagerWorker {
            config: Arc::new(config),
            executing: false,
            mqtt_publisher,
            download_sender,
            progress_sender,
        }
    }

    pub(crate) fn spawn(
        self,
        operation_key: OperationKey,
        smartrest_request: SmartRestFirmwareRequest,
    ) -> DynSender<FirmwareOperationResponse> {
        let mut progress_sender: DynSender<OperationOutcome> = self.progress_sender.sender_clone();
        let (response_sender, response_receiver) = mpsc::channel(10);
        tokio::spawn(async move {
            let result = self
                .run(
                    operation_key.operation_id.clone(),
                    smartrest_request,
                    response_receiver,
                )
                .await;

            if let Err(err) = progress_sender
                .send(OperationOutcome {
                    operation: operation_key,
                    result,
                })
                .await
            {
                error!("Fail to forward operation progress due to: {err}");
            }
        });
        response_sender.into()
    }

    async fn run(
        mut self,
        operation_id: String,
        smartrest_request: SmartRestFirmwareRequest,
        mut response_receiver: mpsc::Receiver<FirmwareOperationResponse>,
    ) -> Result<(), FirmwareManagementError> {
        let child_id = smartrest_request.device.clone();
        let time_limit = self.config.timeout_sec;

        // Forward the request to the child device
        self.handle_firmware_download_request_child_device(smartrest_request, &operation_id)
            .await?;

        // Wait for a response of the child device
        let mut current_status = None;
        loop {
            let response = match timeout(time_limit, response_receiver.next()).await {
                Ok(Some(response)) => response,
                Ok(None) => {
                    // The main actor is shutting down
                    return Ok(());
                }
                Err(_elapsed) => {
                    // The child device failed to process the request within the time limit
                    return Err(FirmwareManagementError::ExceedTimeLimit {
                        child_id,
                        time_limit_sec: time_limit.as_secs(),
                        operation_id,
                    });
                }
            };

            // Proceed with the response of the child device
            self.handle_child_device_firmware_update_response(&mut current_status, &response)
                .await?;

            // Continue till the child is actively sending Executing messages
            if current_status != Some(OperationStatus::Executing) {
                break;
            }
        }

        Ok(())
    }

    // Check if the firmware file is already in cache.
    // If yes, publish a firmware request to child device with that firmware in the cache.
    // Otherwise, send a download request to the DownloaderActor awaiting for the download to complete.
    //
    // This method has to be spawned in a task
    // so other requests/responses can be processed while the download is in progress.
    async fn handle_firmware_download_request_child_device(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
        operation_id: &str,
    ) -> Result<(), FirmwareManagementError> {
        let firmware_url = smartrest_request.url.as_str();
        let file_cache_key = digest(firmware_url);
        let cache_file_path = self
            .config
            .validate_and_get_cache_dir_path()?
            .join(&file_cache_key);

        if cache_file_path.is_file() {
            info!(
                "Hit the file cache={}. File download is skipped.",
                cache_file_path.as_str()
            );
            // Publish a firmware update request to child device.
            self.handle_firmware_update_request_with_downloaded_file(
                smartrest_request,
                operation_id,
                &cache_file_path,
            )
            .await?;
        } else {
            info!(
                "Awaiting firmware download for op_id: {} from url: {}",
                operation_id, firmware_url
            );

            // Send a request to the Downloader to download the file asynchronously.
            let firmware_url = self.config.c8y_end_point.local_proxy_url(firmware_url)?;
            let download_request =
                DownloadRequest::new(firmware_url.as_str(), cache_file_path.as_std_path());

            let (_, download_result) = self
                .download_sender
                .await_response((operation_id.to_string(), download_request))
                .await?;
            match download_result {
                Ok(response) => {
                    // Publish a firmware update request to child device.
                    self.handle_firmware_update_request_with_downloaded_file(
                        smartrest_request,
                        operation_id,
                        &response.file_path,
                    )
                    .await?
                }
                Err(err) => {
                    return Err(FirmwareManagementError::FromDownloadError {
                        firmware_url: smartrest_request.url,
                        err,
                    });
                }
            }
        }
        Ok(())
    }

    // Publish a firmware update request to the child device
    // with firmware file path in the cache published via the file-transfer service and start the timer
    async fn handle_firmware_update_request_with_downloaded_file(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
        operation_id: &str,
        downloaded_firmware: impl AsRef<Path>,
    ) -> Result<(), FirmwareManagementError> {
        let child_id = smartrest_request.device.as_str();
        let firmware_url = smartrest_request.url.as_str();
        let file_cache_key = digest(firmware_url);
        let cache_dir_path = self.config.validate_and_get_cache_dir_path()?;
        let cache_file_path = cache_dir_path.join(&file_cache_key);

        // If the downloaded firmware is not already in the cache, move it there
        if !downloaded_firmware.as_ref().starts_with(&cache_dir_path) {
            move_file(
                &downloaded_firmware,
                &cache_file_path,
                PermissionEntry::new(None, None, None),
            )
            .await
            .map_err(FileError::from)?;
        }

        let symlink_path =
            self.create_file_transfer_symlink(child_id, &file_cache_key, &cache_file_path)?;
        let file_transfer_url = format!(
            "http://{}/tedge/file-transfer/{child_id}/firmware_update/{file_cache_key}",
            &self.config.local_http_host
        );
        let file_sha256 = try_digest(symlink_path.as_path())?;

        let operation_entry = FirmwareOperationEntry {
            operation_id: operation_id.to_string(),
            child_id: child_id.to_string(),
            name: smartrest_request.name.to_string(),
            version: smartrest_request.version.to_string(),
            server_url: firmware_url.to_string(),
            file_transfer_url: file_transfer_url.clone(),
            sha256: file_sha256.to_string(),
            attempt: 1,
        };

        operation_entry.create_status_file(self.config.data_dir.firmware_dir())?;

        self.publish_firmware_update_request(operation_entry)
            .await?;

        Ok(())
    }

    async fn handle_child_device_firmware_update_response(
        &mut self,
        current_status: &mut Option<OperationStatus>,
        response: &FirmwareOperationResponse,
    ) -> Result<(), FirmwareManagementError> {
        let child_device_payload = response.get_payload();
        let child_id = response.get_child_id();
        let operation_id = child_device_payload.operation_id.as_str();
        let received_status = child_device_payload.status;
        info!("Firmware update response received. Details: id={operation_id}, child={child_id}, status={received_status:?}");

        if current_status.is_none() {
            self.publish_c8y_executing_message(&child_id).await?;
        }
        *current_status = Some(received_status);

        match received_status {
            OperationStatus::Successful => {
                let status_file_path = self.config.data_dir.firmware_dir().join(operation_id);
                let operation_entry =
                    FirmwareOperationEntry::read_from_file(status_file_path.as_path())?;

                self.publish_c8y_installed_firmware_message(&operation_entry)
                    .await?;
                self.publish_c8y_successful_message(&child_id).await?;

                self.remove_status_file(operation_id)?;
            }
            OperationStatus::Failed => {
                self.publish_c8y_failed_message(
                    &child_id,
                    "No failure reason provided by child device.",
                )
                .await?;
                self.remove_status_file(operation_id)?;
            }
            OperationStatus::Executing => {
                // Starting timer again means extending the timer.
            }
        }

        Ok(())
    }

    pub(crate) fn remove_status_file(
        &mut self,
        operation_id: &str,
    ) -> Result<(), FirmwareManagementError> {
        let status_file_path = self
            .config
            .validate_and_get_firmware_dir_path()?
            .join(operation_id);
        if status_file_path.exists() {
            fs::remove_file(status_file_path)?;
        }
        Ok(())
    }

    pub(crate) async fn publish_firmware_update_request(
        &mut self,
        operation_entry: FirmwareOperationEntry,
    ) -> Result<(), FirmwareManagementError> {
        let mqtt_message: MqttMessage =
            FirmwareOperationRequest::from(operation_entry.clone()).try_into()?;
        self.mqtt_publisher.send(mqtt_message).await?;
        info!(
            "Firmware update request is sent. operation_id={}, child={}",
            operation_entry.operation_id, operation_entry.child_id
        );
        Ok(())
    }

    pub(crate) async fn publish_c8y_executing_message(
        &mut self,
        child_id: &str,
    ) -> Result<(), FirmwareManagementError> {
        self.executing = true;

        let c8y_child_topic = C8yTopic::ChildSmartRestResponse(child_id.to_string())
            .to_topic(&self.config.c8y_prefix)?;
        let payload = set_operation_executing_with_name(CumulocitySupportedOperations::C8yFirmware);
        let executing_msg = MqttMessage::new(&c8y_child_topic, payload);
        self.mqtt_publisher.send(executing_msg).await?;
        Ok(())
    }

    pub(crate) async fn publish_c8y_successful_message(
        &mut self,
        child_id: &str,
    ) -> Result<(), FirmwareManagementError> {
        if !self.executing {
            self.publish_c8y_executing_message(child_id).await?;
        }
        let c8y_child_topic = C8yTopic::ChildSmartRestResponse(child_id.to_string())
            .to_topic(&self.config.c8y_prefix)?;
        let payload =
            succeed_operation_with_name_no_parameters(CumulocitySupportedOperations::C8yFirmware);
        let successful_msg = MqttMessage::new(&c8y_child_topic, payload);
        self.mqtt_publisher.send(successful_msg).await?;
        Ok(())
    }

    pub(crate) async fn publish_c8y_failed_message(
        &mut self,
        child_id: &str,
        failure_reason: &str,
    ) -> Result<(), FirmwareManagementError> {
        if !self.executing {
            self.publish_c8y_executing_message(child_id).await?;
        }
        let c8y_child_topic = C8yTopic::ChildSmartRestResponse(child_id.to_string())
            .to_topic(&self.config.c8y_prefix)?;
        let payload =
            fail_operation_with_name(CumulocitySupportedOperations::C8yFirmware, failure_reason);
        let failed_msg = MqttMessage::new(&c8y_child_topic, payload);
        self.mqtt_publisher.send(failed_msg).await?;
        Ok(())
    }

    async fn publish_c8y_installed_firmware_message(
        &mut self,
        operation_entry: &FirmwareOperationEntry,
    ) -> Result<(), FirmwareManagementError> {
        use c8y_api::smartrest::message_ids::SET_FIRMWARE;
        let c8y_child_topic = C8yTopic::ChildSmartRestResponse(operation_entry.child_id.clone())
            .to_topic(&self.config.c8y_prefix)?;
        let installed_firmware_payload = format!(
            "{SET_FIRMWARE},{},{},{}",
            operation_entry.name, operation_entry.version, operation_entry.server_url
        );
        let installed_firmware_message =
            MqttMessage::new(&c8y_child_topic, installed_firmware_payload);
        self.mqtt_publisher.send(installed_firmware_message).await?;
        Ok(())
    }

    // The symlink path should be <tedge-data-dir>/file-transfer/<child-id>/firmware_update/<file_cache_key>
    fn create_file_transfer_symlink(
        &self,
        child_id: &str,
        file_cache_key: &str,
        original_file_path: impl AsRef<Path>,
    ) -> Result<Utf8PathBuf, FirmwareManagementError> {
        let file_transfer_dir_path = self.config.validate_and_get_file_transfer_dir_path()?;

        let symlink_dir_path = file_transfer_dir_path
            .join(child_id)
            .join("firmware_update");
        let symlink_path = symlink_dir_path.join(file_cache_key);

        if !symlink_path.is_symlink() {
            fs::create_dir_all(symlink_dir_path)?;
            unix::fs::symlink(original_file_path, &symlink_path)?;
        }
        Ok(symlink_path)
    }

    // Candidate to be removed since another actor should be in charge of this.
    pub(crate) async fn get_pending_operations_from_cloud(
        &mut self,
    ) -> Result<(), FirmwareManagementError> {
        let message = MqttMessage::new(
            &C8yTopic::upstream_topic(&self.config.c8y_prefix),
            GET_PENDING_OPERATIONS.to_string(),
        );
        self.mqtt_publisher.send(message).await?;
        Ok(())
    }
}
