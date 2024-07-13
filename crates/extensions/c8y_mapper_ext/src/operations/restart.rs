use c8y_api::smartrest;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use tedge_api::CommandStatus;
use tedge_api::RestartCommand;
use tedge_mqtt_ext::MqttMessage;

use crate::error::ConversionError;

use super::EntityTarget;
use super::OperationContext;
use super::OperationResult;

impl OperationContext {
    pub async fn publish_restart_operation_status(
        &self,
        target: &EntityTarget,
        cmd_id: &str,
        message: MqttMessage,
    ) -> Result<OperationResult, ConversionError> {
        let command = match RestartCommand::try_from_bytes(
            target.topic_id.clone(),
            cmd_id.to_owned(),
            message.payload_bytes(),
        )? {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok(OperationResult::Ignored);
            }
        };
        let topic = &target.smartrest_publish_topic;

        match command.status() {
            CommandStatus::Executing => Ok(OperationResult::Executing),
            CommandStatus::Successful => {
                let smartrest_set_operation =
                    smartrest::smartrest_serializer::succeed_operation_no_payload(
                        CumulocitySupportedOperations::C8yRestartRequest,
                    );

                Ok(OperationResult::Finished {
                    messages: vec![MqttMessage::new(topic, smartrest_set_operation)],
                    command: command.into_generic_command(&self.mqtt_schema),
                })
            }
            CommandStatus::Failed { ref reason } => {
                let smartrest_set_operation = smartrest::smartrest_serializer::fail_operation(
                    CumulocitySupportedOperations::C8yRestartRequest,
                    &format!("Restart Failed: {reason}"),
                );

                Ok(OperationResult::Finished {
                    messages: vec![MqttMessage::new(topic, smartrest_set_operation)],
                    command: command.into_generic_command(&self.mqtt_schema),
                })
            }
            _ => {
                // The other states are ignored
                Ok(OperationResult::Ignored)
            }
        }
    }
}
