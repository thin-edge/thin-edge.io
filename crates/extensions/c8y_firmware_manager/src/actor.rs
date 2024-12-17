use crate::config::FirmwareManagerConfig;
use crate::error::FirmwareManagementError;
use crate::message::FirmwareOperationResponse;
use crate::operation::FirmwareOperationEntry;
use crate::operation::OperationKey;
use crate::worker::FirmwareManagerWorker;
use crate::worker::IdDownloadRequest;
use crate::worker::IdDownloadResult;
use crate::worker::OperationOutcome;
use async_trait::async_trait;
use c8y_api::smartrest::message::collect_smartrest_messages;
use c8y_api::smartrest::message::get_smartrest_template_id;
use c8y_api::smartrest::message_ids::FIRMWARE;
use c8y_api::smartrest::smartrest_deserializer::SmartRestFirmwareRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use log::error;
use log::info;
use log::warn;
use nanoid::nanoid;
use std::collections::HashMap;
use std::fs;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ClientMessageBox;
use tedge_actors::DynSender;
use tedge_actors::LoggingReceiver;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_mqtt_ext::MqttMessage;

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
    pub(crate) fn new(
        config: FirmwareManagerConfig,
        input_receiver: LoggingReceiver<FirmwareInput>,
        mqtt_publisher: DynSender<MqttMessage>,
        download_sender: ClientMessageBox<IdDownloadRequest, IdDownloadResult>,
        progress_sender: DynSender<OperationOutcome>,
    ) -> Self {
        Self {
            input_receiver,
            worker: FirmwareManagerWorker::new(
                config,
                mqtt_publisher,
                download_sender,
                progress_sender,
            ),
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
            let smartrest_template_id = get_smartrest_template_id(&smartrest_message);
            let result = match smartrest_template_id.as_str().parse::<usize>() {
                Ok(id) if id == FIRMWARE => {
                    match SmartRestFirmwareRequest::from_smartrest(&smartrest_message) {
                        Ok(firmware_request) => {
                            // Addressing a new firmware operation to further step.
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

    // This is the start point function when receiving a firmware response from child device.
    async fn handle_child_device_firmware_operation_response(
        &mut self,
        message: MqttMessage,
    ) -> Result<(), FirmwareManagementError> {
        match FirmwareOperationResponse::try_from(&message) {
            Ok(response) => {
                if let Err(err) =
                    // Address the received response depending on the payload.
                    self
                        .handle_child_device_firmware_update_response(response.clone())
                        .await
                {
                    self.fail_operation_in_cloud(
                        &response.get_child_id(),
                        Some(response.get_payload().operation_id.as_str()),
                        &err.to_string(),
                    )
                    .await?;
                }
            }
            Err(err) => {
                // Ignore bad responses. Eventually, timeout will fail an operation.
                error!("Received a firmware update response with invalid payload:  {err}");
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

    // This function can be removed once we start using operation ID from c8y.
    async fn validate_same_request_in_progress(
        &mut self,
        smartrest_request: SmartRestFirmwareRequest,
    ) -> Result<(), FirmwareManagementError> {
        let firmware_dir_path = self.worker.config.validate_and_get_firmware_dir_path()?;

        for entry in fs::read_dir(firmware_dir_path.clone())? {
            match entry {
                Ok(file_path) => match FirmwareOperationEntry::read_from_file(file_path.path()) {
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

    fn remove_entry_from_active_operations(&mut self, operation_key: &OperationKey) {
        self.active_child_ops.remove(operation_key);
    }
}
