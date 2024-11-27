use anyhow::Context;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use tedge_api::workflow::GenericCommandState;
use tedge_api::CommandStatus;
use tedge_mqtt_ext::MqttMessage;

use super::EntityTarget;
use super::OperationContext;
use super::OperationError;
use super::OperationOutcome;

impl OperationContext {
    pub async fn handle_custom_operation_state_change(
        &self,
        target: &EntityTarget,
        cmd_id: &str,
        message: &MqttMessage,
        operation_name: &str,
    ) -> Result<OperationOutcome, OperationError> {
        let command = match GenericCommandState::from_command_message(message)
            .context("Could not parse command as a custom operation command")
        {
            Ok(command) => command,
            Err(_) => return Ok(OperationOutcome::Ignored),
        };

        let sm_topic = &target.smartrest_publish_topic;

        match command.get_command_status() {
            CommandStatus::Executing => Ok(OperationOutcome::Executing {
                extra_messages: vec![],
            }),
            CommandStatus::Successful => {
                let smartrest_set_operation = self.get_smartrest_successful_status_payload(
                    CumulocitySupportedOperations::C8yCustom(operation_name.to_string()),
                    cmd_id,
                );
                let c8y_notification = MqttMessage::new(sm_topic, smartrest_set_operation);

                Ok(OperationOutcome::Finished {
                    messages: vec![c8y_notification],
                })
            }
            CommandStatus::Failed { reason } => {
                let smartrest_set_operation = self.get_smartrest_failed_status_payload(
                    CumulocitySupportedOperations::C8yCustom(operation_name.to_string()),
                    &reason,
                    cmd_id,
                );
                let c8y_notification = MqttMessage::new(sm_topic, smartrest_set_operation);

                Ok(OperationOutcome::Finished {
                    messages: vec![c8y_notification],
                })
            }
            _ => Ok(OperationOutcome::Ignored),
        }
    }
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
                "text": "do something"
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
    async fn handle_custom_operation_executing_and_failed_cmd_for_child_device() {
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

        mqtt.skip(1).await; // Skip child device registration messages
                            // Simulate custom operation command with "executing" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/command/c8y-mapper-1234"),
            json!({
                "status": "executing",
                "text": "do something"
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `504` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us/child1", "504,1234")]).await;

        // Simulate custom operation command with "failed" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/command/c8y-mapper-1234"),
            json!({
                "status": "failed",
                "text": "do something",
                "reason": "Something went wrong"
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `505` smartrest message on `c8y/s/us`.
        assert_received_contains_str(
            &mut mqtt,
            [("c8y/s/us/child1", "505,1234,Something went wrong")],
        )
        .await;
    }

    #[tokio::test]
    async fn handle_custom_operation_successful_cmd_for_main_device() {
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
                "text": "do something"
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `506` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us", "506,1234")]).await;
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

        mqtt.skip(1).await; // Skip child device registration messages

        // Simulate custom operation command with "successful" state
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/command/c8y-mapper-1234"),
            json!({
                "status": "successful",
                "text": "do something"
            })
            .to_string(),
        ))
        .await
        .expect("Send failed");

        // Expect `506` smartrest message on `c8y/s/us`.
        assert_received_contains_str(&mut mqtt, [("c8y/s/us/child1", "506,1234")]).await;
    }
}
