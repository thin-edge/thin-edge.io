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
