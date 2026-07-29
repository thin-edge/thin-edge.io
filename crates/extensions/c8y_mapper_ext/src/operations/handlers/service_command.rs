use anyhow::Context;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use tedge_api::workflow::GenericCommandState;
use tedge_api::CommandStatus;
use tedge_mqtt_ext::MqttMessage;

use super::error::OperationError;
use super::EntityTarget;
use super::OperationContext;
use super::OperationOutcome;

impl OperationContext {
    /// Report to Cumulocity the state of an action run on a service.
    ///
    /// Any command of a service is an action of that service, triggered by the single
    /// `c8y_ServiceCommand` operation. So this handler is not tied to one command payload:
    /// only the status of the command is read, whatever the action is and whichever workflow
    /// runs it.
    pub async fn handle_service_command_state_change(
        &self,
        target: &EntityTarget,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> Result<OperationOutcome, OperationError> {
        let command = GenericCommandState::from_command_message(message)
            .context("Could not parse the payload of a service command")?;

        match command.get_command_status() {
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
                let action = command.operation().unwrap_or_default();
                Err(anyhow::anyhow!("The '{action}' action failed: {reason}").into())
            }
            _ => {
                // The other states are ignored, including the states of a custom workflow
                Ok(OperationOutcome::Ignored)
            }
        }
    }
}
