use c8y_api::json_c8y::C8yUpdateSoftwareListResponse;
use c8y_api::smartrest;
use tedge_api::workflow::GenericCommandState;
use tedge_api::CommandStatus;
use tedge_api::SoftwareListCommand;
use tedge_config::SoftwareManagementApiFlag;
use tedge_mqtt_ext::MqttMessage;
use tracing::error;

use crate::error::ConversionError;

use super::EntityTarget;
use super::OperationHandler;

const SOFTWARE_LIST_CHUNK_SIZE: usize = 100;

impl OperationHandler {
    pub async fn publish_software_list(
        &self,
        target: EntityTarget,
        cmd_id: &str,
        message: &MqttMessage,
    ) -> Result<(Vec<MqttMessage>, Option<GenericCommandState>), ConversionError> {
        let command = match SoftwareListCommand::try_from_bytes(
            target.topic_id,
            cmd_id.to_owned(),
            message.payload_bytes(),
        )? {
            Some(command) => command,
            None => {
                // The command has been fully processed
                return Ok((Vec::new(), None));
            }
        };

        let messages = match command.status() {
            CommandStatus::Successful => {
                // Send a list via HTTP to support backwards compatibility to c8y < 10.14
                if self.software_management_api == SoftwareManagementApiFlag::Legacy {
                    let c8y_software_list: C8yUpdateSoftwareListResponse = (&command).into();
                    self.http_proxy
                        .clone()
                        .send_software_list_http(
                            c8y_software_list,
                            target.external_id.as_ref().to_string(),
                        )
                        .await?;
                    return Ok((vec![command.clearing_message(&self.mqtt_schema)], None));
                }

                // Send a list via SmartREST, "advanced software list" feature c8y >= 10.14
                let topic = target.smartrest_publish_topic;
                let payloads = smartrest::smartrest_serializer::get_advanced_software_list_payloads(
                    &command,
                    SOFTWARE_LIST_CHUNK_SIZE,
                );

                let mut messages: Vec<MqttMessage> = Vec::new();
                for payload in payloads {
                    messages.push(MqttMessage::new(&topic, payload))
                }
                messages.push(command.clearing_message(&self.mqtt_schema));
                messages
            }

            CommandStatus::Failed { reason } => {
                error!("Fail to list installed software packages: {reason}");
                vec![command.clearing_message(&self.mqtt_schema)]
            }

            CommandStatus::Init
            | CommandStatus::Scheduled
            | CommandStatus::Executing
            | CommandStatus::Unknown => {
                // C8Y doesn't expect any message to be published
                Vec::new()
            }
        };

        Ok((
            messages,
            Some(command.into_generic_command(&self.mqtt_schema)),
        ))
    }
}
