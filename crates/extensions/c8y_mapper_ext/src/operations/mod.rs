//! Utilities for executing Cumulocity operations.
//!
//! C8y operations need some special handling by the C8y mapper, which needs to use the C8y HTTP
//! proxy to report on their progress. Additionally, while executing operations we often need to
//! send messages to different actors and wait for their results before continuing.
//!
//! The operations are always triggered remotely by Cumulocity, and a triggered operation must
//! always terminate in a success or failure. This status needs to be reported to Cumulocity.
//!
//! This module contains:
//! - data definitions of various states which are necessary to maintain in the mapper
//! - status and error handing utilities for reporting operation success/failure in different ways
//!   (MQTT, Smartrest)
//! - implementations of operations
//!
//! thin-edge.io operations reference: https://thin-edge.github.io/thin-edge.io/operate/c8y/supported-operations/

use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::actor::IdUploadRequest;
use crate::actor::IdUploadResult;
use crate::config::C8yMapperConfig;
use crate::converter::CumulocityConverter;
use crate::error::ConversionError;
use crate::Capabilities;
use c8y_api::http_proxy::C8yEndPoint;
use c8y_api::smartrest::smartrest_serializer::fail_operation;
use c8y_api::smartrest::smartrest_serializer::set_operation_executing;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_auth_proxy::url::ProxyUrlGenerator;
use c8y_http_proxy::handle::C8YHttpProxy;
use camino::Utf8Path;
use error::OperationError;
use std::collections::HashMap;
use std::sync::Arc;
use tedge_actors::ClientMessageBox;
use tedge_actors::LoggingSender;
use tedge_actors::Sender;
use tedge_api::commands::ConfigMetadata;
use tedge_api::entity_store::EntityExternalId;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::IdGenerator;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::GenericCommandState;
use tedge_api::Jsonify;
use tedge_config::AutoLogUpload;
use tedge_config::SoftwareManagementApiFlag;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::Topic;
use tracing::debug;
use tracing::error;

pub mod config_snapshot;
pub mod config_update;
mod error;
pub mod firmware_update;
pub mod log_upload;
mod restart;
mod software_list;
mod software_update;
mod upload;

/// Handles operations.
///
/// Handling an operation usually consists of 3 steps:
///
/// 1. Receive a smartrest message which is an operation request, convert it to thin-edge message,
///    and publish on local MQTT (done by the converter).
/// 2. Various local thin-edge components/services (e.g. tedge-agent) execute the operation, and
///    when they're done, they publish an MQTT message with 'status: successful/failed'
/// 3. The cumulocity mapper needs to do some additional steps, like downloading/uploading files via
///    HTTP, or talking to C8y via HTTP proxy, before it can send operation response via the bridge
///    and then clear the local MQTT operation topic.
///
/// This struct concerns itself with performing step 3.
///
/// Incoming operation-related MQTT messages need to be passed to the [`Self::handle`] method, which
/// performs an operation in the background in separate tasks. The operation tasks themselves handle
/// reporting their success/failure.
pub struct OperationHandler {
    context: Arc<OperationContext>,
    running_operations: HashMap<Arc<str>, RunningOperation>,
}

impl OperationHandler {
    pub fn new(
        c8y_mapper_config: &C8yMapperConfig,

        downloader: ClientMessageBox<IdDownloadRequest, IdDownloadResult>,
        uploader: ClientMessageBox<IdUploadRequest, IdUploadResult>,
        mqtt_publisher: LoggingSender<MqttMessage>,

        http_proxy: C8YHttpProxy,
        auth_proxy: ProxyUrlGenerator,
    ) -> Self {
        Self {
            context: Arc::new(OperationContext {
                capabilities: c8y_mapper_config.capabilities,
                auto_log_upload: c8y_mapper_config.auto_log_upload,
                tedge_http_host: c8y_mapper_config.tedge_http_host.clone(),
                tmp_dir: c8y_mapper_config.tmp_dir.clone(),
                mqtt_schema: c8y_mapper_config.mqtt_schema.clone(),
                mqtt_publisher: mqtt_publisher.clone(),
                software_management_api: c8y_mapper_config.software_management_api,

                // TODO(marcel): would be good not to generate new ids from running operations, see if
                // we can remove it somehow
                command_id: IdGenerator::new(crate::converter::REQUESTER_NAME),

                downloader,
                uploader,

                c8y_endpoint: C8yEndPoint::new(
                    &c8y_mapper_config.c8y_host,
                    &c8y_mapper_config.c8y_mqtt,
                    &c8y_mapper_config.device_id,
                ),
                http_proxy: http_proxy.clone(),
                auth_proxy: auth_proxy.clone(),
            }),

            running_operations: Default::default(),
        }
    }

    /// Handles an MQTT command id message.
    ///
    /// All MQTT messages with a topic that contains an operation id, e.g.
    /// `te/device/child001///cmd/software_list/c8y-2023-09-25T14:34:00` need to be passed here for
    /// operations to be processed. Messages not related to operations are ignored.
    ///
    /// `entity` needs to be the same entity as in `message`.
    ///
    /// When a message with a new id is handled, a task will be spawned that processes this
    /// operation. For the operation to be completed, subsequent messages with the same command id
    /// need to be handled as well.
    ///
    /// When an operation terminates (successfully or unsuccessfully), an MQTT operation clearing
    /// message will be published to the broker by the running operation task, but this message also
    /// needs to be handled when an MQTT broker echoes it back to us, so that `OperationHandler` can
    /// free the data associated with the operation.
    pub async fn handle(&mut self, entity: EntityTarget, message: MqttMessage) {
        let Ok((_, channel)) = self.context.mqtt_schema.entity_channel_of(&message.topic) else {
            return;
        };

        let Channel::Command { operation, cmd_id } = channel else {
            return;
        };

        let message = OperationMessage {
            operation,
            entity,
            cmd_id: cmd_id.into(),
            message,
        };

        let topic: Arc<str> = message.message.topic.name.clone().into();
        let terminated_operation = {
            let op = self.running_operations.get(&topic);

            if let Some(running_operation) = op {
                // task already terminated
                if running_operation.tx.send(message).is_err() {
                    let running_operation = self.running_operations.remove(&topic).unwrap();
                    Some(running_operation)
                } else {
                    None
                }
            } else {
                let running_operation = RunningOperation::spawn(message, Arc::clone(&self.context));

                self.running_operations
                    .insert(topic.clone(), running_operation);
                None
            }
        };

        if let Some(terminated_operation) = terminated_operation {
            let join_result = terminated_operation.handle.await;
            if let Err(err) = join_result {
                error!(%topic, ?err, "operation task could not be joined");
            }
        }
    }
}

pub struct RunningOperation {
    handle: tokio::task::JoinHandle<()>,
    tx: tokio::sync::mpsc::UnboundedSender<OperationMessage>,
}

impl RunningOperation {
    /// Spawns a task that handles the operation.
    ///
    /// The task handles a single operation with a given command id, and via a channel it receives
    /// operation state changes (if any) to drive an operation to completion.
    fn spawn(message: OperationMessage, context: Arc<OperationContext>) -> Self {
        let cmd_id = message.cmd_id.clone();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        tx.send(message).unwrap();

        let handle = tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                if message.cmd_id != cmd_id {
                    debug!(
                        msg_cmd_id = %message.cmd_id,
                        %cmd_id, "operation-related message was routed incorrectly"
                    );
                    continue;
                }

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

                let operation_result = match operation {
                    OperationType::Health | OperationType::Custom(_) => {
                        debug!(
                            topic = message.topic.name,
                            ?operation,
                            "ignoring local-only operation"
                        );
                        Ok(OperationOutcome::Ignored)
                    }

                    OperationType::Restart => {
                        context
                            .publish_restart_operation_status(&entity, &cmd_id, &message)
                            .await
                    }
                    // SoftwareList is not a regular operation: it doesn't update its status and doesn't report any
                    // failures; it just maps local software list to c8y software list payloads and sends it via MQTT
                    // Smartrest 2.0/HTTP
                    OperationType::SoftwareList => {
                        let result = context
                            .publish_software_list(&entity, &cmd_id, &message)
                            .await;

                        let mut mqtt_publisher = context.mqtt_publisher.clone();
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
                            Ok(_) => {
                                continue;
                            }
                        }

                        clear_command_topic(command, &mut mqtt_publisher).await;
                        rx.close();
                        continue;
                    }
                    OperationType::SoftwareUpdate => {
                        context
                            .publish_software_update_status(&entity, &cmd_id, &message)
                            .await
                    }
                    OperationType::LogUpload => {
                        context
                            .handle_log_upload_state_change(&entity, &cmd_id, &message)
                            .await
                    }
                    OperationType::ConfigSnapshot => {
                        context
                            .handle_config_snapshot_state_change(&entity, &cmd_id, &message)
                            .await
                    }
                    OperationType::ConfigUpdate => {
                        context
                            .handle_config_update_state_change(&entity, &cmd_id, &message)
                            .await
                    }
                    OperationType::FirmwareUpdate => {
                        context
                            .handle_firmware_update_state_change(&entity, &cmd_id, &message)
                            .await
                    }
                };

                let mut mqtt_publisher = context.mqtt_publisher.clone();

                // unwrap is safe: at this point all local operations that are not regular c8y
                // operations should be handled above
                let c8y_operation = to_c8y_operation(&operation).unwrap();

                match to_response(
                    operation_result,
                    c8y_operation,
                    &entity.smartrest_publish_topic,
                ) {
                    OperationOutcome::Ignored => {}
                    OperationOutcome::Executing => {
                        let c8y_state_executing_payload = set_operation_executing(c8y_operation);
                        let c8y_state_executing_message = MqttMessage::new(
                            &entity.smartrest_publish_topic,
                            c8y_state_executing_payload,
                        );
                        mqtt_publisher
                            .send(c8y_state_executing_message)
                            .await
                            .unwrap();
                    }
                    OperationOutcome::Finished { messages } => {
                        if let Err(e) = context
                            .upload_operation_log(&external_id, &cmd_id, &operation, &command)
                            .await
                        {
                            error!("failed to upload operation logs: {e}");
                        }

                        for message in messages {
                            mqtt_publisher.send(message).await.unwrap();
                        }

                        clear_command_topic(command, &mut mqtt_publisher).await;

                        rx.close();
                    }
                }
            }
        });

        Self { handle, tx }
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
enum OperationOutcome {
    /// Do nothing in response.
    ///
    /// Used for states that don't have an equivalent on C8y so we don't have to notify.
    Ignored,

    /// Update C8y operation state to `EXECUTING`.
    Executing,

    /// Operation is terminated.
    ///
    /// Operation state is either `SUCCESSFUL` or `FAILED`. Report state to C8y, send operation log,
    /// clean local MQTT topic.
    Finished { messages: Vec<MqttMessage> },
}

/// Converts operation result to valid C8y response.
fn to_response(
    result: Result<OperationOutcome, OperationError>,
    operation_type: CumulocitySupportedOperations,
    smartrest_publish_topic: &Topic,
) -> OperationOutcome {
    let err = match result {
        Ok(res) => {
            return res;
        }
        Err(err) => err,
    };

    // assuming `high level error: low level error: root cause error` error display impl
    let set_operation_to_failed_payload = fail_operation(operation_type, &err.to_string());

    let set_operation_to_failed_message =
        MqttMessage::new(smartrest_publish_topic, set_operation_to_failed_payload);

    let messages = vec![set_operation_to_failed_message];

    OperationOutcome::Finished { messages }
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
        // software list is not an c8y, only a fragment, but is a local operation that is spawned as
        // part of C8y_SoftwareUpdate operation
        OperationType::SoftwareList => None,
        // local-only operation, not always invoked by c8y, handled in other codepath
        OperationType::Health => None,
        // other custom operations, no c8y equivalent
        OperationType::Custom(_) => None,
    }
}

/// State required by the operation handlers.
struct OperationContext {
    capabilities: Capabilities,
    auto_log_upload: AutoLogUpload,
    tedge_http_host: Arc<str>,
    tmp_dir: Arc<Utf8Path>,
    mqtt_schema: MqttSchema,
    software_management_api: SoftwareManagementApiFlag,
    command_id: IdGenerator,

    http_proxy: C8YHttpProxy,
    c8y_endpoint: C8yEndPoint,
    auth_proxy: ProxyUrlGenerator,

    downloader: ClientMessageBox<IdDownloadRequest, IdDownloadResult>,
    uploader: ClientMessageBox<IdUploadRequest, IdUploadResult>,
    mqtt_publisher: LoggingSender<MqttMessage>,
}

/// An MQTT message that contains an operation payload.
///
/// These are MQTT messages that contain operation payloads. These messages need to be passed to
/// tasks that handle a given operation to advance the operation and eventually complete it.
struct OperationMessage {
    operation: OperationType,
    entity: EntityTarget,
    cmd_id: Arc<str>,
    message: MqttMessage,
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

impl CumulocityConverter {
    fn convert_config_metadata(
        &mut self,
        topic_id: &EntityTopicId,
        message: &MqttMessage,
        c8y_op_name: &str,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        let metadata = ConfigMetadata::from_json(message.payload_str()?)?;

        let mut messages = match self.register_operation(topic_id, c8y_op_name) {
            Err(err) => {
                error!("Failed to register {c8y_op_name} operation for {topic_id} due to: {err}");
                return Ok(vec![]);
            }
            Ok(messages) => messages,
        };

        // To SmartREST supported config types
        let mut types = metadata.types;
        types.sort();
        let supported_config_types = types.join(",");
        let payload = format!("119,{supported_config_types}");
        let sm_topic = self.smartrest_publish_topic_for_entity(topic_id)?;
        messages.push(MqttMessage::new(&sm_topic, payload));

        Ok(messages)
    }
}

fn get_smartrest_response_for_upload_result(
    upload_result: tedge_uploader_ext::UploadResult,
    binary_url: &str,
    operation: c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations,
) -> c8y_api::smartrest::smartrest_serializer::SmartRest {
    match upload_result {
        Ok(_) => c8y_api::smartrest::smartrest_serializer::succeed_static_operation(
            operation,
            Some(binary_url),
        ),
        Err(err) => c8y_api::smartrest::smartrest_serializer::fail_operation(
            operation,
            &format!("Upload failed with {err}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use c8y_auth_proxy::url::Protocol;
    use c8y_http_proxy::messages::C8YRestRequest;
    use c8y_http_proxy::messages::C8YRestResult;
    use tedge_actors::test_helpers::FakeServerBox;
    use tedge_actors::test_helpers::FakeServerBoxBuilder;
    use tedge_actors::Builder;
    use tedge_actors::MessageSink;
    use tedge_actors::SimpleMessageBox;
    use tedge_actors::SimpleMessageBoxBuilder;
    use tedge_test_utils::fs::TempTedgeDir;

    use crate::tests::test_mapper_config;

    use super::*;

    #[tokio::test]
    async fn ignores_messages_that_are_not_operations() {
        // system under test
        let mut sut = setup_operation_handler().operation_handler;
        let mqtt_schema = sut.context.mqtt_schema.clone();

        let entity_topic_id = EntityTopicId::default_main_device();
        let entity_target = EntityTarget {
            topic_id: entity_topic_id.clone(),
            external_id: EntityExternalId::from("anything"),
            smartrest_publish_topic: Topic::new("anything").unwrap(),
        };

        let message_wrong_entity = MqttMessage::new(&Topic::new("asdf").unwrap(), []);
        sut.handle(entity_target.clone(), message_wrong_entity)
            .await;

        assert_eq!(sut.running_operations.len(), 0);

        let topic = mqtt_schema.topic_for(
            &entity_topic_id,
            &Channel::CommandMetadata {
                operation: OperationType::Restart,
            },
        );
        let message_wrong_channel = MqttMessage::new(&topic, []);
        sut.handle(entity_target, message_wrong_channel).await;

        assert_eq!(sut.running_operations.len(), 0);
    }

    fn setup_operation_handler() -> TestHandle {
        let ttd = TempTedgeDir::new();
        let c8y_mapper_config = test_mapper_config(&ttd);

        let mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
            SimpleMessageBoxBuilder::new("MQTT", 10);
        let mqtt_publisher = LoggingSender::new("MQTT".to_string(), mqtt_builder.get_sender());

        let mut c8y_proxy_builder: FakeServerBoxBuilder<C8YRestRequest, C8YRestResult> =
            FakeServerBoxBuilder::default();
        let c8y_proxy = C8YHttpProxy::new(&mut c8y_proxy_builder);

        let mut uploader_builder: FakeServerBoxBuilder<IdUploadRequest, IdUploadResult> =
            FakeServerBoxBuilder::default();
        let uploader = ClientMessageBox::new(&mut uploader_builder);

        let mut downloader_builder: FakeServerBoxBuilder<IdDownloadRequest, IdDownloadResult> =
            FakeServerBoxBuilder::default();
        let downloader = ClientMessageBox::new(&mut downloader_builder);

        let auth_proxy_addr = c8y_mapper_config.auth_proxy_addr.clone();
        let auth_proxy_port = c8y_mapper_config.auth_proxy_port;
        let auth_proxy = ProxyUrlGenerator::new(auth_proxy_addr, auth_proxy_port, Protocol::Http);

        let operation_handler = OperationHandler::new(
            &c8y_mapper_config,
            downloader,
            uploader,
            mqtt_publisher,
            c8y_proxy,
            auth_proxy,
        );

        let _mqtt = mqtt_builder.build();
        let _downloader = downloader_builder.build();
        let _uploader = uploader_builder.build();
        let _c8y_proxy = c8y_proxy_builder.build();

        TestHandle {
            _mqtt,
            _downloader,
            _uploader,
            _c8y_proxy,
            operation_handler,
        }
    }

    struct TestHandle {
        operation_handler: OperationHandler,
        _mqtt: SimpleMessageBox<MqttMessage, MqttMessage>,
        _c8y_proxy: FakeServerBox<C8YRestRequest, C8YRestResult>,
        _uploader: FakeServerBox<IdUploadRequest, IdUploadResult>,
        _downloader: FakeServerBox<IdDownloadRequest, IdDownloadResult>,
    }
}
