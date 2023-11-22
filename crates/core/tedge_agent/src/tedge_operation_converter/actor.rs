use crate::software_manager::actor::SoftwareCommand;
use async_trait::async_trait;
use log::error;
use log::info;
use std::process::Output;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ClientMessageBox;
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
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::OperationAction;
use tedge_api::workflow::WorkflowExecutionError;
use tedge_api::workflow::WorkflowSupervisor;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_script_ext::Execute;

fan_in_message_type!(AgentInput[MqttMessage, SoftwareCommand, RestartCommand] : Debug);

pub struct TedgeOperationConverterActor {
    pub(crate) mqtt_schema: MqttSchema,
    pub(crate) device_topic_id: EntityTopicId,
    pub(crate) workflows: WorkflowSupervisor,
    pub(crate) input_receiver: LoggingReceiver<AgentInput>,
    pub(crate) software_sender: LoggingSender<SoftwareCommand>,
    pub(crate) restart_sender: LoggingSender<RestartCommand>,
    pub(crate) mqtt_publisher: LoggingSender<MqttMessage>,
    pub(crate) script_runner: ClientMessageBox<Execute, std::io::Result<Output>>,
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
                AgentInput::SoftwareCommand(SoftwareCommand::SoftwareListCommand(res)) => {
                    self.process_software_list_response(res).await?;
                }
                AgentInput::SoftwareCommand(SoftwareCommand::SoftwareUpdateCommand(res)) => {
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
    async fn publish_operation_capabilities(&mut self) -> Result<(), RuntimeError> {
        for capability in self
            .workflows
            .capability_messages(&self.mqtt_schema, &self.device_topic_id)
        {
            self.mqtt_publisher.send(capability).await?
        }
        Ok(())
    }

    async fn process_mqtt_message(&mut self, message: MqttMessage) -> Result<(), RuntimeError> {
        let (target, operation, cmd_id) = match self.mqtt_schema.entity_channel_of(&message.topic) {
            Ok((target, Channel::Command { operation, cmd_id })) => (target, operation, cmd_id),

            _ => {
                log::error!("Unknown command channel: {}", message.topic.name);
                return Ok(());
            }
        };

        match self
            .workflows
            .get_workflow_current_action(&operation, &message)
        {
            Ok(None) | Ok(Some((_, OperationAction::Clear))) => {
                // The command has been fully processed
                Ok(())
            }
            Ok(Some((state, OperationAction::MoveTo(next_step)))) => {
                let new_state = state.move_to(next_step);
                self.publish_command_state(operation, cmd_id, new_state)
                    .await
            }
            Ok(Some((state, OperationAction::BuiltIn))) => {
                let step = &state.status;
                info!("Processing {operation} operation {step} step");
                self.process_internal_operation(target, operation, cmd_id, message)
                    .await
            }
            Ok(Some((state, OperationAction::Delegate(participant)))) => {
                let step = &state.status;
                info!("Delegating {operation} operation {step} step to: {participant}");
                // TODO fail the operation on timeout
                Ok(())
            }
            Ok(Some((state, OperationAction::Script(script)))) => {
                let step = &state.status;
                info!("Processing {operation} operation {step} step with script: {script}");
                if let Ok(mut command) = Execute::try_new(&script) {
                    command.args = state.inject_parameters(&command.args);
                    let output = self.script_runner.await_response(command).await?;
                    let new_state = state.update_with_script_output(script, output);
                    self.publish_command_state(operation, cmd_id, new_state)
                        .await
                } else {
                    let reason = format!("Fail to parse the command line: {script}");
                    let new_state = state.fail_with(reason);
                    self.publish_command_state(operation, cmd_id, new_state)
                        .await
                }
            }
            Err(WorkflowExecutionError::UnknownOperation { operation }) => {
                info!("Ignoring {operation} operation which is not registered");
                Ok(())
            }
            Err(err) => {
                error!("{operation} operation request cannot be processed: {err}");
                Ok(())
            }
        }
    }

    async fn process_internal_operation(
        &mut self,
        target: EntityTopicId,
        operation: OperationType,
        cmd_id: String,
        message: MqttMessage,
    ) -> Result<(), RuntimeError> {
        match operation {
            OperationType::SoftwareList => {
                match SoftwareListCommand::try_from(target, cmd_id, message.payload_bytes()) {
                    Ok(Some(cmd)) => {
                        self.software_sender.send(cmd.into()).await?;
                    }
                    Ok(None) => {
                        // The command has been fully processed
                    }
                    Err(err) => error!("Incorrect software_list request payload: {err}"),
                }
            }

            OperationType::SoftwareUpdate => {
                match SoftwareUpdateCommand::try_from(target, cmd_id, message.payload_bytes()) {
                    Ok(Some(cmd)) => {
                        self.software_sender.send(cmd.into()).await?;
                    }
                    Ok(None) => {
                        // The command has been fully processed
                    }
                    Err(err) => error!("Incorrect software_update request payload: {err}"),
                }
            }

            OperationType::Restart => {
                match RestartCommand::try_from(target, cmd_id, message.payload_bytes()) {
                    Ok(Some(cmd)) => {
                        self.restart_sender.send(cmd).await?;
                    }
                    Ok(None) => {
                        // The command has been fully processed
                    }
                    Err(err) => error!("Incorrect restart request payload: {err}"),
                }
            }

            // Command not managed by the agent
            _ => {}
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
        response: SoftwareUpdateCommand,
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

    async fn publish_command_state(
        &mut self,
        operation: OperationType,
        cmd_id: String,
        response: GenericCommandState,
    ) -> Result<(), RuntimeError> {
        let topic = self.mqtt_schema.topic_for(
            &self.device_topic_id,
            &Channel::Command { operation, cmd_id },
        );
        let payload = response.to_json_string();
        let message = MqttMessage::new(&topic, payload)
            .with_qos(QoS::AtLeastOnce)
            .with_retain();
        self.mqtt_publisher.send(message).await?;
        Ok(())
    }
}
