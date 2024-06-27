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

use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::actor::IdUploadRequest;
use crate::actor::IdUploadResult;
use crate::converter::CumulocityConverter;
use crate::error::ConversionError;
use crate::Capabilities;
use c8y_api::http_proxy::C8yEndPoint;
use c8y_auth_proxy::url::ProxyUrlGenerator;
use c8y_http_proxy::handle::C8YHttpProxy;
use camino::Utf8Path;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
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
    pub capabilities: Capabilities,
    pub auto_log_upload: AutoLogUpload,
    pub tedge_http_host: Arc<str>,
    pub tmp_dir: Arc<Utf8Path>,
    pub mqtt_schema: MqttSchema,
    pub c8y_prefix: tedge_config::TopicPrefix,
    pub software_management_api: SoftwareManagementApiFlag,
    pub command_id: IdGenerator,

    pub http_proxy: C8YHttpProxy,
    pub c8y_endpoint: C8yEndPoint,
    pub auth_proxy: ProxyUrlGenerator,

    pub downloader: ClientMessageBox<IdDownloadRequest, IdDownloadResult>,
    pub uploader: ClientMessageBox<IdUploadRequest, IdUploadResult>,
    pub mqtt_publisher: LoggingSender<MqttMessage>,

    pub running_operations: Arc<Mutex<HashMap<Arc<str>, RunningOperation>>>,
}

impl OperationHandler {
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
    /// message will be published automatically, which does not need to be handled.
    pub async fn handle(self: &Arc<Self>, entity: EntityTarget, message: MqttMessage) {
        let Ok((_, channel)) = self.mqtt_schema.entity_channel_of(&message.topic) else {
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
        let running_operation = {
            let mut lock = self.running_operations.lock().unwrap();
            let op = lock.get(&topic);

            if let Some(running_operation) = op {
                // task already terminated
                if running_operation.tx.send(message).is_err() {
                    let running_operation = lock.remove(&topic).unwrap();
                    Some(running_operation)
                } else {
                    None
                }
            } else {
                let handler = self.clone();
                let running_operation = RunningOperation::spawn(message, handler);

                lock.insert(topic, running_operation);
                None
            }
        };

        if let Some(running_operation) = running_operation {
            let join_result = running_operation.handle.await;
            error!("handle: {join_result:?}");
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
    fn spawn(message: OperationMessage, handler: Arc<OperationHandler>) -> Self {
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
                        handler
                            .publish_restart_operation_status(entity, &cmd_id, message)
                            .await
                    }
                    OperationType::SoftwareList => {
                        handler
                            .publish_software_list(entity, &cmd_id, &message)
                            .await
                    }
                    OperationType::SoftwareUpdate => {
                        handler
                            .publish_software_update_status(entity, &cmd_id, &message)
                            .await
                    }
                    OperationType::LogUpload => {
                        handler
                            .handle_log_upload_state_change(entity, &cmd_id, &message)
                            .await
                    }
                    OperationType::ConfigSnapshot => {
                        handler
                            .handle_config_snapshot_state_change(entity, &cmd_id, &message)
                            .await
                    }
                    OperationType::ConfigUpdate => {
                        handler
                            .handle_config_update_state_change(entity, &cmd_id, &message)
                            .await
                    }
                    OperationType::FirmwareUpdate => {
                        handler
                            .handle_firmware_update_state_change(entity, &cmd_id, &message)
                            .await
                    }
                };

                let mut mqtt_publisher = handler.mqtt_publisher.clone();
                match res {
                    // If there are mapped final status messages to be published, they are cached until the operation
                    // log is uploaded
                    Ok((messages, command)) => {
                        if let Some(command) = command {
                            if let Err(e) = handler
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
    use crate::tests::skip_init_messages;
    use crate::tests::spawn_c8y_mapper_actor;
    use crate::tests::TestHandle;
    use std::time::Duration;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::Sender;
    use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

    #[tokio::test]
    async fn mapper_converts_config_metadata_to_supported_op_and_types_for_main_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate config_snapshot cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_snapshot"),
            r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Validate SmartREST message is published
        assert_received_contains_str(
            &mut mqtt,
            [
                ("c8y/s/us", "114,c8y_UploadConfigFile"),
                ("c8y/s/us", "119,typeA,typeB,typeC"),
            ],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/c8y_UploadConfigFile")
            .exists());

        // Simulate config_update cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_update"),
            r#"{"types" : [ "typeD", "typeE", "typeF" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Validate SmartREST message is published
        assert_received_contains_str(
            &mut mqtt,
            [
                (
                    "c8y/s/us",
                    "114,c8y_DownloadConfigFile,c8y_UploadConfigFile",
                ),
                ("c8y/s/us", "119,typeD,typeE,typeF"),
            ],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/c8y_DownloadConfigFile")
            .exists());
    }

    #[tokio::test]
    async fn mapper_converts_config_cmd_to_supported_op_and_types_for_child_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate config_snapshot cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_snapshot"),
            r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
        ))
        .await
        .expect("Send failed");

        mqtt.skip(2).await; // Skip the mapped child device registration message

        // Validate SmartREST message is published
        assert_received_contains_str(
            &mut mqtt,
            [
                (
                    "c8y/s/us/test-device:device:child1",
                    "114,c8y_UploadConfigFile",
                ),
                (
                    "c8y/s/us/test-device:device:child1",
                    "119,typeA,typeB,typeC",
                ),
            ],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/test-device:device:child1/c8y_UploadConfigFile")
            .exists());

        // Sending an updated list of config types
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_snapshot"),
            r#"{"types" : [ "typeB", "typeC", "typeD" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Assert that the updated config type list does not trigger a duplicate supported ops message
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/test-device:device:child1",
                "119,typeB,typeC,typeD",
            )],
        )
        .await;

        // Simulate config_update cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_update"),
            r#"{"types" : [ "typeD", "typeE", "typeF" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Validate SmartREST message is published
        assert_received_contains_str(
            &mut mqtt,
            [
                (
                    "c8y/s/us/test-device:device:child1",
                    "114,c8y_DownloadConfigFile,c8y_UploadConfigFile",
                ),
                (
                    "c8y/s/us/test-device:device:child1",
                    "119,typeD,typeE,typeF",
                ),
            ],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/test-device:device:child1/c8y_DownloadConfigFile")
            .exists());

        // Sending an updated list of config types
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_update"),
            r#"{"types" : [ "typeB", "typeC", "typeD" ]}"#,
        ))
        .await
        .expect("Send failed");

        // Assert that the updated config type list does not trigger a duplicate supported ops message
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/test-device:device:child1",
                "119,typeB,typeC,typeD",
            )],
        )
        .await;
    }
}
