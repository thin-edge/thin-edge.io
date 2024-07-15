use crate::converter::CumulocityConverter;
use crate::error::ConversionError;
use crate::error::CumulocityMapperError;
use c8y_api::json_c8y_deserializer::C8yDownloadConfigFile;
use c8y_api::smartrest::smartrest_serializer::fail_operation;
use c8y_api::smartrest::smartrest_serializer::succeed_operation_no_payload;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use std::sync::Arc;
use tedge_api::commands::CommandStatus;
use tedge_api::commands::ConfigUpdateCmd;
use tedge_api::commands::ConfigUpdateCmdPayload;
use tedge_api::entity_store::EntityExternalId;
use tedge_api::entity_store::EntityMetadata;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::Jsonify;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tracing::log::warn;

use super::EntityTarget;
use super::OperationContext;
use super::OperationResult;

pub fn topic_filter(mqtt_schema: &MqttSchema) -> TopicFilter {
    [
        mqtt_schema.topics(
            EntityFilter::AnyEntity,
            ChannelFilter::Command(OperationType::ConfigUpdate),
        ),
        mqtt_schema.topics(
            EntityFilter::AnyEntity,
            ChannelFilter::CommandMetadata(OperationType::ConfigUpdate),
        ),
    ]
    .into_iter()
    .collect()
}

impl OperationContext {
    /// Address a received ThinEdge config_update command. If its status is
    /// - "executing", it converts the message to SmartREST "Executing".
    /// - "successful", it converts the message to SmartREST "Successful".
    /// - "failed", it converts the message to SmartREST "Failed".
    pub async fn handle_config_update_state_change(
        &self,
        target: &EntityTarget,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> Result<OperationResult, ConversionError> {
        if !self.capabilities.config_update {
            warn!("Received a config_update command, however, config_update feature is disabled");
            return Ok(OperationResult::Ignored);
        }

        let command = match ConfigUpdateCmd::try_from_bytes(
            target.topic_id.clone(),
            cmd_id.into(),
            message.payload_bytes(),
        )? {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok(OperationResult::Ignored);
            }
        };

        let sm_topic = &target.smartrest_publish_topic;

        match command.status() {
            CommandStatus::Executing => Ok(OperationResult::Executing),
            CommandStatus::Successful => {
                let smartrest_operation_status = succeed_operation_no_payload(
                    CumulocitySupportedOperations::C8yDownloadConfigFile,
                );
                let c8y_notification = MqttMessage::new(sm_topic, smartrest_operation_status);

                Ok(OperationResult::Finished {
                    messages: vec![c8y_notification],
                })
            }
            CommandStatus::Failed { reason } => {
                let smartrest_operation_status = fail_operation(
                    CumulocitySupportedOperations::C8yDownloadConfigFile,
                    &reason,
                );
                let c8y_notification = MqttMessage::new(sm_topic, smartrest_operation_status);

                Ok(OperationResult::Finished {
                    messages: vec![c8y_notification],
                })
            }
            _ => {
                Ok(OperationResult::Ignored) // Do nothing as other components might handle those states
            }
        }
    }
}

impl CumulocityConverter {
    /// Upon receiving a SmartREST c8y_DownloadConfigFile request, convert it to a message on the
    /// command channel.
    pub async fn convert_config_update_request(
        &self,
        device_xid: String,
        cmd_id: String,
        config_download_request: C8yDownloadConfigFile,
    ) -> Result<Vec<MqttMessage>, CumulocityMapperError> {
        let entity_xid: EntityExternalId = device_xid.into();
        let target = self.entity_store.try_get_by_external_id(&entity_xid)?;

        let message =
            self.create_config_update_cmd(cmd_id.into(), &config_download_request, target);
        Ok(message)
    }

    /// Converts a config_update metadata message to
    /// - supported operation "c8y_DownloadConfigFile"
    /// - supported config types
    pub fn convert_config_update_metadata(
        &mut self,
        topic_id: &EntityTopicId,
        message: &MqttMessage,
    ) -> Result<Vec<MqttMessage>, ConversionError> {
        if !self.config.capabilities.config_update {
            warn!("Received config_update metadata, however, config_update feature is disabled");
            return Ok(vec![]);
        }
        self.convert_config_metadata(topic_id, message, "c8y_DownloadConfigFile")
    }

    fn create_config_update_cmd(
        &self,
        cmd_id: Arc<str>,
        config_download_request: &C8yDownloadConfigFile,
        target: &EntityMetadata,
    ) -> Vec<MqttMessage> {
        let channel = Channel::Command {
            operation: OperationType::ConfigUpdate,
            cmd_id: cmd_id.to_string(),
        };
        let topic = self.mqtt_schema.topic_for(&target.topic_id, &channel);

        let proxy_url = self
            .c8y_endpoint
            .maybe_tenant_url(&config_download_request.url)
            .map(|cumulocity_url| self.auth_proxy.proxy_url(cumulocity_url).into());

        let remote_url = proxy_url.unwrap_or(config_download_request.url.to_string());

        let request = ConfigUpdateCmdPayload {
            status: CommandStatus::Init,
            tedge_url: None,
            remote_url,
            config_type: config_download_request.config_type.clone(),
            path: None,
            log_path: None,
        };

        // Command messages must be retained
        vec![MqttMessage::new(&topic, request.to_json()).with_retain()]
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::skip_init_messages;
    use crate::tests::spawn_c8y_mapper_actor;
    use crate::tests::TestHandle;
    use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
    use serde_json::json;
    use std::time::Duration;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::Sender;
    use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
    use tedge_mqtt_ext::test_helpers::assert_received_includes_json;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

    #[tokio::test]
    async fn mapper_converts_config_download_op_for_main_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_DownloadConfigFile operation delivered via JSON over MQTT
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "c8y_DownloadConfigFile": {
                    "type": "path/config/A",
                    "url": "http://www.my.url"
                },
                "externalSource": {
                    "externalId": "test-device",
                    "type": "c8y_Serial"
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/main///cmd/config_update/c8y-mapper-123456",
                json!({
                    "status": "init",
                    "remoteUrl": "http://www.my.url",
                    "type": "path/config/A",
                }),
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn mapper_converts_config_download_op_for_child_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // The child device must be registered first
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            r#"{ "@type":"child-device", "@id":"child1" }"#,
        ))
        .await
        .expect("fail to register the child-device");

        mqtt.skip(1).await; // Skip child device registration messages

        // Simulate c8y_DownloadConfigFile operation delivered via JSON over MQTT
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "c8y_DownloadConfigFile": {
                    "type": "configA",
                    "url": "http://www.my.url"
                },
                "externalSource": {
                    "externalId": "child1",
                    "type": "c8y_Serial"
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/child1///cmd/config_update/c8y-mapper-123456",
                json!({
                    "status": "init",
                    "remoteUrl": "http://www.my.url",
                    "type": "configA",
                }),
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_config_update_executing_and_failed_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate config_snapshot command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_update/c8y-mapper-1234"),
            json!({
            "status": "executing",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device/config_update/typeA-c8y-mapper-1234",
            "remoteUrl": "http://www.my.url",
            "type": "typeA",
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `501` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_DownloadConfigFile")]).await;

        // Simulate config_update command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_update/c8y-mapper-1234"),
            json!({
            "status": "failed",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device/config_update/typeA-c8y-mapper-1234",
            "remoteUrl": "http://www.my.url",
            "type": "typeA",
            "reason": "Something went wrong"
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `502` smartrest message on `c8y/s/us`.
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us",
                "502,c8y_DownloadConfigFile,Something went wrong",
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_config_update_executing_and_failed_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // The child device must be registered first
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            r#"{ "@type":"child-device", "@id":"child1" }"#,
        ))
        .await
        .expect("fail to register the child-device");

        mqtt.skip(1).await; // Skip child device registration messages

        // Simulate config_snapshot command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_update/c8y-mapper-1234"),
            json!({
            "status": "executing",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/child1/config_update/typeA-c8y-mapper-1234",
            "remoteUrl": "http://www.my.url",
            "type": "typeA",
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `501` smartrest message on child topic.
        assert_received_contains_str(
            &mut mqtt,
            [("c8y/s/us/child1", "501,c8y_DownloadConfigFile")],
        )
        .await;

        // Simulate config_update command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_update/c8y-mapper-1234"),
            json!({
            "status": "failed",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/child1/config_update/typeA-c8y-mapper-1234",
            "remoteUrl": "http://www.my.url",
            "type": "typeA",
            "reason": "Something went wrong"
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `502` smartrest message on child topic.
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/child1",
                "502,c8y_DownloadConfigFile,Something went wrong",
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_config_update_successful_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate config_update command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_update/c8y-mapper-1234"),
            json!({
            "status": "successful",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device/config_update/path:type:A-c8y-mapper-1234",
            "remoteUrl": "http://www.my.url",
            "type": "path/type/A",
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `503` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "503,c8y_DownloadConfigFile")]).await;
    }

    #[tokio::test]
    async fn handle_config_update_successful_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // The child device must be registered first
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            r#"{ "@type":"child-device", "@id":"child1" }"#,
        ))
        .await
        .expect("fail to register the child-device");

        mqtt.skip(1).await; // Skip child device registration messages

        // Simulate config_update command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/config_update/c8y-mapper-1234"),
            json!({
            "status": "successful",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/child1/config_update/typeA-c8y-mapper-1234",
            "remoteUrl": "http://www.my.url",
            "type": "typeA",
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Expect `503` smartrest message on child topic.
        assert_received_contains_str(
            &mut mqtt,
            [("c8y/s/us/child1", "503,c8y_DownloadConfigFile")],
        )
        .await;
    }
}
