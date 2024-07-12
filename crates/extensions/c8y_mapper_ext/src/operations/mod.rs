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
use c8y_auth_proxy::url::ProxyUrlGenerator;
use c8y_http_proxy::handle::C8YHttpProxy;
use camino::Utf8Path;
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
use tedge_api::Jsonify;
use tedge_config::AutoLogUpload;
use tedge_config::SoftwareManagementApiFlag;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tracing::error;

pub mod config_snapshot;
pub mod config_update;
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
                    continue;
                }

                let OperationMessage {
                    entity,
                    cmd_id,
                    message,
                    operation,
                } = message;
                let external_id = entity.external_id.clone();

                let command_topic = message.topic.clone();
                let res = match operation {
                    OperationType::Health | OperationType::Custom(_) => Ok((vec![], None)),

                    OperationType::Restart => {
                        context
                            .publish_restart_operation_status(entity, &cmd_id, message)
                            .await
                    }
                    OperationType::SoftwareList => {
                        context
                            .publish_software_list(entity, &cmd_id, &message)
                            .await
                    }
                    OperationType::SoftwareUpdate => {
                        context
                            .publish_software_update_status(entity, &cmd_id, &message)
                            .await
                    }
                    OperationType::LogUpload => {
                        context
                            .handle_log_upload_state_change(entity, &cmd_id, &message)
                            .await
                    }
                    OperationType::ConfigSnapshot => {
                        context
                            .handle_config_snapshot_state_change(entity, &cmd_id, &message)
                            .await
                    }
                    OperationType::ConfigUpdate => {
                        context
                            .handle_config_update_state_change(entity, &cmd_id, &message)
                            .await
                    }
                    OperationType::FirmwareUpdate => {
                        context
                            .handle_firmware_update_state_change(entity, &cmd_id, &message)
                            .await
                    }
                };

                let mut mqtt_publisher = context.mqtt_publisher.clone();
                match res {
                    // If there are mapped final status messages to be published, they are cached until the operation
                    // log is uploaded
                    Ok((messages, command)) => {
                        if let Some(command) = command {
                            if let Err(e) = context
                                .upload_operation_log(&external_id, &cmd_id, &operation, command)
                                .await
                            {
                                error!("failed to upload operation logs: {e}");
                            }
                        }

                        for message in messages {
                            // if task publishes MQTT clearing message, an operation is considered finished, so we can
                            // terminate the task as well
                            if message.retain
                                && message.payload_bytes().is_empty()
                                && message.topic == command_topic
                            {
                                rx.close();
                            }
                            mqtt_publisher.send(message).await.unwrap();
                        }
                    }
                    Err(e) => error!("{e}"),
                }
            }
        });

        Self { handle, tx }
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
