use anyhow::Context;
use c8y_api::smartrest;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::CommandStatus;
use tedge_api::SoftwareListCommand;
use tedge_api::SoftwareUpdateCommand;
use tedge_mqtt_ext::MqttMessage;

use super::error::OperationError;
use super::EntityTarget;
use super::OperationContext;
use super::OperationOutcome;

impl OperationContext {
    pub async fn publish_software_update_status(
        &self,
        target: &EntityTarget,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> Result<OperationOutcome, OperationError> {
        let command = match SoftwareUpdateCommand::try_from_bytes(
            target.topic_id.clone(),
            cmd_id.to_string(),
            message.payload_bytes(),
        )
        .context("Could not parse command as a software update command")?
        {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok(OperationOutcome::Ignored);
            }
        };

        let topic = &target.smartrest_publish_topic;
        match command.status() {
            CommandStatus::Init | CommandStatus::Scheduled | CommandStatus::Unknown => {
                // The command has not been processed yet
                Ok(OperationOutcome::Ignored)
            }
            CommandStatus::Executing => Ok(OperationOutcome::Executing {
                extra_messages: vec![],
            }),
            CommandStatus::Successful => {
                let smartrest_set_operation =
                    smartrest::smartrest_serializer::succeed_operation_no_payload(
                        CumulocitySupportedOperations::C8ySoftwareUpdate,
                    );

                Ok(OperationOutcome::Finished {
                    messages: vec![
                        MqttMessage::new(topic, smartrest_set_operation),
                        self.request_software_list(&target.topic_id),
                    ],
                })
            }
            // TODO(marcel): use simpler error handling once software list request extracted to converter
            CommandStatus::Failed { reason } => {
                let smartrest_set_operation = smartrest::smartrest_serializer::fail_operation(
                    CumulocitySupportedOperations::C8ySoftwareUpdate,
                    &reason,
                );

                Ok(OperationOutcome::Finished {
                    messages: vec![
                        MqttMessage::new(topic, smartrest_set_operation),
                        self.request_software_list(&target.topic_id),
                    ],
                })
            }
        }
    }

    fn request_software_list(&self, target: &EntityTopicId) -> MqttMessage {
        let cmd_id = self.command_id.new_id();
        let request = SoftwareListCommand::new(target, cmd_id);
        request.command_message(&self.mqtt_schema)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
    use serde_json::json;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::Sender;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::mqtt_topics::MqttSchema;
    use tedge_api::CommandStatus;
    use tedge_api::SoftwareUpdateCommand;
    use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
    use tedge_mqtt_ext::test_helpers::assert_received_includes_json;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_test_utils::fs::TempTedgeDir;

    use crate::tests::skip_init_messages;
    use crate::tests::spawn_c8y_mapper_actor;
    use crate::tests::spawn_dummy_c8y_http_proxy;
    use crate::tests::TestHandle;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

    #[tokio::test]
    async fn mapper_publishes_software_update_request() {
        // The test assures c8y mapper correctly receives software update request from JSON over MQTT
        // and converts it to thin-edge json message published on `te/device/main///cmd/software_update/+`.
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, http, .. } = test_handle;
        spawn_dummy_c8y_http_proxy(http);

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate c8y_SoftwareUpdate JSON over MQTT request
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "c8y_SoftwareUpdate": [
                    {
                        "name": "nodered",
                        "action": "install",
                        "version": "1.0.0::debian",
                        "url": ""
                    }
                ],
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
                "te/device/main///cmd/software_update/c8y-mapper-123456",
                json!({
                    "status": "init",
                    "updateList": [
                        {
                            "type": "debian",
                            "modules": [
                                {
                                    "name": "nodered",
                                    "version": "1.0.0",
                                    "action": "install"
                                }
                            ]
                        }
                    ]
                }),
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn mapper_publishes_software_update_status_onto_c8y_topic() {
        // The test assures SM Mapper correctly receives software update response message on `te/device/main///cmd/software_update/123`
        // and publishes status of the operation `501` on `c8y/s/us`

        // Start SM Mapper
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, http, .. } = test_handle;
        spawn_dummy_c8y_http_proxy(http);

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
        skip_init_messages(&mut mqtt).await;

        // Prepare and publish a software update status response message `executing` on `te/device/main///cmd/software_update/123`.
        let mqtt_schema = MqttSchema::default();
        let device = EntityTopicId::default_main_device();
        let request = SoftwareUpdateCommand::new(&device, "c8y-mapper-123".to_string());
        let response = request.with_status(CommandStatus::Executing);
        mqtt.send(response.command_message(&mqtt_schema))
            .await
            .expect("Send failed");

        // Expect `501` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_SoftwareUpdate")]).await;

        // Prepare and publish a software update response `successful`.
        let response = response.with_status(CommandStatus::Successful);
        mqtt.send(response.command_message(&mqtt_schema))
            .await
            .expect("Send failed");

        // Expect `503` messages with correct payload have been received on `c8y/s/us`, if no msg received for the timeout the test fails.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "503,c8y_SoftwareUpdate")]).await;

        // An updated list of software is requested
        assert_received_contains_str(
            &mut mqtt,
            [(
                "te/device/main///cmd/software_list/+",
                r#"{"status":"init"}"#,
            )],
        )
        .await;

        // The successful state is cleared
        assert_received_contains_str(
            &mut mqtt,
            [("te/device/main///cmd/software_update/c8y-mapper-123", "")],
        )
        .await;
    }

    #[tokio::test]
    async fn mapper_publishes_software_update_failed_status_onto_c8y_topic() {
        // Start SM Mapper
        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
        skip_init_messages(&mut mqtt).await;

        // The agent publish an error
        let mqtt_schema = MqttSchema::default();
        let device = EntityTopicId::default_main_device();
        let response = SoftwareUpdateCommand::new(&device, "c8y-mapper-123".to_string())
            .with_error("Partial failure: Couldn't install collectd and nginx".to_string());
        mqtt.send(response.command_message(&mqtt_schema))
            .await
            .expect("Send failed");

        // `502` messages with correct payload have been received on `c8y/s/us`, if no msg received for the timeout the test fails.
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us",
                "502,c8y_SoftwareUpdate,Partial failure: Couldn't install collectd and nginx",
            )],
        )
        .await;

        // An updated list of software is requested
        assert_received_contains_str(
            &mut mqtt,
            [(
                "te/device/main///cmd/software_list/+",
                r#"{"status":"init"}"#,
            )],
        )
        .await;

        // The failed state is cleared
        assert_received_contains_str(
            &mut mqtt,
            [("te/device/main///cmd/software_update/c8y-mapper-123", "")],
        )
        .await;
    }

    #[tokio::test]
    async fn mapper_publishes_software_update_request_with_wrong_action() {
        // The test assures c8y-mapper correctly receives software update request via JSON over MQTT
        // Then the c8y-mapper finds out that wrong action as part of the update request.
        // Then c8y-mapper publishes an operation status message as executing `501,c8y_SoftwareUpdate'
        // Then c8y-mapper publishes an operation status message as failed `502,c8y_SoftwareUpdate,Action remove is not recognized. It must be install or delete.` on `c8/s/us`.
        // Then the subscriber that subscribed for messages on `c8/s/us` receives these messages and verifies them.

        let ttd = TempTedgeDir::new();
        let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
        let TestHandle { mqtt, .. } = test_handle;

        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
        skip_init_messages(&mut mqtt).await;

        // Publish a c8y_SoftwareUpdate via JSON over MQTT that contains a wrong action `remove`, that is not known by c8y.
        mqtt.send(MqttMessage::new(
            &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
            json!({
                "id": "123456",
                "c8y_SoftwareUpdate": [
                    {
                        "name": "nodered",
                        "action": "remove",
                        "version": "1.0.0::debian"
                    }
                ],
                "externalSource": {
                    "externalId": "test-device",
                    "type": "c8y_Serial"
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect a 501 (executing) followed by a 502 (failed)
        assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us",
                "501,c8y_SoftwareUpdate",
            ),
            (
                "c8y/s/us",
                "502,c8y_SoftwareUpdate,Parameter remove is not recognized. It must be install or delete."
            )
        ],
    )
        .await;
    }
}
