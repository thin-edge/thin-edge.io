use anyhow::Context;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use tedge_api::commands::ControlCmd;
use tedge_api::CommandStatus;
use tedge_mqtt_ext::MqttMessage;
use tracing::warn;

use super::error::OperationError;
use super::EntityTarget;
use super::OperationContext;
use super::OperationOutcome;

impl OperationContext {
    pub async fn handle_control_state_change(
        &self,
        target: &EntityTarget,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> Result<OperationOutcome, OperationError> {
        if !self.capabilities.service_commands {
            warn!("Received a control command, however, service_commands feature is disabled");
            return Ok(OperationOutcome::Ignored);
        }

        let command = match ControlCmd::try_from_bytes(
            target.topic_id.clone(),
            cmd_id.to_owned(),
            message.payload_bytes(),
        )
        .context("Could not parse the payload of a service command")?
        {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok(OperationOutcome::Ignored);
            }
        };

        match command.status() {
            CommandStatus::Executing => Ok(OperationOutcome::Executing {
                extra_messages: vec![],
            }),
            CommandStatus::Successful => {
                let smartrest_set_operation = self.get_smartrest_successful_status_payload(
                    CumulocitySupportedOperations::C8yServiceCommand,
                    cmd_id,
                );

                Ok(OperationOutcome::Finished {
                    messages: vec![MqttMessage::new(
                        &target.smartrest_publish_topic,
                        smartrest_set_operation,
                    )],
                })
            }
            CommandStatus::Failed { reason } => {
                let action = &command.payload.action;
                Err(anyhow::anyhow!("The '{action}' action failed: {reason}").into())
            }
            _ => {
                // The other states are ignored
                Ok(OperationOutcome::Ignored)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::spawn_c8y_mapper_actor_with_config;
    use crate::tests::test_mapper_config;
    use crate::tests::TestHandle;
    use serde_json::json;
    use std::time::Duration;
    use tedge_actors::MessageReceiver;
    use tedge_actors::Sender;
    use tedge_mqtt_ext::MqttMessage;
    use tedge_mqtt_ext::Topic;
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

    #[tokio::test]
    async fn a_control_command_status_is_ignored_when_service_commands_are_disabled() {
        let tmp_dir = TempTedgeDir::new();
        let mut config = test_mapper_config(&tmp_dir);
        config.capabilities.service_commands = false;
        let test_handle = spawn_c8y_mapper_actor_with_config(&tmp_dir, config, true).await;
        let TestHandle { mqtt, .. } = test_handle;
        let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/collectd"),
            r#"{"@type":"service"}"#,
        ))
        .await
        .unwrap();
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/collectd/cmd/control/c8y-mapper-1234"),
            json!({"status": "executing", "action": "restart", "serviceName": "collectd", "serviceType": "service"})
                .to_string(),
        ))
        .await
        .unwrap();

        while let Some(message) = mqtt.recv().await {
            assert_ne!(
                message.topic.name, "c8y/s/us/test-device:device:main:service:collectd",
                "{message:?}"
            );
        }
    }
}
