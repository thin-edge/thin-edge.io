use super::error::OperationError;
use super::EntityTarget;
use super::OperationContext;
use super::OperationOutcome;
use anyhow::Context;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use tedge_api::commands::FirmwareInfo;
use tedge_api::commands::FirmwareUpdateCmd;
use tedge_api::mqtt_topics::Channel;
use tedge_api::CommandStatus;
use tedge_api::Jsonify;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tracing::warn;

impl OperationContext {
    /// Address a received ThinEdge firmware_update command. If its status is
    /// - "executing", it converts the message to SmartREST "Executing".
    /// - "successful", it converts the message to SmartREST "Successful" and update the current installed firmware.
    /// - "failed", it converts the message to SmartREST "Failed".
    pub async fn handle_firmware_update_state_change(
        &self,
        target: &EntityTarget,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> Result<OperationOutcome, OperationError> {
        if !self.capabilities.firmware_update {
            warn!(
                "Received a firmware_update command, however, firmware_update feature is disabled"
            );
            return Ok(OperationOutcome::Ignored);
        }

        let command = match FirmwareUpdateCmd::try_from_bytes(
            target.topic_id.clone(),
            cmd_id.into(),
            message.payload_bytes(),
        )
        .context("Could not parse command as a firmware update command")?
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
                    CumulocitySupportedOperations::C8yFirmware,
                    cmd_id,
                );
                let c8y_notification = MqttMessage::new(sm_topic, smartrest_operation_status);

                let twin_metadata_topic = self.mqtt_schema.topic_for(
                    &target.topic_id,
                    &Channel::EntityTwinData {
                        fragment_key: "firmware".to_string(),
                    },
                );

                let twin_metadata_payload = FirmwareInfo {
                    name: Some(command.payload.name.clone()),
                    version: Some(command.payload.version.clone()),
                    remote_url: Some(command.payload.remote_url.clone()),
                };

                let twin_metadata =
                    MqttMessage::new(&twin_metadata_topic, twin_metadata_payload.to_json())
                        .with_retain()
                        .with_qos(QoS::AtLeastOnce);

                Ok(OperationOutcome::Finished {
                    messages: vec![c8y_notification, twin_metadata],
                })
            }
            CommandStatus::Failed { reason } => Err(anyhow::anyhow!(reason).into()),
            _ => Ok(OperationOutcome::Ignored),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::C8yMapperConfig;
    use crate::tests::*;
    use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
    use serde_json::json;
    use std::time::Duration;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::MessageReceiver;
    use tedge_actors::Sender;
    use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
    use tedge_mqtt_ext::test_helpers::assert_received_includes_json;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

    #[tokio::test]
    async fn create_firmware_operation_file_for_main_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate firmware_update cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/firmware_update"),
            "{}",
        ))
        .await
        .expect("Send failed");

        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "114,c8y_Firmware")]).await;

        // Validate if the supported operation file is created
        assert!(ttd.path().join("operations/c8y/c8y_Firmware").exists());
    }

    #[tokio::test]
    async fn create_firmware_operation_file_for_child_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate firmware_update cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/firmware_update"),
            "{}",
        ))
        .await
        .expect("Send failed");

        // Expect auto-registration message
        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/child1//",
                json!({"@type":"child-device","@id":"test-device:device:child1"}),
            )],
        )
        .await;

        assert_received_contains_str(
            &mut mqtt,
            [
                (
                    "c8y/s/us",
                    "101,test-device:device:child1,child1,thin-edge.io-child",
                ),
                ("c8y/s/us/test-device:device:child1", "114,c8y_Firmware"),
            ],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/test-device:device:child1/c8y_Firmware")
            .exists());

        // Duplicate firmware_update cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/firmware_update"),
            "{}",
        ))
        .await
        .expect("Send failed");

        // Assert that the supported ops message is not duplicated
        assert_eq!(mqtt.recv().await, None);
    }

    #[tokio::test]
    async fn mapper_converts_firmware_op_to_firmware_update_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;

        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_Firmware operation delivered via JSON over MQTT
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "c8y_Firmware": {
                    "name": "myFirmware",
                    "version": "1.0",
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
                "te/device/main///cmd/firmware_update/+",
                json!({
                    "status": "init",
                    "name": "myFirmware",
                    "version": "1.0",
                    "remoteUrl": "http://www.my.url",
                    "tedgeUrl": "http://www.my.url/"
                }),
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn mapper_converts_firmware_op_to_firmware_update_cmd_when_remote_utl_has_c8y_url() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;

        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_Firmware operation delivered via JSON over MQTT
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "c8y_Firmware": {
                    "name": "myFirmware",
                    "version": "1.0",
                    "url": "http://test.c8y.io/inventory/binaries/51541"
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
                "te/device/main///cmd/firmware_update/+",
                json!({
                    "status": "init",
                    "name": "myFirmware",
                    "version": "1.0",
                    "remoteUrl": "http://test.c8y.io/inventory/binaries/51541",
                    "tedgeUrl": "http://127.0.0.1:8001/c8y/inventory/binaries/51541"
                }),
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn mapper_converts_firmware_op_to_firmware_update_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;

        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate firmware_update cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///twin/firmware"),
            r#"{"name": "firmware", "version": "0.1"}"#,
        ))
        .await
        .expect("Send failed");

        mqtt.skip(3).await; // Skip entity registration, mapping and installed firmware messages

        // Simulate c8y_Firmware operation delivered via JSON over MQTT
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "c8y_Firmware": {
                    "name": "myFirmware",
                    "version": "1.0",
                    "url": "http://www.my.url"
                },
                "externalSource": {
                    "externalId": "test-device:device:child1",
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
                "te/device/child1///cmd/firmware_update/c8y-mapper-123456",
                json!({
                    "status": "init",
                    "name": "myFirmware",
                    "version": "1.0",
                    "remoteUrl": "http://www.my.url",
                    "tedgeUrl": "http://www.my.url/"
                }),
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_firmware_update_executing_and_failed_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;

        let TestHandle { mqtt, .. } = test_handle;

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
        skip_init_messages(&mut mqtt).await;

        // Simulate firmware_update command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/firmware_update/c8y-mapper-1234"),
            json!({
                "status": "executing",
                "name": "myFirmware",
                "version": "1.0",
                "remoteUrl": "http://www.my.url",
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `501` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_Firmware")]).await;

        // Simulate log_upload command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/firmware_update/c8y-mapper-1234"),
            json!({
                "status": "failed",
                "name": "myFirmware",
                "version": "1.0",
                "remoteUrl": "http://www.my.url",
                "reason": "Something went wrong"
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `502` smartrest message on `c8y/s/us`.
        assert_received_contains_str(
            &mut mqtt,
            [("c8y/s/us", "502,c8y_Firmware,Something went wrong")],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_firmware_update_executing_and_failed_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;

        let TestHandle { mqtt, .. } = test_handle;

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
        skip_init_messages(&mut mqtt).await;

        // Simulate log_upload command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/firmware_update/c8y-mapper-1234"),
            json!({
                "status": "executing",
                "name": "myFirmware",
                "version": "1.0",
                "remoteUrl": "http://www.my.url"
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        mqtt.skip(2).await; // Skip child device registration messages

        // Expect `501` smartrest message on `c8y/s/us/child1`.
        assert_received_contains_str(
            &mut mqtt,
            [("c8y/s/us/test-device:device:child1", "501,c8y_Firmware")],
        )
        .await;

        // Simulate log_upload command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/firmware_update/c8y-mapper-1234"),
            json!({
                "status": "failed",
                "name": "myFirmware",
                "version": "1.0",
                "remoteUrl": "http://www.my.url",
                "reason": "Something went wrong"
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `502` smartrest message on `c8y/s/us/child1`.
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us/test-device:device:child1",
                "502,c8y_Firmware,Something went wrong",
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_firmware_update_executing_and_failed_cmd_with_op_id() {
        let ttd = TempTedgeDir::new();
        let config = C8yMapperConfig {
            smartrest_use_operation_id: true,
            ..test_mapper_config(&ttd)
        };
        let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;

        let TestHandle { mqtt, .. } = test_handle;

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
        skip_init_messages(&mut mqtt).await;

        // Simulate firmware_update command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/firmware_update/c8y-mapper-1234"),
            json!({
                "status": "executing",
                "name": "myFirmware",
                "version": "1.0",
                "remoteUrl": "http://www.my.url",
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `504` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "504,1234")]).await;

        // Simulate log_upload command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/firmware_update/c8y-mapper-1234"),
            json!({
                "status": "failed",
                "name": "myFirmware",
                "version": "1.0",
                "remoteUrl": "http://www.my.url",
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
    async fn handle_firmware_update_successful_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, http, .. } = test_handle;
        spawn_dummy_c8y_http_proxy(http);

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
        skip_init_messages(&mut mqtt).await;

        // Simulate firmware_update command with "successful" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/firmware_update/c8y-mapper-1234"),
            json!({
            "status": "successful",
            "name": "myFirmware",
            "version": "1.0",
            "remoteUrl": "http://www.my.url",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device/firmware_update/myFirmware-c8y-mapper-1234",
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Assert MQTT messages
        assert_received_contains_str(
            &mut mqtt,
            [
                ("c8y/s/us", "503,c8y_Firmware"), // SmartREST successful
                (
                    "te/device/main///twin/firmware",
                    r#"{"name":"myFirmware","version":"1.0","url":"http://www.my.url"}"#,
                ), // Twin firmware metadata
                ("te/device/main///cmd/firmware_update/c8y-mapper-1234", ""), // Clear cmd
            ],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_firmware_update_successful_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, http, .. } = test_handle;
        spawn_dummy_c8y_http_proxy(http);

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
        skip_init_messages(&mut mqtt).await;

        // Simulate log_upload command with "successful" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/firmware_update/c8y-mapper-1234"),
            json!({
                "status": "successful",
                "name": "myFirmware",
                "version": "1.0",
                "remoteUrl": "http://www.my.url",
                "tedgeUrl": "http://localhost:8888/tedge/file-transfer/child1/firmware_update/myFirmware-c8y-mapper-1234",
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        mqtt.skip(2).await; // Skip child device registration messages

        // Assert MQTT messages
        assert_received_contains_str(
            &mut mqtt,
            [
                ("c8y/s/us/test-device:device:child1", "503,c8y_Firmware"), // SmartREST successful
                (
                    "te/device/child1///twin/firmware",
                    r#"{"name":"myFirmware","version":"1.0","url":"http://www.my.url"}"#,
                ), // Twin firmware metadata
                ("te/device/child1///cmd/firmware_update/c8y-mapper-1234", ""), // Clear cmd
            ],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_firmware_update_successful_cmd_with_op_id() {
        let ttd = TempTedgeDir::new();
        let config = C8yMapperConfig {
            smartrest_use_operation_id: true,
            ..test_mapper_config(&ttd)
        };
        let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
        let TestHandle { mqtt, http, .. } = test_handle;
        spawn_dummy_c8y_http_proxy(http);

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
        skip_init_messages(&mut mqtt).await;

        // Simulate firmware_update command with "successful" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/firmware_update/c8y-mapper-1234"),
            json!({
            "status": "successful",
            "name": "myFirmware",
            "version": "1.0",
            "remoteUrl": "http://www.my.url",
            "tedgeUrl": "http://localhost:8888/tedge/file-transfer/test-device/firmware_update/myFirmware-c8y-mapper-1234",
        })
                .to_string(),
        ))
            .await
            .expect("Send failed");

        // Assert MQTT messages
        assert_received_contains_str(
            &mut mqtt,
            [
                ("c8y/s/us", "506,1234"), // SmartREST successful
                (
                    "te/device/main///twin/firmware",
                    r#"{"name":"myFirmware","version":"1.0","url":"http://www.my.url"}"#,
                ), // Twin firmware metadata
                ("te/device/main///cmd/firmware_update/c8y-mapper-1234", ""), // Clear cmd
            ],
        )
        .await;
    }
}
