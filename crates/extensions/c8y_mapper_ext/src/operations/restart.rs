use c8y_api::smartrest;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use tedge_api::workflow::GenericCommandState;
use tedge_api::CommandStatus;
use tedge_api::RestartCommand;
use tedge_mqtt_ext::MqttMessage;

use crate::error::ConversionError;

use super::EntityTarget;
use super::OperationContext;

impl OperationContext {
    pub async fn publish_restart_operation_status(
        &self,
        target: EntityTarget,
        cmd_id: &str,
        message: MqttMessage,
    ) -> Result<(Vec<MqttMessage>, Option<GenericCommandState>), ConversionError> {
        let command = match RestartCommand::try_from_bytes(
            target.topic_id,
            cmd_id.to_owned(),
            message.payload_bytes(),
        )? {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok((vec![], None));
            }
        };
        let topic = target.smartrest_publish_topic;

        let messages = match command.status() {
            CommandStatus::Executing => {
                let smartrest_set_operation =
                    smartrest::smartrest_serializer::set_operation_executing(
                        CumulocitySupportedOperations::C8yRestartRequest,
                    );
                vec![MqttMessage::new(&topic, smartrest_set_operation)]
            }
            CommandStatus::Successful => {
                let smartrest_set_operation =
                    smartrest::smartrest_serializer::succeed_operation_no_payload(
                        CumulocitySupportedOperations::C8yRestartRequest,
                    );

                vec![
                    command.clearing_message(&self.mqtt_schema),
                    MqttMessage::new(&topic, smartrest_set_operation),
                ]
            }
            CommandStatus::Failed { ref reason } => {
                let smartrest_set_operation = smartrest::smartrest_serializer::fail_operation(
                    CumulocitySupportedOperations::C8yRestartRequest,
                    &format!("Restart Failed: {reason}"),
                );

                vec![
                    command.clearing_message(&self.mqtt_schema),
                    MqttMessage::new(&topic, smartrest_set_operation),
                ]
            }
            _ => {
                // The other states are ignored
                vec![]
            }
        };

        Ok((
            messages,
            Some(command.into_generic_command(&self.mqtt_schema)),
        ))
    }
}
