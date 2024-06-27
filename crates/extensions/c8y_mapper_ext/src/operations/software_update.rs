use c8y_api::smartrest;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::workflow::GenericCommandState;
use tedge_api::CommandStatus;
use tedge_api::SoftwareListCommand;
use tedge_api::SoftwareUpdateCommand;
use tedge_mqtt_ext::MqttMessage;

use crate::error::ConversionError;

use super::EntityTarget;
use super::OperationHandler;

impl OperationHandler {
    pub async fn publish_software_update_status(
        &self,
        target: EntityTarget,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> Result<(Vec<MqttMessage>, Option<GenericCommandState>), ConversionError> {
        let command = match SoftwareUpdateCommand::try_from_bytes(
            target.topic_id.clone(),
            cmd_id.to_string(),
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
            CommandStatus::Init | CommandStatus::Scheduled | CommandStatus::Unknown => {
                // The command has not been processed yet
                vec![]
            }
            CommandStatus::Executing => {
                let smartrest_set_operation_status =
                    smartrest::smartrest_serializer::set_operation_executing(
                        CumulocitySupportedOperations::C8ySoftwareUpdate,
                    );
                vec![MqttMessage::new(&topic, smartrest_set_operation_status)]
            }
            CommandStatus::Successful => {
                let smartrest_set_operation =
                    smartrest::smartrest_serializer::succeed_operation_no_payload(
                        CumulocitySupportedOperations::C8ySoftwareUpdate,
                    );

                vec![
                    MqttMessage::new(&topic, smartrest_set_operation),
                    command.clearing_message(&self.mqtt_schema),
                    self.request_software_list(&target.topic_id),
                ]
            }
            CommandStatus::Failed { reason } => {
                let smartrest_set_operation = smartrest::smartrest_serializer::fail_operation(
                    CumulocitySupportedOperations::C8ySoftwareUpdate,
                    &reason,
                );

                vec![
                    MqttMessage::new(&topic, smartrest_set_operation),
                    command.clearing_message(&self.mqtt_schema),
                    self.request_software_list(&target.topic_id),
                ]
            }
        };

        Ok((
            messages,
            Some(command.into_generic_command(&self.mqtt_schema)),
        ))
    }

    fn request_software_list(&self, target: &EntityTopicId) -> MqttMessage {
        let cmd_id = self.command_id.new_id();
        let request = SoftwareListCommand::new(target, cmd_id);
        request.command_message(&self.mqtt_schema)
    }
}
