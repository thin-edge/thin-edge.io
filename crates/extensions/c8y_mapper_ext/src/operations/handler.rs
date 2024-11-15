use super::handlers::EntityTarget;
use super::handlers::OperationContext;
use super::handlers::OperationMessage;
use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::actor::IdUploadRequest;
use crate::actor::IdUploadResult;
use crate::config::C8yMapperConfig;
use crate::Capabilities;
use c8y_api::http_proxy::C8yEndPoint;
use c8y_api::proxy_url::ProxyUrlGenerator;
use c8y_http_proxy::handle::C8YHttpProxy;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;
use tedge_actors::ClientMessageBox;
use tedge_actors::LoggingSender;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_api::workflow::GenericCommandState;
use tedge_mqtt_ext::MqttMessage;
use tracing::debug;
use tracing::error;
use tracing::warn;

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
                smart_rest_use_operation_id: c8y_mapper_config.smartrest_use_operation_id,

                // TODO(marcel): would be good not to generate new ids from running operations, see if
                // we can remove it somehow
                command_id: c8y_mapper_config.id_generator(),

                downloader,
                uploader,

                c8y_endpoint: C8yEndPoint::new(
                    &c8y_mapper_config.c8y_host,
                    &c8y_mapper_config.c8y_mqtt,
                    &c8y_mapper_config.device_id,
                    auth_proxy.clone(),
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
    ///
    /// # Panics
    ///
    /// Will panic if a task that runs the operation has panicked. The task can panic if e.g. MQTT
    /// send returns an error or the task encountered any other unexpected error that makes it
    /// impossible to finish handling the operation (i.e. send MQTT clearing message and report
    /// operation status to c8y).
    ///
    /// The panic in the operation task has to happen first, and then another message with the same
    /// command id has to be handled for the call to `.handle()` to panic.
    //
    // but there's a problem: in practice, when a panic in a child task happens, .handle() will
    // never get called for that operation again. Operation task itself sends the messages, so if
    // they can't be sent over MQTT because of a panic, they won't be handled, won't be joined, so
    // we will not see that an exception has occurred.
    // FIXME(marcel): ensure panics are always propagated without the caller having to ask for them
    pub async fn handle(&mut self, entity: EntityTarget, message: MqttMessage) {
        let Ok((_, channel)) = self.context.mqtt_schema.entity_channel_of(&message.topic) else {
            return;
        };

        let Channel::Command { operation, cmd_id } = channel else {
            return;
        };

        // don't process sub-workflow calls
        if cmd_id.starts_with("sub:") {
            return;
        }

        if !self.context.command_id.is_generator_of(&cmd_id) {
            return;
        }

        let message = OperationMessage {
            operation,
            entity,
            cmd_id: cmd_id.into(),
            message,
        };

        let topic = Arc::from(message.message.topic.name.as_str());

        let status = match GenericCommandState::from_command_message(&message.message) {
            Ok(command) if command.is_cleared() => None,
            Ok(command) => Some(command.status),
            Err(err) => {
                error!(%err, ?message, "could not parse command payload");
                return;
            }
        };

        let current_operation = self.running_operations.entry(topic);

        match current_operation {
            Entry::Vacant(entry) => {
                let Some(status) = status else {
                    debug!(topic = %entry.key(), "unexpected clearing message");
                    return;
                };

                let context = Arc::clone(&self.context);
                let handle = tokio::spawn(async move { context.update(message).await });

                let running_operation = RunningOperation { handle, status };

                entry.insert(running_operation);
            }

            Entry::Occupied(entry) => {
                let previous_status = entry.get().status.as_str();
                if status.as_ref().is_some_and(|s| *s == previous_status) {
                    debug!(
                        "already handling operation message with this topic and status, ignoring"
                    );
                    return;
                }

                // if handling a clearing message, wait for a task to finish
                let Some(status) = status else {
                    let operation = entry.remove();
                    operation
                        .handle
                        .await
                        .expect("operation task should not panic");
                    return;
                };

                // we got a new status, check if it's not invalid and then await previous one and
                // handle the new one
                if !is_operation_status_transition_valid(previous_status, &status) {
                    warn!(
                        topic = %entry.key(),
                        previous = previous_status,
                        next = status,
                        "attempted invalid status transition, ignoring"
                    );
                    return;
                }

                let (key, operation) = entry.remove_entry();
                let context = Arc::clone(&self.context);
                let handle = tokio::spawn(async move {
                    operation.handle.await.unwrap();
                    context.update(message).await;
                });
                let running_operation = RunningOperation { handle, status };
                self.running_operations.insert(key, running_operation);
            }
        }
    }

    /// A topic filter for operation types this object can handle.
    ///
    /// The MQTT client should subscribe to topics with this filter to receive MQTT messages that it
    /// should then pass to the [`Self::handle`] method. Depending on the tedge configuration, some
    /// operations may be disabled and therefore absent in the filter.
    pub fn topic_filter(capabilities: &Capabilities) -> Vec<(EntityFilter, ChannelFilter)> {
        use tedge_api::mqtt_topics::ChannelFilter::Command;
        use tedge_api::mqtt_topics::ChannelFilter::CommandMetadata;
        use tedge_api::mqtt_topics::EntityFilter::AnyEntity;
        use tedge_api::mqtt_topics::OperationType;

        let mut topics = vec![];

        if capabilities.log_upload {
            topics.extend([
                (AnyEntity, Command(OperationType::LogUpload)),
                (AnyEntity, CommandMetadata(OperationType::LogUpload)),
            ]);
        }
        if capabilities.config_snapshot {
            topics.extend([
                (AnyEntity, Command(OperationType::ConfigSnapshot)),
                (AnyEntity, CommandMetadata(OperationType::ConfigSnapshot)),
            ]);
        }
        if capabilities.config_update {
            topics.extend([
                (AnyEntity, Command(OperationType::ConfigUpdate)),
                (AnyEntity, CommandMetadata(OperationType::ConfigUpdate)),
            ]);
        }
        if capabilities.firmware_update {
            topics.extend([
                (AnyEntity, Command(OperationType::FirmwareUpdate)),
                (AnyEntity, CommandMetadata(OperationType::FirmwareUpdate)),
            ]);
        }

        if capabilities.device_profile {
            topics.extend([
                (AnyEntity, Command(OperationType::DeviceProfile)),
                (AnyEntity, CommandMetadata(OperationType::DeviceProfile)),
            ]);
        }

        topics
    }
}
struct RunningOperation {
    handle: tokio::task::JoinHandle<()>,
    status: String,
}

// TODO: logic of which status transitions are valid should be defined in tedge_api and be
// considered together with custom statuses of custom workflows
fn is_operation_status_transition_valid(previous: &str, next: &str) -> bool {
    #[allow(clippy::match_like_matches_macro)]
    match (previous, next) {
        // not really a transition but false to make sure we're not sending multiple smartrest msgs
        (prev, next) if prev == next => false,

        // successful and failed are terminal, can't change them
        ("successful", _) => false,
        ("failed", _) => false,

        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::time::Duration;

    use c8y_api::proxy_url::Protocol;
    use c8y_http_proxy::C8YHttpConfig;
    use tedge_actors::test_helpers::FakeServerBox;
    use tedge_actors::test_helpers::FakeServerBoxBuilder;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::Builder;
    use tedge_actors::MessageReceiver;
    use tedge_actors::MessageSink;
    use tedge_actors::Sender;
    use tedge_actors::SimpleMessageBox;
    use tedge_actors::SimpleMessageBoxBuilder;
    use tedge_api::commands::ConfigSnapshotCmd;
    use tedge_api::commands::ConfigSnapshotCmdPayload;
    use tedge_api::entity_store::EntityExternalId;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::mqtt_topics::OperationType;
    use tedge_api::CommandStatus;
    use tedge_downloader_ext::DownloadResponse;
    use tedge_http_ext::HttpRequest;
    use tedge_http_ext::HttpResult;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;
    use tedge_uploader_ext::UploadResponse;

    use crate::tests::spawn_dummy_c8y_http_proxy;
    use crate::tests::test_mapper_config;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

    #[tokio::test]
    async fn handle_ignores_messages_that_are_not_operations() {
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

    #[tokio::test]
    async fn handle_ignores_topic_from_different_mapper_instance() {
        let test_handle = setup_operation_handler();
        let mut sut = test_handle.operation_handler;

        let mqtt_schema = sut.context.mqtt_schema.clone();

        let entity_topic_id = EntityTopicId::default_main_device();
        let entity_target = EntityTarget {
            topic_id: entity_topic_id.clone(),
            external_id: EntityExternalId::from("anything"),
            smartrest_publish_topic: Topic::new("anything").unwrap(),
        };

        // Using a firmware operation here, but should hold for any operation type
        let different_mapper_topic = mqtt_schema.topic_for(
            &entity_topic_id,
            &Channel::Command {
                operation: OperationType::Restart,
                cmd_id: "different-prefix-mapper-1923738".to_string(),
            },
        );
        let different_mapper_message =
            MqttMessage::new(&different_mapper_topic, r#"{"status":"executing"}"#);

        sut.handle(entity_target.clone(), different_mapper_message)
            .await;

        assert_eq!(sut.running_operations.len(), 0);
    }

    #[tokio::test]
    async fn handle_ignores_subcommand_topics_3048() {
        let test_handle = setup_operation_handler();
        let mut sut = test_handle.operation_handler;
        let mqtt = test_handle.mqtt;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        let mqtt_schema = sut.context.mqtt_schema.clone();

        let entity_topic_id = EntityTopicId::default_main_device();
        let entity_target = EntityTarget {
            topic_id: entity_topic_id.clone(),
            external_id: EntityExternalId::from("anything"),
            smartrest_publish_topic: Topic::new("anything").unwrap(),
        };

        // Using a firmware operation here, but should hold for any operation type
        let sub_workflow_topic = mqtt_schema.topic_for(
            &entity_topic_id,
            &Channel::Command {
                operation: OperationType::Restart,
                cmd_id: "sub:firmware_update:c8y-mapper-192481".to_string(),
            },
        );
        let sub_workflow_message =
            MqttMessage::new(&sub_workflow_topic, r#"{"status":"executing"}"#);

        sut.handle(entity_target.clone(), sub_workflow_message)
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

        assert_eq!(
            sut.running_operations.len(),
            0,
            "task shouldn't be spawned for sub-workflow"
        );
        assert_eq!(mqtt.recv().await, None);
    }

    #[tokio::test]
    async fn handle_joins_terminated_operations() {
        let TestHandle {
            operation_handler: mut sut,
            downloader: dl,
            uploader: ul,
            mqtt,
            c8y_proxy,
            ttd: _ttd,
            ..
        } = setup_operation_handler();

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
        let mut dl = dl.with_timeout(TEST_TIMEOUT_MS);
        let mut ul = ul.with_timeout(TEST_TIMEOUT_MS);
        spawn_dummy_c8y_http_proxy(c8y_proxy);

        let mqtt_schema = sut.context.mqtt_schema.clone();

        let entity_topic_id = EntityTopicId::default_main_device();
        let entity_target = EntityTarget {
            topic_id: entity_topic_id.clone(),
            external_id: EntityExternalId::from("anything"),
            smartrest_publish_topic: Topic::new("anything").unwrap(),
        };

        // spawn an operation to see if it's successfully joined when it's completed.
        // particular operation used is not important, because we want to test only the handler.
        // it would be even better if we could define some inline operation so test could be shorter
        // TODO(marcel): don't assume operation implementations when testing the handler
        let config_snapshot_operation = ConfigSnapshotCmd {
            target: entity_topic_id,
            cmd_id: "c8y-mapper-1273384".to_string(),
            payload: ConfigSnapshotCmdPayload {
                status: CommandStatus::Successful,
                tedge_url: Some("asdf".to_string()),
                config_type: "typeA".to_string(),
                path: None,
                log_path: None,
            },
        };

        sut.handle(
            entity_target.clone(),
            config_snapshot_operation.command_message(&mqtt_schema),
        )
        .await;
        assert_eq!(sut.running_operations.len(), 1);

        dl.recv()
            .await
            .expect("downloader should receive DownloadRequest");

        dl.send((
            "config-snapshot-1".to_string(),
            Ok(DownloadResponse {
                url: "asdf".to_string(),
                file_path: "asdf".into(),
            }),
        ))
        .await
        .unwrap();

        ul.recv()
            .await
            .expect("uploader should receive UploadRequest");

        ul.send((
            "config-snapshot-1".to_string(),
            Ok(UploadResponse {
                url: "asdf".to_string(),
                file_path: "asdf".into(),
            }),
        ))
        .await
        .unwrap();

        assert_eq!(sut.running_operations.len(), 1);

        // skip 503 smartrest
        mqtt.skip(1).await;

        let clearing_message = mqtt.recv().await.expect("MQTT should receive message");
        assert_eq!(
            clearing_message,
            config_snapshot_operation.clearing_message(&mqtt_schema)
        );

        assert_eq!(sut.running_operations.len(), 1);

        // finally, check that after handling clearing message, operation was joined
        sut.handle(entity_target, clearing_message).await;

        assert_eq!(sut.running_operations.len(), 0);
    }

    #[tokio::test]
    async fn ignores_malformed_command_payloads() {
        let TestHandle {
            operation_handler: mut sut,
            ttd: _ttd,
            ..
        } = setup_operation_handler();

        let mqtt_schema = sut.context.mqtt_schema.clone();

        let entity_topic_id = EntityTopicId::default_main_device();
        let entity_target = EntityTarget {
            topic_id: entity_topic_id.clone(),
            external_id: EntityExternalId::from("anything"),
            smartrest_publish_topic: Topic::new("anything").unwrap(),
        };

        let command_topic = mqtt_schema.topic_for(
            &entity_topic_id,
            &Channel::Command {
                operation: OperationType::ConfigSnapshot,
                cmd_id: "config-snapshot-1".to_string(),
            },
        );

        let invalid_command_message = MqttMessage::new(&command_topic, "invalid command payload");

        sut.handle(entity_target, invalid_command_message).await;

        assert!(!sut
            .running_operations
            .contains_key(command_topic.name.as_str()));
    }

    #[tokio::test]
    async fn ignores_unexpected_clearing_messages() {
        let TestHandle {
            operation_handler: mut sut,
            ttd: _ttd,
            ..
        } = setup_operation_handler();

        let mqtt_schema = sut.context.mqtt_schema.clone();

        let entity_topic_id = EntityTopicId::default_main_device();
        let entity_target = EntityTarget {
            topic_id: entity_topic_id.clone(),
            external_id: EntityExternalId::from("anything"),
            smartrest_publish_topic: Topic::new("anything").unwrap(),
        };

        let config_snapshot_operation = ConfigSnapshotCmd {
            target: entity_topic_id,
            cmd_id: "c8y-mapper-229394".to_string(),
            payload: ConfigSnapshotCmdPayload {
                status: CommandStatus::Executing,
                tedge_url: Some("asdf".to_string()),
                config_type: "typeA".to_string(),
                path: None,
                log_path: None,
            },
        };
        let clearing_message = config_snapshot_operation.clearing_message(&mqtt_schema);
        let clearing_message_topic = clearing_message.topic.name.clone();

        sut.handle(entity_target, clearing_message).await;

        assert!(!sut
            .running_operations
            .contains_key(clearing_message_topic.as_str()));
    }

    #[tokio::test]
    async fn shouldnt_process_duplicate_messages() {
        let TestHandle {
            operation_handler: mut sut,
            mqtt,
            ttd: _ttd,
            ..
        } = setup_operation_handler();

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        let mqtt_schema = sut.context.mqtt_schema.clone();

        let entity_topic_id = EntityTopicId::default_main_device();
        let entity_target = EntityTarget {
            topic_id: entity_topic_id.clone(),
            external_id: EntityExternalId::from("anything"),
            smartrest_publish_topic: Topic::new("anything").unwrap(),
        };

        let config_snapshot_operation = ConfigSnapshotCmd {
            target: entity_topic_id,
            cmd_id: "c8y-mapper-123456".to_string(),
            payload: ConfigSnapshotCmdPayload {
                status: CommandStatus::Executing,
                tedge_url: Some("asdf".to_string()),
                config_type: "typeA".to_string(),
                path: None,
                log_path: None,
            },
        };

        // check that if the same message is handled 3 times by mistake, we don't call process it multiple times
        for _ in 0..3 {
            sut.handle(
                entity_target.clone(),
                config_snapshot_operation.command_message(&mqtt_schema),
            )
            .await;
        }

        let smartrest_executing_message = mqtt.recv().await.unwrap();
        assert_eq!(
            smartrest_executing_message.payload_str().unwrap(),
            "501,c8y_UploadConfigFile"
        );

        assert_eq!(
            mqtt.recv().await,
            None,
            "shouldn't receive duplicates of EXECUTING message"
        )
    }

    #[tokio::test]
    async fn shouldnt_process_invalid_status_transitions() {
        let TestHandle {
            operation_handler: mut sut,
            ttd: _ttd,
            ..
        } = setup_operation_handler();

        let mqtt_schema = sut.context.mqtt_schema.clone();

        let entity_topic_id = EntityTopicId::default_main_device();
        let entity_target = EntityTarget {
            topic_id: entity_topic_id.clone(),
            external_id: EntityExternalId::from("anything"),
            smartrest_publish_topic: Topic::new("anything").unwrap(),
        };

        let failed_message = ConfigSnapshotCmd {
            target: entity_topic_id.clone(),
            cmd_id: "c8y-mapper-284842".to_string(),
            payload: ConfigSnapshotCmdPayload {
                status: CommandStatus::Failed {
                    reason: "test".to_string(),
                },
                tedge_url: Some("asdf".to_string()),
                config_type: "typeA".to_string(),
                path: None,
                log_path: None,
            },
        };

        let successful_message = ConfigSnapshotCmd {
            target: entity_topic_id,
            cmd_id: "c8y-mapper-28433842".to_string(),
            payload: ConfigSnapshotCmdPayload {
                status: CommandStatus::Successful,
                tedge_url: Some("asdf".to_string()),
                config_type: "typeA".to_string(),
                path: None,
                log_path: None,
            },
        };

        let failed_message_mqtt = failed_message.command_message(&mqtt_schema);
        let failed_topic = failed_message_mqtt.topic.name.as_str();
        sut.handle(entity_target.clone(), failed_message_mqtt.clone())
            .await;
        assert_eq!(
            &sut.running_operations.get(failed_topic).unwrap().status,
            "failed"
        );

        let successful_message_mqtt = successful_message.command_message(&mqtt_schema);
        let successful_topic = successful_message_mqtt.topic.name.as_str();
        sut.handle(entity_target.clone(), successful_message_mqtt.clone())
            .await;
        assert_eq!(
            &sut.running_operations
                .get(successful_message_mqtt.topic.name.as_str())
                .unwrap()
                .status,
            "successful"
        );

        // status shouldn't change from successful/failed to executing
        let executing_message = failed_message.with_status(CommandStatus::Executing);
        sut.handle(
            entity_target.clone(),
            executing_message.command_message(&mqtt_schema),
        )
        .await;
        assert_eq!(
            &sut.running_operations
                .get(dbg!(failed_topic))
                .unwrap()
                .status,
            "failed"
        );

        let executing_message = successful_message.with_status(CommandStatus::Executing);
        sut.handle(
            entity_target.clone(),
            executing_message.command_message(&mqtt_schema),
        )
        .await;
        assert_eq!(
            &sut.running_operations.get(successful_topic).unwrap().status,
            "successful"
        );
    }

    #[tokio::test]
    #[should_panic]
    async fn handle_should_panic_when_background_task_panics() {
        // we're immediately dropping test's temporary directory, so we'll get an error that a
        // directory for the operation could not be created
        let TestHandle {
            operation_handler: mut sut,
            ..
        } = setup_operation_handler();

        let mqtt_schema = sut.context.mqtt_schema.clone();

        let entity_topic_id = EntityTopicId::default_main_device();
        let entity_target = EntityTarget {
            topic_id: entity_topic_id.clone(),
            external_id: EntityExternalId::from("anything"),
            smartrest_publish_topic: Topic::new("anything").unwrap(),
        };

        // spawn an operation to see if it's successfully joined when it's completed.
        // particular operation used is not important, because we want to test only the handler.
        // it would be even better if we could define some inline operation so test could be shorter
        // TODO(marcel): don't assume operation implementations when testing the handler
        let config_snapshot_operation = ConfigSnapshotCmd {
            target: entity_topic_id,
            cmd_id: "config-snapshot-1".to_string(),
            payload: ConfigSnapshotCmdPayload {
                status: CommandStatus::Successful,
                tedge_url: Some("asdf".to_string()),
                config_type: "typeA".to_string(),
                path: None,
                log_path: None,
            },
        };

        sut.handle(
            entity_target.clone(),
            config_snapshot_operation.command_message(&mqtt_schema),
        )
        .await;
        assert_eq!(sut.running_operations.len(), 1);

        // give OperationHandler time to handle message
        // TODO(marcel): remove sleeps
        tokio::time::sleep(Duration::from_millis(50)).await;

        // normally clearing message would be sent by operation task.
        // Using it here just as a dummy, to call `handle` with the same cmd-id, so that it panics
        sut.handle(
            entity_target.clone(),
            config_snapshot_operation.clearing_message(&mqtt_schema),
        )
        .await;
    }

    fn setup_operation_handler() -> TestHandle {
        let ttd = TempTedgeDir::new();
        let c8y_mapper_config = test_mapper_config(&ttd);

        let mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
            SimpleMessageBoxBuilder::new("MQTT", 10);
        let mqtt_publisher = LoggingSender::new("MQTT".to_string(), mqtt_builder.get_sender());

        let mut http_builder: FakeServerBoxBuilder<HttpRequest, HttpResult> =
            FakeServerBoxBuilder::default();
        let auth_proxy = ProxyUrlGenerator::default();
        let http_config = C8YHttpConfig::new(
            c8y_mapper_config.device_id.clone(),
            c8y_mapper_config.c8y_host.clone(),
            c8y_mapper_config.c8y_mqtt.clone(),
            auth_proxy.clone(),
        );
        let c8y_proxy = C8YHttpProxy::new(http_config, &mut http_builder);

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

        let mqtt = mqtt_builder.build();
        let downloader = downloader_builder.build();
        let uploader = uploader_builder.build();
        let http = http_builder.build();

        TestHandle {
            mqtt,
            downloader,
            uploader,
            c8y_proxy: http,
            operation_handler,
            ttd,
        }
    }

    struct TestHandle {
        operation_handler: OperationHandler,
        mqtt: SimpleMessageBox<MqttMessage, MqttMessage>,
        c8y_proxy: FakeServerBox<HttpRequest, HttpResult>,
        uploader: FakeServerBox<IdUploadRequest, IdUploadResult>,
        downloader: FakeServerBox<IdDownloadRequest, IdDownloadResult>,
        ttd: TempTedgeDir,
    }
}
