use async_trait::async_trait;
use c8y_api::smartrest::message::collect_smartrest_messages;
use c8y_api::smartrest::message::get_smartrest_template_id;
use c8y_api::smartrest::smartrest_deserializer::SmartRestFirmwareRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::smartrest_serializer::OperationStatusMessage;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::credentials::JwtRetriever;
use camino::Utf8PathBuf;
use log::error;
use log::info;
use log::warn;
use nanoid::nanoid;
use sha256::digest;
use sha256::try_digest;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::Path;
use std::sync::Arc;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ClientMessageBox;
use tedge_actors::CloneSender;
use tedge_actors::DynSender;
use tedge_actors::LoggingReceiver;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_api::topic::get_child_id_from_child_topic;
use tedge_api::Auth;
use tedge_api::OperationStatus;
use tedge_downloader_ext::DownloadRequest;
use tedge_downloader_ext::DownloadResult;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_utils::file::move_file;
use tedge_utils::file::FileError;
use tedge_utils::file::PermissionEntry;
use tokio::time::timeout;

use crate::config::FirmwareManagerConfig;
use crate::error::FirmwareManagementError;
use crate::message::DownloadFirmwareStatusMessage;
use crate::message::FirmwareOperationRequest;
use crate::message::FirmwareOperationResponse;
use crate::mpsc;
use crate::operation::FirmwareOperationEntry;
use crate::operation::OperationKey;

pub type IdDownloadResult = (String, DownloadResult);
pub type IdDownloadRequest = (String, DownloadRequest);

#[derive(Debug)]
pub struct OperationOutcome {
    operation: OperationKey,
    result: Result<(), FirmwareManagementError>,
}

fan_in_message_type!(FirmwareInput[MqttMessage, OperationOutcome] : Debug);

pub struct FirmwareManagerActor {
    input_receiver: LoggingReceiver<FirmwareInput>,
    worker: FirmwareManagerWorker,
    active_child_ops: HashMap<OperationKey, DynSender<FirmwareOperationResponse>>,
}

#[async_trait]
impl Actor for FirmwareManagerActor {
    fn name(&self) -> &str {
        "FirmwareManager"
    }

    // This actor handles 2 kinds of messages from its peer actors:
    //
    // 1. MQTT messages from the MqttActor for firmware update requests from the cloud and firmware update responses from the child devices
    // 2. RequestOutcome sent back by the background workers once the firmware request has been fully processed or failed
    async fn run(mut self) -> Result<(), RuntimeError> {
        self.resend_operations_to_child_device().await?;
        // TODO: We need a dedicated actor to publish 500 later.
        self.worker.get_pending_operations_from_cloud().await?;

        info!("Ready to serve firmware requests.");
        while let Some(event) = self.input_receiver.recv().await {
            match event {
                FirmwareInput::MqttMessage(message) => {
                    self.process_mqtt_message(message).await?;
                }
                FirmwareInput::OperationOutcome(outcome) => {
                    if let Err(err) = outcome.result {
                        self.fail_operation_in_cloud(
                            &outcome.operation.child_id,
                            Some(&outcome.operation.operation_id),
                            &err.to_string(),
                        )
                        .await?;
                    } else {
                        self.worker
                            .publish_c8y_successful_message(&outcome.operation.child_id)
                            .await?;
                    }
                    self.remove_entry_from_active_operations(&outcome.operation);
                }
            }
        }
        Ok(())
    }
}

impl FirmwareManagerActor {
    pub fn new(
        config: FirmwareManagerConfig,
        input_receiver: LoggingReceiver<FirmwareInput>,
        mqtt_publisher: DynSender<MqttMessage>,
        jwt_retriever: JwtRetriever,
        download_sender: ClientMessageBox<IdDownloadRequest, IdDownloadResult>,
        progress_sender: DynSender<OperationOutcome>,
    ) -> Self {
        Self {
            input_receiver,
            worker: FirmwareManagerWorker {
                config: Arc::new(config),
                executing: false,
                mqtt_publisher,
                jwt_retriever,
                download_sender,
                progress_sender,
            },
            active_child_ops: HashMap::new(),
        }
    }

    // Based on the topic name, process either a new firmware update operation from the cloud or a response from child device.
    pub async fn process_mqtt_message(
        &mut self,
        message: MqttMessage,
    ) -> Result<(), FirmwareManagementError> {
        if self.worker.config.c8y_request_topics.accept(&message) {
            // New firmware operation from c8y
            self.handle_firmware_update_smartrest_request(message)
                .await?;
        } else if self
            .worker
            .config
            .firmware_update_response_topics
            .accept(&message)
        {
            // Response from child device
            self.handle_child_device_firmware_operation_response(message.clone())
                .await?;
        } else {
            error!(
                "Received unexpected message on topic: {}",
                message.topic.name
            );
        }
        Ok(())
    }

    // This is the start point function when receiving a new c8y_Firmware operation from c8y.
    pub async fn handle_firmware_update_smartrest_request(
        &mut self,
        message: MqttMessage,
    ) -> Result<(), FirmwareManagementError> {
        for smartrest_message in collect_smartrest_messages(message.payload_str()?) {
            let result = match get_smartrest_template_id(&smartrest_message).as_str() {
                "515" => match SmartRestFirmwareRequest::from_smartrest(&smartrest_message) {
                    Ok(firmware_request) => {
                        // Addressing a new firmware operation to further step.
                        self.handle_firmware_download_request(firmware_request)
                            .await
                    }
                    Err(_) => {
                        error!("Incorrect c8y_Firmware SmartREST payload: {smartrest_message}");
                        Ok(())
                    }
                },
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

    // Validates the received SmartREST request and processes it further if it's meant for a child device
    async fn handle_firmware_download_request(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
    ) -> Result<(), FirmwareManagementError> {
        info!("Handling c8y_Firmware operation: {smartrest_request}");

        if smartrest_request.device == self.worker.config.tedge_device_id {
            warn!("c8y-firmware-plugin does not support firmware operation for the main tedge device. \
            Please define a custom operation handler for the c8y_Firmware operation.");
            return Ok(());
        }

        let child_id = smartrest_request.device.clone();

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

        // Addressing the new firmware operation to further step.
        let operation_id = nanoid!();
        let operation_key = OperationKey::new(&child_id, &operation_id);

        let worker = self.worker.clone();
        let worker_sender = worker.spawn(operation_key.clone(), smartrest_request);
        self.active_child_ops.insert(operation_key, worker_sender);

        Ok(())
    }
}

impl FirmwareManagerWorker {
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
            let download_request = if self
                .config
                .c8y_end_point
                .maybe_tenant_url(firmware_url)
                .is_some()
            {
                if let Ok(token) = self.jwt_retriever.await_response(()).await? {
                    DownloadRequest::new(firmware_url, cache_file_path.as_std_path())
                        .with_auth(Auth::new_bearer(&token))
                } else {
                    return Err(FirmwareManagementError::NoJwtToken);
                }
            } else {
                DownloadRequest::new(firmware_url, cache_file_path.as_std_path())
            };

            let (_, download_result) = self
                .download_sender
                .await_response((operation_id.to_string(), download_request))
                .await?;
            self.process_downloaded_firmware(operation_id, smartrest_request, download_result)
                .await?;
        }
        Ok(())
    }

    // This function is called on receiving a DownloadResult from the DownloaderActor.
    // If the download is successful, publish a firmware request to child device with it
    // Otherwise, fail the operation in the cloud
    async fn process_downloaded_firmware(
        &mut self,
        operation_id: &str,
        smartrest_request: SmartRestFirmwareRequest,
        download_result: DownloadResult,
    ) -> Result<(), FirmwareManagementError> {
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
}

impl FirmwareManagerActor {
    // This is the start point function when receiving a firmware response from child device.
    async fn handle_child_device_firmware_operation_response(
        &mut self,
        message: MqttMessage,
    ) -> Result<(), FirmwareManagementError> {
        let topic_name = &message.topic.name;
        let child_id = get_child_id_from_child_topic(topic_name).ok_or(
            FirmwareManagementError::InvalidTopicFromChildOperation {
                topic: topic_name.to_string(),
            },
        )?;

        match FirmwareOperationResponse::try_from(&message) {
            Ok(response) => {
                if let Err(err) =
                    // Address the received response depending on the payload.
                    self
                        .handle_child_device_firmware_update_response(response.clone())
                        .await
                {
                    self.fail_operation_in_cloud(
                        &child_id,
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

    async fn handle_child_device_firmware_update_response(
        &mut self,
        response: FirmwareOperationResponse,
    ) -> Result<(), FirmwareManagementError> {
        let child_device_payload = response.get_payload();
        let child_id = response.get_child_id();
        let operation_id = child_device_payload.operation_id.as_str();
        let operation_key = OperationKey::new(&child_id, operation_id);

        match self.active_child_ops.get_mut(&operation_key) {
            None => {
                info!("Received a response from {child_id} for unknown request {operation_id}");
                return Ok(());
            }
            Some(worker) => {
                // forward the response to the worker
                worker.send(response).await?;
            }
        }

        Ok(())
    }
}

impl FirmwareManagerWorker {
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
}

impl FirmwareManagerActor {
    // This function can be removed once we start using operation ID from c8y.
    async fn validate_same_request_in_progress(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
    ) -> Result<(), FirmwareManagementError> {
        let firmware_dir_path = self.worker.config.validate_and_get_firmware_dir_path()?;

        for entry in fs::read_dir(firmware_dir_path.clone())? {
            match entry {
                Ok(file_path) => match FirmwareOperationEntry::read_from_file(&file_path.path()) {
                    Ok(recorded_entry) => {
                        if recorded_entry.child_id == smartrest_request.device
                            && recorded_entry.name == smartrest_request.name
                            && recorded_entry.version == smartrest_request.version
                            && recorded_entry.server_url == smartrest_request.url
                        {
                            info!("The same operation as the received c8y_Firmware operation is already in progress.");

                            // Resend a firmware request with incremented attempt.
                            let new_operation_entry = recorded_entry.increment_attempt();
                            new_operation_entry.overwrite_file(&firmware_dir_path)?;
                            self.worker
                                .publish_firmware_update_request(new_operation_entry)
                                .await?;

                            return Err(FirmwareManagementError::RequestAlreadyAddressed);
                        }
                    }
                    Err(err) => {
                        warn!("Error: {err} while reading the contents of persistent store directory {}",
                            firmware_dir_path.as_str());
                        continue;
                    }
                },
                Err(err) => {
                    warn!(
                        "Error: {err} while reading the contents of persistent store directory {}",
                        firmware_dir_path.as_str()
                    );
                    continue;
                }
            }
        }
        Ok(())
    }

    async fn fail_operation_in_cloud(
        &mut self,
        child_id: &str,
        op_id: Option<&str>,
        failure_reason: &str,
    ) -> Result<(), FirmwareManagementError> {
        error!("{}", failure_reason);
        if let Some(operation_id) = op_id {
            self.worker.remove_status_file(operation_id)?;
            self.worker
                .publish_c8y_failed_message(child_id, failure_reason)
                .await?;
        };

        Ok(())
    }

    async fn resend_operations_to_child_device(&mut self) -> Result<(), FirmwareManagementError> {
        let firmware_dir_path = self.worker.config.data_dir.firmware_dir().clone();
        if !firmware_dir_path.is_dir() {
            // Do nothing if the persistent store directory does not exist yet.
            return Ok(());
        }

        for entry in fs::read_dir(&firmware_dir_path)? {
            let file_path = entry?.path();
            if file_path.is_file() {
                let operation_entry =
                    FirmwareOperationEntry::read_from_file(&file_path)?.increment_attempt();

                operation_entry.overwrite_file(&firmware_dir_path)?;
                self.worker
                    .publish_firmware_update_request(operation_entry)
                    .await?;
            }
        }
        Ok(())
    }
}

impl FirmwareManagerWorker {
    fn remove_status_file(&mut self, operation_id: &str) -> Result<(), FirmwareManagementError> {
        let status_file_path = self
            .config
            .validate_and_get_firmware_dir_path()?
            .join(operation_id);
        if status_file_path.exists() {
            fs::remove_file(status_file_path)?;
        }
        Ok(())
    }

    async fn publish_firmware_update_request(
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

    async fn publish_c8y_executing_message(
        &mut self,
        child_id: &str,
    ) -> Result<(), FirmwareManagementError> {
        self.executing = true;

        let c8y_child_topic = Topic::new_unchecked(
            &C8yTopic::ChildSmartRestResponse(child_id.to_string()).to_string(),
        );
        let executing_msg = MqttMessage::new(
            &c8y_child_topic,
            DownloadFirmwareStatusMessage::status_executing(),
        );
        self.mqtt_publisher.send(executing_msg).await?;
        Ok(())
    }

    async fn publish_c8y_successful_message(
        &mut self,
        child_id: &str,
    ) -> Result<(), FirmwareManagementError> {
        if !self.executing {
            self.publish_c8y_executing_message(child_id).await?;
        }
        let c8y_child_topic = Topic::new_unchecked(
            &C8yTopic::ChildSmartRestResponse(child_id.to_string()).to_string(),
        );
        let successful_msg = MqttMessage::new(
            &c8y_child_topic,
            DownloadFirmwareStatusMessage::status_successful(None),
        );
        self.mqtt_publisher.send(successful_msg).await?;
        Ok(())
    }

    async fn publish_c8y_failed_message(
        &mut self,
        child_id: &str,
        failure_reason: &str,
    ) -> Result<(), FirmwareManagementError> {
        if !self.executing {
            self.publish_c8y_executing_message(child_id).await?;
        }
        let c8y_child_topic = Topic::new_unchecked(
            &C8yTopic::ChildSmartRestResponse(child_id.to_string()).to_string(),
        );
        let failed_msg = MqttMessage::new(
            &c8y_child_topic,
            DownloadFirmwareStatusMessage::status_failed(failure_reason),
        );
        self.mqtt_publisher.send(failed_msg).await?;
        Ok(())
    }

    async fn publish_c8y_installed_firmware_message(
        &mut self,
        operation_entry: &FirmwareOperationEntry,
    ) -> Result<(), FirmwareManagementError> {
        let c8y_child_topic = Topic::new_unchecked(
            &C8yTopic::ChildSmartRestResponse(operation_entry.child_id.clone()).to_string(),
        );
        let installed_firmware_payload = format!(
            "115,{},{},{}",
            operation_entry.name, operation_entry.version, operation_entry.server_url
        );
        let installed_firmware_message =
            MqttMessage::new(&c8y_child_topic, installed_firmware_payload);
        self.mqtt_publisher.send(installed_firmware_message).await?;
        Ok(())
    }
}

impl FirmwareManagerActor {
    fn remove_entry_from_active_operations(&mut self, operation_key: &OperationKey) {
        self.active_child_ops.remove(operation_key);
    }
}

impl FirmwareManagerWorker {
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
            unix_fs::symlink(original_file_path, &symlink_path)?;
        }
        Ok(symlink_path)
    }

    // Candidate to be removed since another actor should be in charge of this.
    async fn get_pending_operations_from_cloud(&mut self) -> Result<(), FirmwareManagementError> {
        let message = MqttMessage::new(&C8yTopic::SmartRestResponse.to_topic()?, "500");
        self.mqtt_publisher.send(message).await?;
        Ok(())
    }
}

struct FirmwareManagerWorker {
    config: Arc<FirmwareManagerConfig>,
    executing: bool,
    mqtt_publisher: DynSender<MqttMessage>,
    jwt_retriever: JwtRetriever,
    download_sender: ClientMessageBox<IdDownloadRequest, IdDownloadResult>,
    progress_sender: DynSender<OperationOutcome>,
}

impl Clone for FirmwareManagerWorker {
    fn clone(&self) -> Self {
        FirmwareManagerWorker {
            config: self.config.clone(),
            executing: false,
            mqtt_publisher: self.mqtt_publisher.sender_clone(),
            jwt_retriever: self.jwt_retriever.clone(),
            download_sender: self.download_sender.clone(),
            progress_sender: self.progress_sender.sender_clone(),
        }
    }
}

use tedge_actors::futures::StreamExt;

impl FirmwareManagerWorker {
    pub fn spawn(
        self,
        operation_key: OperationKey,
        smartrest_request: SmartRestFirmwareRequest,
    ) -> DynSender<FirmwareOperationResponse> {
        let mut progress_sender: DynSender<OperationOutcome> = self.progress_sender.sender_clone();
        let (input_sender, input_receiver) = mpsc::channel(10);
        tokio::spawn(async move {
            let result = self
                .run(
                    operation_key.operation_id.clone(),
                    smartrest_request,
                    input_receiver,
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
        input_sender.into()
    }

    pub async fn run(
        mut self,
        operation_id: String,
        smartrest_request: SmartRestFirmwareRequest,
        mut input_receiver: mpsc::Receiver<FirmwareOperationResponse>,
    ) -> Result<(), FirmwareManagementError> {
        let child_id = smartrest_request.device.clone();
        let time_limit = self.config.timeout_sec;

        // Forward the request to the child device
        self.handle_firmware_download_request_child_device(smartrest_request, &operation_id)
            .await?;

        // Wait for a response of the child device
        let mut current_status = None;
        loop {
            let response = match timeout(time_limit, input_receiver.next()).await {
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
}
