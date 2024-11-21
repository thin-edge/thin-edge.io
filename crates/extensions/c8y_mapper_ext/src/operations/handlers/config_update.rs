use anyhow::Context;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use tedge_api::commands::CommandStatus;
use tedge_api::commands::ConfigUpdateCmd;
use tedge_mqtt_ext::MqttMessage;
use tracing::log::warn;

use super::error::OperationError;
use super::EntityTarget;
use super::OperationContext;
use super::OperationOutcome;

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
    ) -> Result<OperationOutcome, OperationError> {
        if !self.capabilities.config_update {
            warn!("Received a config_update command, however, config_update feature is disabled");
            return Ok(OperationOutcome::Ignored);
        }

        let command = match ConfigUpdateCmd::try_from_bytes(
            target.topic_id.clone(),
            cmd_id.into(),
            message.payload_bytes(),
        )
        .context("Could not parse command as a config update command")?
        {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok(OperationOutcome::Ignored);
            }
        };

        let sm_topic = &target.smartrest_publish_topic;

        match command.status() {
            CommandStatus::Executing => Ok(OperationOutcome::Executing {
                extra_messages: vec![],
            }),
            CommandStatus::Successful => {
                let smartrest_operation_status = self.get_smartrest_successful_status_payload(
                    CumulocitySupportedOperations::C8yDownloadConfigFile,
                    cmd_id,
                );
                let c8y_notification = MqttMessage::new(sm_topic, smartrest_operation_status);

                Ok(OperationOutcome::Finished {
                    messages: vec![c8y_notification],
                })
            }
            CommandStatus::Failed { reason } => Err(anyhow::anyhow!(reason).into()),
            _ => {
                Ok(OperationOutcome::Ignored) // Do nothing as other components might handle those states
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::C8yMapperConfig;
    use crate::tests::skip_init_messages;
    use crate::tests::spawn_c8y_mapper_actor;
    use crate::tests::spawn_c8y_mapper_actor_with_config;
    use crate::tests::test_mapper_config;
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
                    "remoteUrl": "http://www.my.url/",
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
                    "remoteUrl": "http://www.my.url/",
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
    async fn handle_config_update_executing_and_failed_cmd_with_id() {
        let ttd = TempTedgeDir::new();
        let config = C8yMapperConfig {
            smartrest_use_operation_id: true,
            ..test_mapper_config(&ttd)
        };
        let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
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

        // Expect `504` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "504,1234")]).await;

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

        // Expect `505` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "505,1234,Something went wrong")])
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

    #[tokio::test]
    async fn handle_config_update_successful_cmd_with_id() {
        let ttd = TempTedgeDir::new();
        let config = C8yMapperConfig {
            smartrest_use_operation_id: true,
            ..test_mapper_config(&ttd)
        };
        let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
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
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "506,1234")]).await;
    }
}
