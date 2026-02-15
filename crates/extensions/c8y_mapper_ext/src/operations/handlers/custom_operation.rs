use anyhow::Context;
use c8y_api::smartrest::payload::SmartrestPayload;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::EmbeddedCsv;
use c8y_api::smartrest::smartrest_serializer::TextOrCsv;
use serde_json::Value;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::StateExcerpt;
use tedge_api::CommandStatus;
use tedge_mqtt_ext::MqttMessage;

use super::EntityTarget;
use super::OperationContext;
use super::OperationOutcome;

impl OperationContext {
    pub async fn handle_custom_operation_state_change(
        &self,
        target: &EntityTarget,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> (OperationOutcome, Option<CumulocitySupportedOperations>) {
        let command = match GenericCommandState::from_command_message(message)
            .context("Could not parse command as a custom operation command")
        {
            Ok(command) => command,
            Err(_) => return (OperationOutcome::Ignored, None),
        };

        let mapper_id = self.command_id.prefix();
        let operation = match get_c8y_operation(mapper_id, &command.payload)
            .context("Could not get CumulocitySupportedOperation from payload")
        {
            Ok(c8y_operation) => c8y_operation,
            Err(_) => return (OperationOutcome::Ignored, None),
        };

        let sm_topic = &target.smartrest_publish_topic;
        let outcome = match command.get_command_status() {
            CommandStatus::Executing => OperationOutcome::Executing {
                extra_messages: vec![],
            },
            CommandStatus::Successful => {
                let smartrest_set_operation =
                    self.convert_output(mapper_id, &command, operation.clone(), cmd_id);
                let c8y_notification = MqttMessage::new(sm_topic, smartrest_set_operation);

                OperationOutcome::Finished {
                    messages: vec![c8y_notification],
                }
            }
            CommandStatus::Failed { reason } => {
                let smartrest_set_operation =
                    self.get_smartrest_failed_status_payload(operation.clone(), &reason, cmd_id);
                let c8y_notification = MqttMessage::new(sm_topic, smartrest_set_operation);

                OperationOutcome::Finished {
                    messages: vec![c8y_notification],
                }
            }
            _ => OperationOutcome::Ignored,
        };

        (outcome, Some(operation))
    }

    fn convert_output(
        &self,
        mapper_id: &str,
        state: &GenericCommandState,
        operation: CumulocitySupportedOperations,
        cmd_id: &str,
    ) -> SmartrestPayload {
        match state.payload.pointer(&format!("/{mapper_id}/output")) {
            Some(output) => {
                let excerpt = StateExcerpt::from(output.clone());
                match excerpt.extract_value_from(state) {
                    Value::Null => self.get_smartrest_successful_status_payload(operation, cmd_id),
                    Value::String(text) => {
                        let text_or_csv = TextOrCsv::Text(text.clone());
                        self.try_get_smartrest_successful_status_payload_with_args(
                            operation,
                            cmd_id,
                            text_or_csv,
                        )
                    }
                    Value::Array(vec) => {
                        let csv = vec.iter().map(|x| x.to_string() + ",").collect::<String>();
                        let text_or_csv = EmbeddedCsv::new(csv).into();
                        self.try_get_smartrest_successful_status_payload_with_args(
                            operation,
                            cmd_id,
                            text_or_csv,
                        )
                    }
                    Value::Bool(_) | Value::Number(_) | Value::Object(_) => self
                        .get_smartrest_failed_status_payload(
                            operation,
                            "'output' supports only String or Array",
                            cmd_id,
                        ),
                }
            }
            None => self.get_smartrest_successful_status_payload(operation, cmd_id),
        }
    }
}

fn get_c8y_operation(
    mapper_id: &str,
    value: &serde_json::Value,
) -> Option<CumulocitySupportedOperations> {
    if let Some(maybe_c8y_operation) = value.pointer(&format!("/{mapper_id}/on_fragment")) {
        if let Some(c8y_operation) = maybe_c8y_operation.as_str() {
            return Some(CumulocitySupportedOperations::C8yCustom(
                c8y_operation.to_string(),
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::config::C8yMapperConfig;
    use crate::tests::skip_init_messages;
    use crate::tests::spawn_c8y_mapper_actor_with_config;
    use crate::tests::test_mapper_config;
    use crate::tests::TestHandle;

    use serde_json::json;
    use std::time::Duration;
    use tedge_actors::test_helpers::MessageReceiverExt;
    use tedge_actors::Sender;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;

    use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

    #[tokio::test]
    async fn handle_custom_operation_executing_and_failed_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let config = C8yMapperConfig {
            smartrest_use_operation_id: true,
            ..test_mapper_config(&ttd)
        };
        let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate custom operation command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/command/c8y-mapper-1234"),
            json!({
                "status": "executing",
                "text": "do something",
                "c8y-mapper": {
                    "on_fragment": "c8y_Something",
                    "output": null
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `504` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "504,1234")]).await;

        // Simulate custom operation command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/command/c8y-mapper-1234"),
            json!({
                "status": "failed",
                "text": "do something",
                "reason": "Something went wrong",
                "c8y-mapper": {
                    "on_fragment": "c8y_Something",
                    "output": null
                }
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
    async fn handle_custom_operation_executing_and_failed_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let config = C8yMapperConfig {
            smartrest_use_operation_id: false,
            ..test_mapper_config(&ttd)
        };
        let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
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

        mqtt.skip(2).await; // Skip child device registration messages
                            // Simulate custom operation command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/command/c8y-mapper-1234"),
            json!({
                "status": "executing",
                "text": "do something",
                "c8y-mapper": {
                    "on_fragment": "c8y_Something",
                    "output": null
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `501` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us/child1", "501,c8y_Something")]).await;

        // Simulate custom operation command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/command/c8y-mapper-1234"),
            json!({
                "status": "failed",
                "text": "do something",
                "reason": "Something went wrong",
                "c8y-mapper": {
                    "on_fragment": "c8y_Something",
                    "output": null
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `502` smartrest message on `c8y/s/us`.
        assert_received_contains_str(
            &mut mqtt,
            [("c8y/s/us/child1", "502,c8y_Something,Something went wrong")],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_custom_operation_successful_cmd_for_main_device() {
        let ttd = TempTedgeDir::new();
        let config = C8yMapperConfig {
            smartrest_use_operation_id: false,
            ..test_mapper_config(&ttd)
        };
        let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate custom operation command with "successful" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/command/c8y-mapper-1234"),
            json!({
                "status": "successful",
                "text": "do something",
                "c8y-mapper": {
                    "on_fragment": "c8y_Something",
                    "output": null
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `503` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "503,c8y_Something")]).await;
    }

    #[tokio::test]
    async fn handle_custom_operation_successful_cmd_for_main_device_with_output() {
        let ttd = TempTedgeDir::new();
        let config = C8yMapperConfig {
            smartrest_use_operation_id: true,
            ..test_mapper_config(&ttd)
        };
        let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate custom operation command with "successful" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/command/c8y-mapper-1234"),
            json!({
                "status": "successful",
                "text": "do something",
                "result": ["on","off","on"],
                "c8y-mapper": {
                    "on_fragment": "c8y_Something",
                    "output": "${.payload.result}"
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `506` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "506,1234,on,off,on")]).await;
    }

    #[tokio::test]
    async fn handle_custom_operation_successful_cmd_with_unsupported_result() {
        let ttd = TempTedgeDir::new();
        let config = C8yMapperConfig {
            smartrest_use_operation_id: false,
            ..test_mapper_config(&ttd)
        };
        let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        skip_init_messages(&mut mqtt).await;

        // Simulate custom operation command with "successful" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/command/c8y-mapper-1234"),
            json!({
                "status": "successful",
                "text": "do something",
                "result": true,
                "c8y-mapper": {
                    "on_fragment": "c8y_Something",
                    "output": "${.payload.result}"
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `502` smartrest message on `c8y/s/us` as 'result' contains unsupported type
        assert_received_contains_str(
            &mut mqtt,
            [(
                "c8y/s/us",
                "502,c8y_Something,'output' supports only String or Array",
            )],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_custom_operation_successful_cmd_for_child_device() {
        let ttd = TempTedgeDir::new();
        let config = C8yMapperConfig {
            smartrest_use_operation_id: true,
            ..test_mapper_config(&ttd)
        };
        let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
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

        mqtt.skip(2).await; // Skip child device registration messages

        // Simulate custom operation command with "successful" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/command/c8y-mapper-1234"),
            json!({
                "status": "successful",
                "text": "do something",
                "result": "on,off,on",
                "c8y-mapper": {
                    "on_fragment": "c8y_Something",
                    "output": "${.payload.result}"
                }
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `506` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us/child1", "506,1234,\"on,off,on\"")])
            .await;
    }
}
