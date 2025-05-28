//! Handling of different types of thin-edge.io operations.

mod config_snapshot;
mod config_update;
mod custom_operation;
mod device_profile;
mod firmware_update;
mod log_upload;
mod restart;
mod software_list;
mod software_update;

use super::error;
use super::error::OperationError;
use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::actor::IdUploadRequest;
use crate::actor::IdUploadResult;
use crate::Capabilities;
use c8y_api::smartrest::payload::SmartrestPayload;
use c8y_api::smartrest::smartrest_serializer::fail_operation_with_id;
use c8y_api::smartrest::smartrest_serializer::fail_operation_with_name;
use c8y_api::smartrest::smartrest_serializer::set_operation_executing_with_id;
use c8y_api::smartrest::smartrest_serializer::set_operation_executing_with_name;
use c8y_api::smartrest::smartrest_serializer::succeed_operation_with_id;
use c8y_api::smartrest::smartrest_serializer::succeed_operation_with_id_no_parameters;
use c8y_api::smartrest::smartrest_serializer::succeed_operation_with_name;
use c8y_api::smartrest::smartrest_serializer::succeed_operation_with_name_no_parameters;
use c8y_api::smartrest::smartrest_serializer::succeed_static_operation_with_id;
use c8y_api::smartrest::smartrest_serializer::succeed_static_operation_with_name;
use c8y_api::smartrest::smartrest_serializer::C8yOperation;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::TextOrCsv;
use c8y_http_proxy::handle::C8YHttpProxy;
use camino::Utf8Path;
use std::sync::Arc;
use tedge_actors::ClientMessageBox;
use tedge_actors::LoggingSender;
use tedge_actors::Sender;
use tedge_api::entity::EntityExternalId;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::IdGenerator;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::GenericCommandState;
use tedge_config::models::AutoLogUpload;
use tedge_config::models::SoftwareManagementApiFlag;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::Topic;
use tracing::debug;
use tracing::error;

/// State required by the operation handlers.
pub(super) struct OperationContext {
    pub(super) capabilities: Capabilities,
    pub(super) auto_log_upload: AutoLogUpload,
    pub(super) tedge_http_host: Arc<str>,
    pub(super) tmp_dir: Arc<Utf8Path>,
    pub(super) mqtt_schema: MqttSchema,
    pub(super) software_management_api: SoftwareManagementApiFlag,
    pub(super) command_id: IdGenerator,
    pub(super) smart_rest_use_operation_id: bool,
    pub(super) http_proxy: C8YHttpProxy,
    pub(super) downloader: ClientMessageBox<IdDownloadRequest, IdDownloadResult>,
    pub(super) uploader: ClientMessageBox<IdUploadRequest, IdUploadResult>,
    pub(super) mqtt_publisher: LoggingSender<MqttMessage>,
}

impl OperationContext {
    pub async fn update(&self, message: OperationMessage) {
        let OperationMessage {
            entity,
            cmd_id,
            message,
            operation,
        } = message;
        let external_id = entity.external_id.clone();

        let command = match GenericCommandState::from_command_message(&message) {
            Ok(command) => command,
            Err(err) => {
                error!(%err, ?message, "could not parse command payload");
                return;
            }
        };

        let mut c8y_operation = to_c8y_operation(&operation);

        let operation_result = match operation {
            OperationType::Health => {
                debug!(
                    topic = message.topic.name,
                    ?operation,
                    "ignoring local-only operation"
                );
                Ok(OperationOutcome::Ignored)
            }

            OperationType::Restart => {
                self.publish_restart_operation_status(&entity, &cmd_id, &message)
                    .await
            }
            // SoftwareList is not a regular operation: it doesn't update its status and doesn't report any
            // failures; it just maps local software list to c8y software list payloads and sends it via MQTT
            // Smartrest 2.0/HTTP
            OperationType::SoftwareList => {
                let result = self.publish_software_list(&entity, &cmd_id, &message).await;

                let mut mqtt_publisher = self.mqtt_publisher.clone();
                match result {
                    Err(err) => {
                        error!("Fail to list installed software packages: {err}");
                    }
                    Ok(OperationOutcome::Finished { messages }) => {
                        for message in messages {
                            mqtt_publisher.send(message).await.unwrap();
                        }
                    }
                    // command is not yet finished, avoid clearing the command topic
                    Ok(_) => return,
                }

                clear_command_topic(command, &mut mqtt_publisher).await;
                return;
            }
            OperationType::SoftwareUpdate => {
                self.publish_software_update_status(&entity, &cmd_id, &message)
                    .await
            }
            OperationType::LogUpload => {
                self.handle_log_upload_state_change(&entity, &cmd_id, &message)
                    .await
            }
            OperationType::ConfigSnapshot => {
                self.handle_config_snapshot_state_change(&entity, &cmd_id, &message)
                    .await
            }
            OperationType::ConfigUpdate => {
                self.handle_config_update_state_change(&entity, &cmd_id, &message)
                    .await
            }
            OperationType::FirmwareUpdate => {
                self.handle_firmware_update_state_change(&entity, &cmd_id, &message)
                    .await
            }
            OperationType::DeviceProfile => {
                self.handle_device_profile_state_change(&entity, &cmd_id, &message)
                    .await
            }
            OperationType::Custom(_) => {
                let (outcome, maybe_c8y_operation) = self
                    .handle_custom_operation_state_change(&entity, &cmd_id, &message)
                    .await;
                c8y_operation = maybe_c8y_operation;
                Ok(outcome)
            }
        };
        let mut mqtt_publisher = self.mqtt_publisher.clone();

        let c8y_operation = c8y_operation.unwrap_or(CumulocitySupportedOperations::C8yCustom(
            "unknown".to_string(),
        ));

        match self.to_response(
            operation_result,
            c8y_operation.clone(),
            &entity.smartrest_publish_topic,
            &cmd_id,
        ) {
            OperationOutcome::Ignored => {}
            OperationOutcome::Executing { mut extra_messages } => {
                let c8y_state_executing_payload = match self.get_operation_id(&cmd_id) {
                    Some(op_id) if self.smart_rest_use_operation_id => {
                        set_operation_executing_with_id(&op_id)
                    }
                    _ => set_operation_executing_with_name(c8y_operation),
                };

                let c8y_state_executing_message =
                    MqttMessage::new(&entity.smartrest_publish_topic, c8y_state_executing_payload);

                let mut messages = vec![c8y_state_executing_message];
                messages.append(&mut extra_messages);

                for message in messages {
                    mqtt_publisher.send(message).await.unwrap();
                }
            }
            OperationOutcome::Finished { messages } => {
                if let Err(e) = self
                    .upload_operation_log(&external_id, &cmd_id, &operation, &command)
                    .await
                {
                    error!("failed to upload operation logs: {e}");
                }

                for message in messages {
                    mqtt_publisher.send(message).await.unwrap();
                }

                clear_command_topic(command, &mut mqtt_publisher).await;
            }
        }
    }

    pub fn get_smartrest_successful_status_payload(
        &self,
        operation: CumulocitySupportedOperations,
        cmd_id: &str,
    ) -> SmartrestPayload {
        match self.get_operation_id(cmd_id) {
            Some(op_id) if self.smart_rest_use_operation_id => {
                succeed_operation_with_id_no_parameters(&op_id)
            }
            _ => succeed_operation_with_name_no_parameters(operation),
        }
    }

    pub fn get_smartrest_failed_status_payload(
        &self,
        operation: CumulocitySupportedOperations,
        reason: &str,
        cmd_id: &str,
    ) -> SmartrestPayload {
        match self.get_operation_id(cmd_id) {
            Some(op_id) if self.smart_rest_use_operation_id => {
                fail_operation_with_id(&op_id, reason)
            }
            _ => fail_operation_with_name(operation, reason),
        }
    }

    pub fn try_get_smartrest_successful_status_payload_with_args(
        &self,
        operation: CumulocitySupportedOperations,
        cmd_id: &str,
        text_or_csv: TextOrCsv,
    ) -> SmartrestPayload {
        let result = match self.get_operation_id(cmd_id) {
            Some(op_id) if self.smart_rest_use_operation_id => {
                succeed_operation_with_id(&op_id, text_or_csv)
            }
            _ => succeed_operation_with_name(operation.name(), text_or_csv),
        };

        match result {
            Ok(successful_message) => successful_message,
            Err(err) => {
                self.get_smartrest_failed_status_payload(operation, &err.to_string(), cmd_id)
            }
        }
    }

    /// Converts operation result to valid C8y response.
    fn to_response(
        &self,
        result: Result<OperationOutcome, OperationError>,
        operation_type: CumulocitySupportedOperations,
        smartrest_publish_topic: &Topic,
        cmd_id: &str,
    ) -> OperationOutcome {
        let err = match result {
            Ok(res) => {
                return res;
            }
            Err(err) => err,
        };

        // assuming `high level error: low level error: root cause error` error display impl
        let set_operation_to_failed_payload =
            self.get_smartrest_failed_status_payload(operation_type, &err.to_string(), cmd_id);

        let set_operation_to_failed_message =
            MqttMessage::new(smartrest_publish_topic, set_operation_to_failed_payload);

        let messages = vec![set_operation_to_failed_message];

        OperationOutcome::Finished { messages }
    }

    fn get_operation_id(&self, cmd_id: &str) -> Option<String> {
        self.command_id
            .get_value(cmd_id)
            .and_then(|s| s.parse::<u32>().ok()) // Ensure the operation ID is numeric
            .map(|s| s.to_string())
    }
}

async fn clear_command_topic(
    command: GenericCommandState,
    mqtt_publisher: &mut LoggingSender<MqttMessage>,
) {
    let command = command.clear();
    let clearing_message = command.into_message();
    assert!(clearing_message.payload_bytes().is_empty());
    assert!(clearing_message.retain);
    assert_eq!(clearing_message.qos, QoS::AtLeastOnce);
    mqtt_publisher.send(clearing_message).await.unwrap();
}

/// Result of an update of operation's state.
///
/// When a new MQTT message is received with an updated state of the operation, the mapper needs to
/// do something in response. Depending on if it cares about the operation, it can ignore it, send
/// some MQTT messages to notify C8y about the state change, or terminate the operation.
pub(super) enum OperationOutcome {
    /// Do nothing in response.
    ///
    /// Used for states that don't have an equivalent on C8y so we don't have to notify.
    Ignored,

    /// Update C8y operation state to `EXECUTING`.
    /// `extra_messages` can be used if an operation requires more than the status update message.
    Executing { extra_messages: Vec<MqttMessage> },

    /// Operation is terminated.
    ///
    /// Operation state is either `SUCCESSFUL` or `FAILED`. Report state to C8y, send operation log,
    /// clean local MQTT topic.
    Finished { messages: Vec<MqttMessage> },
}

/// For a given `OperationType`, obtain a matching `C8ySupportedOperations`.
///
/// For `OperationType`s that don't have C8y operation equivalent, `None` is returned.
fn to_c8y_operation(operation_type: &OperationType) -> Option<CumulocitySupportedOperations> {
    match operation_type {
        OperationType::LogUpload => Some(CumulocitySupportedOperations::C8yLogFileRequest),
        OperationType::Restart => Some(CumulocitySupportedOperations::C8yRestartRequest),
        OperationType::ConfigSnapshot => Some(CumulocitySupportedOperations::C8yUploadConfigFile),
        OperationType::ConfigUpdate => Some(CumulocitySupportedOperations::C8yDownloadConfigFile),
        OperationType::FirmwareUpdate => Some(CumulocitySupportedOperations::C8yFirmware),
        OperationType::SoftwareUpdate => Some(CumulocitySupportedOperations::C8ySoftwareUpdate),
        OperationType::DeviceProfile => Some(CumulocitySupportedOperations::C8yDeviceProfile),
        // Cannot convert custom operation name systematically
        OperationType::Custom(_) => None,
        // software list is not an c8y, only a fragment, but is a local operation that is spawned as
        // part of C8y_SoftwareUpdate operation
        OperationType::SoftwareList => None,
        // local-only operation, not always invoked by c8y, handled in other codepath
        OperationType::Health => None,
    }
}
/// An MQTT message that contains an operation payload.
///
/// These are MQTT messages that contain operation payloads. These messages need to be passed to
/// tasks that handle a given operation to advance the operation and eventually complete it.
#[derive(Debug, Clone)]
pub(super) struct OperationMessage {
    pub(super) operation: OperationType,
    pub(super) entity: EntityTarget,
    pub(super) cmd_id: Arc<str>,
    pub(super) message: MqttMessage,
}

/// A subset of entity-related information necessary to handle an operation.
///
/// Because the operation may take time and other operations may run concurrently, we don't want to
/// query the entity store.
#[derive(Clone, Debug)]
pub struct EntityTarget {
    pub topic_id: EntityTopicId,
    pub external_id: EntityExternalId,
    pub smartrest_publish_topic: Topic,
}

pub fn get_smartrest_response_for_upload_result(
    upload_result: tedge_uploader_ext::UploadResult,
    binary_url: &str,
    operation: c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations,
    use_operation_id: bool,
    op_id: Option<String>,
) -> SmartrestPayload {
    match upload_result {
        Ok(_) => match op_id {
            Some(op_id) if use_operation_id => {
                succeed_static_operation_with_id(&op_id, Some(binary_url))
            }
            _ => succeed_static_operation_with_name(operation, Some(binary_url)),
        },
        Err(err) => match op_id {
            Some(op_id) if use_operation_id => {
                fail_operation_with_id(&op_id, &format!("Upload failed with {err}"))
            }
            _ => fail_operation_with_name(operation, &format!("Upload failed with {err}")),
        },
    }
}
