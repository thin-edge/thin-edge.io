use crate::converter::CumulocityConverter;
use crate::error::ConversionError;
use crate::error::CumulocityMapperError;
use c8y_api::json_c8y_deserializer::C8yFirmware;
use c8y_api::smartrest::smartrest_serializer::fail_operation;
use c8y_api::smartrest::smartrest_serializer::set_operation_executing;
use c8y_api::smartrest::smartrest_serializer::succeed_operation_no_payload;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use tedge_api::entity_store::EntityExternalId;
use tedge_api::messages::FirmwareInfo;
use tedge_api::messages::FirmwareUpdateCmdPayload;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter::Command;
use tedge_api::mqtt_topics::ChannelFilter::CommandMetadata;
use tedge_api::mqtt_topics::EntityFilter::AnyEntity;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::CommandStatus;
use tedge_api::Jsonify;
use tedge_mqtt_ext::Message;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::TopicFilter;
use tracing::error;
use tracing::warn;

pub fn firmware_update_topic_filter(mqtt_schema: &MqttSchema) -> TopicFilter {
    [
        mqtt_schema.topics(AnyEntity, Command(OperationType::FirmwareUpdate)),
        mqtt_schema.topics(AnyEntity, CommandMetadata(OperationType::FirmwareUpdate)),
    ]
    .into_iter()
    .collect()
}

impl CumulocityConverter {
    /// Convert c8y_Firmware JSON over MQTT operation to ThinEdge firmware_update command.
    pub fn convert_firmware_update_request(
        &self,
        device_xid: String,
        cmd_id: String,
        firmware_request: C8yFirmware,
    ) -> Result<Vec<Message>, CumulocityMapperError> {
        let entity_xid: EntityExternalId = device_xid.into();

        let target = self.entity_store.try_get_by_external_id(&entity_xid)?;

        let channel = Channel::Command {
            operation: OperationType::FirmwareUpdate,
            cmd_id,
        };
        let topic = self.mqtt_schema.topic_for(&target.topic_id, &channel);

        let request = FirmwareUpdateCmdPayload {
            status: CommandStatus::Init,
            tedge_url: None,
            remote_url: firmware_request.url,
            name: firmware_request.name,
            version: firmware_request.version,
        };

        // Command messages must be retained
        Ok(vec![Message::new(&topic, request.to_json()).with_retain()])
    }

    /// Address a received ThinEdge firmware_update command. If its status is
    /// - "executing", it converts the message to SmartREST "Executing".
    /// - "successful", it converts the message to SmartREST "Successful" and update the current installed firmware.
    /// - "failed", it converts the message to SmartREST "Failed".
    pub async fn handle_firmware_update_state_change(
        &mut self,
        topic_id: &EntityTopicId,
        message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        if !self.config.capabilities.firmware_update {
            warn!(
                "Received a firmware_update command, however, firmware_update feature is disabled"
            );
            return Ok(vec![]);
        }

        let sm_topic = self.smartrest_publish_topic_for_entity(topic_id)?;
        let payload = message.payload_str()?;
        let response = &FirmwareUpdateCmdPayload::from_json(payload)?;

        let messages = match &response.status {
            CommandStatus::Executing => {
                let smartrest_operation_status =
                    set_operation_executing(CumulocitySupportedOperations::C8yFirmware);

                vec![Message::new(&sm_topic, smartrest_operation_status)]
            }
            CommandStatus::Successful => {
                let smartrest_operation_status =
                    succeed_operation_no_payload(CumulocitySupportedOperations::C8yFirmware);
                let c8y_notification = Message::new(&sm_topic, smartrest_operation_status);

                let clear_local_cmd = Message::new(&message.topic, "")
                    .with_retain()
                    .with_qos(QoS::AtLeastOnce);

                let twin_metadata_topic = self.mqtt_schema.topic_for(
                    topic_id,
                    &Channel::EntityTwinData {
                        fragment_key: "firmware".to_string(),
                    },
                );

                let twin_metadata_payload = FirmwareInfo {
                    name: Some(response.name.clone()),
                    version: Some(response.version.clone()),
                    remote_url: Some(response.remote_url.clone()),
                };

                let twin_metadata =
                    Message::new(&twin_metadata_topic, twin_metadata_payload.to_json())
                        .with_retain()
                        .with_qos(QoS::AtLeastOnce);

                vec![c8y_notification, clear_local_cmd, twin_metadata]
            }
            CommandStatus::Failed { reason } => {
                let smartrest_operation_status =
                    fail_operation(CumulocitySupportedOperations::C8yFirmware, reason);
                let c8y_notification = Message::new(&sm_topic, smartrest_operation_status);
                let clear_local_cmd = Message::new(&message.topic, "")
                    .with_retain()
                    .with_qos(QoS::AtLeastOnce);

                vec![c8y_notification, clear_local_cmd]
            }
            _ => {
                vec![] // Do nothing as other components might handle those states
            }
        };

        Ok(messages)
    }

    pub fn register_firmware_update_operation(
        &mut self,
        topic_id: &EntityTopicId,
    ) -> Result<Vec<Message>, ConversionError> {
        if !self.config.capabilities.firmware_update {
            warn!(
                "Received firmware_update metadata, however, firmware_update feature is disabled"
            );
            return Ok(vec![]);
        }

        match self.register_operation(topic_id, "c8y_Firmware") {
            Err(err) => {
                error!("Failed to register `c8y_Firmware` operation for {topic_id} due to: {err}");
                Ok(vec![])
            }
            Ok(messages) => Ok(messages),
        }
    }
}

#[cfg(test)]
mod tests {
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

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

    #[tokio::test]
    async fn create_firmware_operation_file_for_main_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
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
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
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
        let cfg_dir = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_Firmware operation delivered via JSON over MQTT
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".into()),
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
                    "remoteUrl": "http://www.my.url"
                }),
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn mapper_converts_firmware_op_to_firmware_update_cmd_for_child_device() {
        let cfg_dir = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
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
            &C8yDeviceControlTopic::topic(&"c8y".into()),
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
                    "remoteUrl": "http://www.my.url"
                }),
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_firmware_update_executing_and_failed_cmd_for_main_device() {
        let cfg_dir = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

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
        let cfg_dir = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

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
    async fn handle_firmware_update_successful_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
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
                ("te/device/main///cmd/firmware_update/c8y-mapper-1234", ""), // Clear cmd
                (
                    "te/device/main///twin/firmware",
                    r#"{"name":"myFirmware","version":"1.0","url":"http://www.my.url"}"#,
                ), // Twin firmware metadata
            ],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_firmware_update_successful_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
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
                ("te/device/child1///cmd/firmware_update/c8y-mapper-1234", ""), // Clear cmd
                (
                    "te/device/child1///twin/firmware",
                    r#"{"name":"myFirmware","version":"1.0","url":"http://www.my.url"}"#,
                ), // Twin firmware metadata
            ],
        )
        .await;
    }
}
