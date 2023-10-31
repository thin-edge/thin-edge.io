use crate::software_manager::actor::SoftwareRequest;
use crate::software_manager::actor::SoftwareResponse;
use async_trait::async_trait;
use log::error;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::LoggingReceiver;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_api::messages::RestartCommand;
use tedge_api::messages::SoftwareListCommand;
use tedge_api::messages::SoftwareUpdateCommand;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::AnonymisedAuth;
use tedge_api::IdentityInjector;
use tedge_api::NeverAuth;
use tedge_mqtt_ext::MqttMessage;

fan_in_message_type!(AgentInput[MqttMessage, SoftwareResponse, RestartCommand] : Debug);

pub struct TedgeOperationConverterActor {
    mqtt_schema: MqttSchema,
    device_topic_id: EntityTopicId,
    input_receiver: LoggingReceiver<AgentInput>,
    software_sender: LoggingSender<SoftwareRequest>,
    restart_sender: LoggingSender<RestartCommand>,
    mqtt_publisher: LoggingSender<MqttMessage>,
    identity_injector: IdentityInjector,
}

#[async_trait]
impl Actor for TedgeOperationConverterActor {
    fn name(&self) -> &str {
        "TedgeOperationConverter"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        self.publish_operation_capabilities().await?;

        while let Some(input) = self.input_receiver.recv().await {
            match input {
                AgentInput::MqttMessage(message) => {
                    self.process_mqtt_message(message).await?;
                }
                AgentInput::SoftwareResponse(SoftwareResponse::SoftwareListCommand(res)) => {
                    self.process_software_list_response(res).await?;
                }
                AgentInput::SoftwareResponse(SoftwareResponse::SoftwareUpdateCommand(res)) => {
                    self.process_software_update_response(res).await?;
                }
                AgentInput::RestartCommand(cmd) => {
                    self.process_restart_response(cmd).await?;
                }
            }
        }
        Ok(())
    }
}

impl TedgeOperationConverterActor {
    pub fn new(
        mqtt_schema: MqttSchema,
        device_topic_id: EntityTopicId,
        input_receiver: LoggingReceiver<AgentInput>,
        software_sender: LoggingSender<SoftwareRequest>,
        restart_sender: LoggingSender<RestartCommand>,
        mqtt_publisher: LoggingSender<MqttMessage>,
        identity_injector: IdentityInjector,
    ) -> Self {
        Self {
            mqtt_schema,
            device_topic_id,
            input_receiver,
            software_sender,
            restart_sender,
            mqtt_publisher,
            identity_injector,
        }
    }

    async fn publish_operation_capabilities(&mut self) -> Result<(), RuntimeError> {
        let capabilities = [
            RestartCommand::capability_message(&self.mqtt_schema, &self.device_topic_id),
            SoftwareListCommand::capability_message(&self.mqtt_schema, &self.device_topic_id),
            SoftwareUpdateCommand::<NeverAuth>::capability_message(
                &self.mqtt_schema,
                &self.device_topic_id,
            ),
        ];
        for capability in capabilities {
            self.mqtt_publisher.send(capability).await?
        }
        Ok(())
    }

    async fn process_mqtt_message(&mut self, message: MqttMessage) -> Result<(), RuntimeError> {
        match self.mqtt_schema.entity_channel_of(&message.topic) {
            // Software List
            Ok((
                target,
                Channel::Command {
                    operation: OperationType::SoftwareList,
                    cmd_id,
                },
            )) => match SoftwareListCommand::try_from(target, cmd_id, message.payload_bytes()) {
                Ok(Some(cmd)) => {
                    self.software_sender.send(cmd.into()).await?;
                }
                Ok(None) => {
                    // The command has been fully processed
                }
                Err(err) => error!("Incorrect software_list request payload: {err}"),
            },

            // Software Update
            Ok((
                target,
                Channel::Command {
                    operation: OperationType::SoftwareUpdate,
                    cmd_id,
                },
            )) => match SoftwareUpdateCommand::try_from(target, cmd_id, message.payload_bytes()) {
                Ok(Some(cmd)) => {
                    let cmd = cmd.convert_auth_with(&self.identity_injector);
                    self.software_sender.send(cmd.into()).await?;
                }
                Ok(None) => {
                    // The command has been fully processed
                }
                Err(err) => error!("Incorrect software_update request payload: {err}"),
            },

            // Restart
            Ok((
                target,
                Channel::Command {
                    operation: OperationType::Restart,
                    cmd_id,
                },
            )) => match RestartCommand::try_from(target, cmd_id, message.payload_bytes()) {
                Ok(Some(cmd)) => {
                    self.restart_sender.send(cmd).await?;
                }
                Ok(None) => {
                    // The command has been fully processed
                }
                Err(err) => error!("Incorrect restart request payload: {err}"),
            },

            // Unknown command
            _ => {
                log::error!("Unknown command channel: {}", message.topic.name);
            }
        }
        Ok(())
    }

    async fn process_software_list_response(
        &mut self,
        response: SoftwareListCommand,
    ) -> Result<(), RuntimeError> {
        let message = response.command_message(&self.mqtt_schema);
        self.mqtt_publisher.send(message).await?;
        Ok(())
    }

    async fn process_software_update_response(
        &mut self,
        response: SoftwareUpdateCommand<AnonymisedAuth>,
    ) -> Result<(), RuntimeError> {
        let message = response.command_message(&self.mqtt_schema);
        self.mqtt_publisher.send(message).await?;
        Ok(())
    }

    async fn process_restart_response(
        &mut self,
        response: RestartCommand,
    ) -> Result<(), RuntimeError> {
        let message = response.command_message(&self.mqtt_schema);
        self.mqtt_publisher.send(message).await?;
        Ok(())
    }
}
