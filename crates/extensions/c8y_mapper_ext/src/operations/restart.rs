use anyhow::Context;
use c8y_api::smartrest;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use tedge_api::CommandStatus;
use tedge_api::RestartCommand;
use tedge_mqtt_ext::MqttMessage;

use super::error::OperationError;
use super::EntityTarget;
use super::OperationContext;
use super::OperationOutcome;

impl OperationContext {
    pub async fn publish_restart_operation_status(
        &self,
        target: &EntityTarget,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> Result<OperationOutcome, OperationError> {
        let command = match RestartCommand::try_from_bytes(
            target.topic_id.clone(),
            cmd_id.to_owned(),
            message.payload_bytes(),
        )
        .context("Could not parse command as a restart command")?
        {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok(OperationOutcome::Ignored);
            }
        };
        let topic = &target.smartrest_publish_topic;

        match command.status() {
            CommandStatus::Executing => Ok(OperationOutcome::Executing),
            CommandStatus::Successful => {
                let smartrest_set_operation =
                    smartrest::smartrest_serializer::succeed_operation_no_payload(
                        CumulocitySupportedOperations::C8yRestartRequest,
                    );

                Ok(OperationOutcome::Finished {
                    messages: vec![MqttMessage::new(topic, smartrest_set_operation)],
                })
            }
            CommandStatus::Failed { ref reason } => {
                Err(anyhow::anyhow!("Restart Failed: {reason}").into())
            }
            _ => {
                // The other states are ignored
                Ok(OperationOutcome::Ignored)
            }
        }
    }
}
