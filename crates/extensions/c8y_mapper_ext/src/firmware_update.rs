use crate::converter::CumulocityConverter;
use crate::error::ConversionError;
use crate::error::CumulocityMapperError;
use c8y_api::smartrest::smartrest_deserializer::SmartRestFirmwareRequest;
use c8y_api::smartrest::smartrest_deserializer::SmartRestRequestGeneric;
use c8y_api::smartrest::smartrest_serializer::fail_operation;
use c8y_api::smartrest::smartrest_serializer::set_operation_executing;
use c8y_api::smartrest::smartrest_serializer::succeed_operation_no_payload;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use tedge_api::entity_store::EntityType;
use tedge_api::messages::FirmwareMetadata;
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
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::TopicFilter;
use tedge_utils::file::create_directory_with_defaults;
use tedge_utils::file::create_file_with_defaults;
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
    /// Convert c8y_Firmware SmartREST request to ThinEdge firmware_update command.
    /// Command ID is generated here, but it should be replaced by c8y's operation ID in the future.
    pub fn convert_firmware_update_request(
        &self,
        smartrest: &str,
    ) -> Result<Vec<Message>, CumulocityMapperError> {
        let firmware_request = SmartRestFirmwareRequest::from_smartrest(smartrest)?;

        let target = self
            .entity_store
            .try_get_by_external_id(&firmware_request.device.clone().into())?;

        let cmd_id = self.command_id.new_id();
        let channel = Channel::Command {
            operation: OperationType::FirmwareUpdate,
            cmd_id: cmd_id.clone(),
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

                let metadata_topic = self.mqtt_schema.topic_for(
                    topic_id,
                    &Channel::CommandMetadata {
                        operation: OperationType::FirmwareUpdate,
                    },
                );
                let metadata_payload = FirmwareMetadata {
                    name: Some(response.name.clone()),
                    version: Some(response.version.clone()),
                    remote_url: Some(response.remote_url.clone()),
                };
                let update_metadata = Message::new(&metadata_topic, metadata_payload.to_json())
                    .with_retain()
                    .with_qos(QoS::AtLeastOnce);

                vec![c8y_notification, clear_local_cmd, update_metadata]
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

    /// Converts a firmware_update metadata message to
    /// - supported operation "c8y_Firmware"
    /// - current installed firmware
    pub fn convert_firmware_metadata(
        &mut self,
        topic_id: &EntityTopicId,
        message: &Message,
    ) -> Result<Vec<Message>, ConversionError> {
        if !self.config.capabilities.firmware_update {
            warn!(
                "Received firmware_update metadata, however, firmware_update feature is disabled"
            );
            return Ok(vec![]);
        }

        let metadata = FirmwareMetadata::from_json(message.payload_str()?)?;

        // Get the device metadata from its id
        let target = self.entity_store.try_get(topic_id)?;

        // Create a c8y_Firmware operation file
        let dir_path = match target.r#type {
            EntityType::MainDevice => self.ops_dir.clone(),
            EntityType::ChildDevice => {
                let child_dir_name = target.external_id.as_ref();
                self.ops_dir.clone().join(child_dir_name)
            }
            EntityType::Service => {
                warn!("firmware_update feature is not supported for services");
                return Ok(vec![]);
            }
        };
        create_directory_with_defaults(&dir_path)?;
        create_file_with_defaults(dir_path.join("c8y_Firmware"), None)?;

        // To SmartREST current installed firmware message
        let c8y_topic = self.smartrest_publish_topic_for_entity(topic_id)?;
        let payload = format!(
            "115,{name},{version},{url}",
            name = metadata.name.unwrap_or("".into()),
            version = metadata.version.unwrap_or("".into()),
            url = metadata.remote_url.unwrap_or("".into()),
        );
        let installed_firmware = MqttMessage::new(&c8y_topic, payload);

        Ok(vec![installed_firmware])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;
    use c8y_api::smartrest::topic::C8yTopic;
    use serde_json::json;
    use std::time::Duration;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::Sender;
    use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
    use tedge_mqtt_ext::test_helpers::assert_received_includes_json;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

    #[tokio::test]
    async fn mapper_converts_firmware_update_metadata_for_main_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate firmware_update cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/firmware_update"),
            r#"{"name":"firmware", "version":"1.0"}"#,
        ))
        .await
        .expect("Send failed");

        // Validate if current installed firmware message is sent
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "115,firmware,1.0,")]).await;

        // Validate if the supported operation file is created
        assert!(ttd.path().join("operations/c8y/c8y_Firmware").exists());
    }

    #[tokio::test]
    async fn mapper_converts_firmware_update_metadata_for_child_device() {
        let ttd = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate log_upload cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/firmware_update"),
            r#"{"name":"firmware", "version":"1.0"}"#,
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
            [(
                "c8y/s/us",
                "101,test-device:device:child1,child1,thin-edge.io-child",
            )],
        )
        .await;

        // Validate if current installed firmware message is sent
        assert_received_contains_str(
            &mut mqtt,
            [("c8y/s/us/test-device:device:child1", "115,firmware,1.0,")],
        )
        .await;

        // Validate if the supported operation file is created
        assert!(ttd
            .path()
            .join("operations/c8y/test-device:device:child1/c8y_Firmware")
            .exists());
    }

    #[tokio::test]
    async fn mapper_converts_smartrest_firmware_req_to_firmware_update_cmd_for_main_device() {
        let cfg_dir = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_Firmware SmartREST request
        mqtt.send(MqttMessage::new(
            &C8yTopic::downstream_topic(),
            "515,test-device,myFirmware,1.0,http://www.my.url",
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
    async fn mapper_converts_smartrest_firmware_req_to_firmware_update_cmd_for_child_device() {
        let cfg_dir = TempTedgeDir::new();
        let (mqtt, _http, _fs, _timer, _ul, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate firmware_update cmd metadata message
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/firmware_update"),
            r#"{"name": "firmware", "version": "0.1"}"#,
        ))
        .await
        .expect("Send failed");

        mqtt.skip(3).await; // Skip entity registration, mapping and installed firmware messages

        // Simulate c8y_Firmware SmartREST request
        mqtt.send(MqttMessage::new(
            &C8yTopic::downstream_topic(),
            "515,test-device:device:child1,myFirmware,1.0,http://www.my.url",
        ))
        .await
        .expect("Send failed");

        assert_received_includes_json(
            &mut mqtt,
            [(
                "te/device/child1///cmd/firmware_update/+",
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
                    "te/device/main///cmd/firmware_update",
                    r#"{"name":"myFirmware","version":"1.0","url":"http://www.my.url"}"#,
                ), // Update metadata
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
                    "te/device/child1///cmd/firmware_update",
                    r#"{"name":"myFirmware","version":"1.0","url":"http://www.my.url"}"#,
                ), // Update metadata
            ],
        )
        .await;
    }
}
