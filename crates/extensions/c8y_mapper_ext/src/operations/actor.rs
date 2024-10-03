//! Actor handles c8y operations.
//!
//! First, Cumulocity starts an operation like `c8y_SoftwareUpdate` or `c8y_UploadConfigFile`. This
//! is converted by the mapper into a local thin-edge.io command, that is executed by tedge-agent.
//! As the agent executes a command that corresponds to the operation we need to report on that
//! operation progress by sending smartrest messages like `Set operation to EXECUTING`.
//!
//! The handler ignores clearing messages that it receives, as it alone should send clearing
//! messages.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use super::handler::is_operation_status_transition_valid;
use super::handler::RunningOperation;
use super::handlers::OperationContext;
use super::handlers::OperationMessage;
use super::handlers::OperationOutcome;
use super::OperationHandler;
use crate::actor::PublishMessage;
use async_trait::async_trait;
use tedge_actors::Actor;
use tedge_actors::CloneSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_api::mqtt_topics::Channel;
use tedge_api::workflow::GenericCommandState;
use tedge_mqtt_ext::MqttMessage;
use tokio::sync::Mutex;
use tracing::debug;
use tracing::error;
use tracing::warn;

pub struct OperationHandlerActor {
    pub(super) messages: SimpleMessageBox<OperationMessage, PublishMessage>,
    pub(super) operation_handler: OperationHandler,
    pub(super) running_operations: RunningOperations,
}

#[async_trait]
impl Actor for OperationHandlerActor {
    fn name(&self) -> &str {
        "OperationHandler"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        while let Some(input_message) = self.messages.recv().await {
            self.handle_operation_message(input_message).await;
        }

        Ok(())
    }
}

impl OperationHandlerActor {
    async fn handle_operation_message(&mut self, message: OperationMessage) {
        let context = self.operation_handler.context.clone();

        // input validation
        let Ok((_, channel)) = context
            .mqtt_schema
            .entity_channel_of(&message.message.topic)
        else {
            return;
        };

        let Channel::Command { cmd_id, .. } = channel else {
            return;
        };

        // don't process sub-workflow calls
        if cmd_id.starts_with("sub:") {
            return;
        }

        if !context.command_id.is_generator_of(cmd_id.as_str()) {
            return;
        }

        let topic = message.message.topic.clone();

        let mut message_box = self.messages.sender_clone();
        self.running_operations
            .report(message, |outcome| async move {
                match outcome {
                    OperationOutcome::Ignored => {}
                    OperationOutcome::Executing { extra_messages } => {
                        for m in extra_messages {
                            message_box.send(PublishMessage(m)).await.unwrap();
                        }
                    }
                    OperationOutcome::Finished { messages } => {
                        for m in messages {
                            message_box.send(PublishMessage(m)).await.unwrap();
                        }

                        let clearing_message = MqttMessage::new(&topic, []).with_retain();
                        message_box
                            .send(PublishMessage(clearing_message))
                            .await
                            .unwrap();
                    }
                }
            })
            .await;
    }
}

pub(super) struct RunningOperations {
    pub(super) current_statuses: Arc<Mutex<HashMap<Arc<str>, RunningOperation>>>,
    pub(super) context: Arc<OperationContext>,
}

impl RunningOperations {
    // If operation status transition hasn't been handled yet, spawn a task that will handle it.
    async fn report<F, Fut>(&mut self, message: OperationMessage, f: F)
    where
        F: FnOnce(OperationOutcome) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send,
    {
        let topic = message.message.topic.name.as_str();
        let status = match GenericCommandState::from_command_message(&message.message) {
            // clearing message was either echoed back to us by MQTT broker, or was published by
            // some other MQTT client; the latter shouldn't really happen, but the former is
            // expected
            Ok(command) if command.is_cleared() => {
                debug!(topic = %topic, "unexpected clearing message");
                return;
            }
            Err(err) => {
                error!(%err, ?message, "could not parse command payload");
                return;
            }
            Ok(command) => command.status,
        };

        let context = self.context.clone();
        let mut current_statuses = self.current_statuses.lock().await;
        let current_operation = current_statuses.get(topic);

        match current_operation {
            None => {
                let topic: Arc<str> = topic.into();
                let handle = tokio::spawn(async move {
                    let outcome = context.report(message).await;
                    f(outcome).await;
                });
                current_statuses.insert(topic, RunningOperation { status, handle });
            }

            // if we have task running, check if new status is allowed and then spawn a new task
            // that also waits for old transition to complete
            Some(current_operation) => {
                let previous_status = &current_operation.status;
                if status == current_operation.status.as_str() {
                    debug!(
                        "already handling operation message with this topic and status, ignoring"
                    );
                    return;
                }

                // we got a new status, check if it's not invalid and then await previous one and
                // handle the new one
                if !is_operation_status_transition_valid(previous_status, &status) {
                    warn!(
                        topic = %topic,
                        previous = previous_status,
                        next = status,
                        "attempted invalid status transition, ignoring"
                    );
                    return;
                }

                // remove currently running operation task from the hashmap and spawn a new one that
                // also waits on the old one
                let topic: Arc<str> = topic.into();

                let _current_statuses = self.current_statuses.clone();
                let _topic = topic.clone();

                let handle = tokio::spawn(async move {
                    let outcome = context.report(message).await;
                    if let OperationOutcome::Finished { .. } = outcome {
                        _current_statuses.lock().await.remove(&*_topic);
                    }
                    f(outcome).await;
                });

                current_statuses.insert(topic, RunningOperation { handle, status });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::actor::IdDownloadRequest;
    use crate::actor::IdDownloadResult;
    use crate::actor::IdUploadRequest;
    use crate::actor::IdUploadResult;
    use crate::actor::PublishMessage;
    use crate::operations::builder::OperationHandlerBuilder;
    use crate::operations::handler::OperationHandlerConfig;
    use crate::Capabilities;
    use c8y_api::http_proxy::C8yEndPoint;
    use c8y_auth_proxy::url::Protocol;
    use c8y_auth_proxy::url::ProxyUrlGenerator;
    use c8y_http_proxy::messages::C8YRestRequest;
    use c8y_http_proxy::messages::C8YRestResult;
    use tedge_actors::test_helpers::FakeServerBox;
    use tedge_actors::test_helpers::FakeServerBoxBuilder;
    use tedge_actors::Actor;
    use tedge_actors::Builder;
    use tedge_actors::Sender;
    use tedge_actors::SimpleMessageBox;
    use tedge_actors::SimpleMessageBoxBuilder;
    use tedge_api::commands::ConfigSnapshotCmd;
    use tedge_api::commands::ConfigSnapshotCmdPayload;
    use tedge_api::entity_store::EntityMetadata;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::mqtt_topics::IdGenerator;
    use tedge_api::mqtt_topics::MqttSchema;
    use tedge_api::CommandStatus;
    use tedge_config::AutoLogUpload;
    use tedge_config::TopicPrefix;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_test_utils::fs::TempTedgeDir;
    use tokio::task::JoinHandle;
    use tracing::Level;

    #[tokio::test]
    // #[should_panic]
    async fn panics_when_task_panics() {
        tedge_config::system_services::set_log_level(Level::DEBUG);
        let TestHandle {
            mut mqtt,
            handle: actor_handle,
            ..
        } = spawn_operation_actor().await;

        let mqtt_schema = MqttSchema::new();
        let entity_topic_id = EntityTopicId::default_main_device();
        let entity_metadata = EntityMetadata::main_device("anything".to_string());

        // spawn an operation to see if it's successfully joined when it's completed.
        // particular operation used is not important, because we want to test only the handler.
        // it would be even better if we could define some inline operation so test could be shorter
        // TODO(marcel): don't assume operation implementations when testing the handler
        let command = ConfigSnapshotCmd {
            target: entity_topic_id,
            cmd_id: "c8y-mapper-1".to_string(),
            payload: ConfigSnapshotCmdPayload {
                status: CommandStatus::Successful,
                tedge_url: Some("asdf".to_string()),
                config_type: "typeA".to_string(),
                path: None,
                log_path: None,
            },
        };
        let message = command.command_message(&mqtt_schema);

        mqtt.send((message, entity_metadata)).await.unwrap();
        drop(mqtt);

        actor_handle.await.unwrap();
    }

    struct TestHandle {
        handle: JoinHandle<()>,
        mqtt: SimpleMessageBox<PublishMessage, (MqttMessage, EntityMetadata)>,
        _dl: FakeServerBox<IdDownloadRequest, IdDownloadResult>,
        _ul: FakeServerBox<IdUploadRequest, IdUploadResult>,
        _c8y_proxy: FakeServerBox<C8YRestRequest, C8YRestResult>,
        _ttd: TempTedgeDir,
    }

    async fn spawn_operation_actor() -> TestHandle {
        let auth_proxy_addr = "127.0.0.1".into();
        let auth_proxy_port = 8001;
        let auth_proxy_protocol = Protocol::Http;

        let ttd = TempTedgeDir::new();
        let config = OperationHandlerConfig {
            capabilities: Capabilities::default(),
            auto_log_upload: AutoLogUpload::OnFailure,
            tedge_http_host: Arc::from("127.0.0.1:8000"),
            tmp_dir: ttd.utf8_path().into(),
            software_management_api: tedge_config::SoftwareManagementApiFlag::Legacy,
            mqtt_schema: MqttSchema::with_root("te".to_string()),
            c8y_endpoint: C8yEndPoint::new("c8y.url", "c8y.url", "device_id"),
            c8y_prefix: TopicPrefix::try_from("c8y").unwrap(),
            auth_proxy: ProxyUrlGenerator::new(
                auth_proxy_addr,
                auth_proxy_port,
                auth_proxy_protocol,
            ),
            id_generator: IdGenerator::new("c8y"),
            smartrest_use_operation_id: true,
        };

        let mut mqtt_builder: SimpleMessageBoxBuilder<
            PublishMessage,
            (MqttMessage, EntityMetadata),
        > = SimpleMessageBoxBuilder::new("MQTT", 10);
        let mut c8y_proxy_builder: FakeServerBoxBuilder<C8YRestRequest, C8YRestResult> =
            FakeServerBoxBuilder::default();
        let mut uploader_builder: FakeServerBoxBuilder<IdUploadRequest, IdUploadResult> =
            FakeServerBoxBuilder::default();
        let mut downloader_builder: FakeServerBoxBuilder<IdDownloadRequest, IdDownloadResult> =
            FakeServerBoxBuilder::default();

        let operation_handler_builder = OperationHandlerBuilder::new(
            config,
            &mut mqtt_builder,
            &mut uploader_builder,
            &mut downloader_builder,
            &mut c8y_proxy_builder,
        );

        let actor = operation_handler_builder.build();
        let handle = tokio::spawn(async move { actor.run().await.unwrap() });

        TestHandle {
            handle,
            mqtt: mqtt_builder.build(),
            _dl: downloader_builder.build(),
            _ul: uploader_builder.build(),
            _c8y_proxy: c8y_proxy_builder.build(),
            _ttd: ttd,
        }
    }
}
