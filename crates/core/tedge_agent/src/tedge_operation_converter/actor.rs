use crate::software_manager::actor::SoftwareCommand;
use async_trait::async_trait;
use camino::Utf8PathBuf;
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
use time::format_description;
use time::OffsetDateTime;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

fan_in_message_type!(AgentInput[MqttMessage, SoftwareCommand, RestartCommand] : Debug);

pub struct TedgeOperationConverterActor {
    pub(crate) mqtt_schema: MqttSchema,
    pub(crate) device_topic_id: EntityTopicId,
    pub(crate) workflows: WorkflowSupervisor,
    pub(crate) log_dir: Utf8PathBuf,
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

        let mut log_file = CommandLog::new(self.log_dir.clone(), &operation, &cmd_id).await;
        match self
            .workflows
            .get_workflow_current_action(&operation, &message)
        {
            Ok(None) => {
                log_file
                    .log_step("", "The command has been fully processed")
                    .await;
            }
            Ok(Some((state, action))) => {
                self.process_workflow_action(
                    &mut log_file,
                    message,
                    target,
                    operation,
                    cmd_id,
                    state,
                    action,
                )
                .await?;
            }
            Err(WorkflowExecutionError::UnknownOperation { operation }) => {
                info!("Ignoring {operation} operation which is not registered");
            }
            Err(WorkflowExecutionError::UnknownStep { operation, step }) => {
                info!("No action defined for {operation} operation {step} step");
                log_file.log_step(&step, "No action defined").await;
            }
            Err(err) => {
                error!("{operation} operation request cannot be processed: {err}");
                log_file
                    .log_step("Unknown", &format!("Error: {err}\n"))
                    .await;
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn process_workflow_action(
        &mut self,
        log_file: &mut CommandLog,
        message: MqttMessage,
        target: EntityTopicId,
        operation: OperationType,
        cmd_id: String,
        state: GenericCommandState,
        action: OperationAction,
    ) -> Result<(), RuntimeError> {
        log_file.log_state_action(&state, &action).await;

        match action {
            OperationAction::Clear => {
                info!(
                    "Waiting {} {operation} operation to be cleared",
                    state.status
                );
                Ok(())
            }
            OperationAction::MoveTo(next_step) => {
                info!("Moving {operation} operation to state: {next_step}");
                let new_state = state.move_to(next_step);
                self.publish_command_state(operation, cmd_id, new_state)
                    .await
            }
            OperationAction::BuiltIn => {
                let step = &state.status;
                info!("Processing {operation} operation {step} step");
                self.process_internal_operation(target, operation, cmd_id, message)
                    .await
            }
            OperationAction::Delegate(participant) => {
                let step = &state.status;
                info!("Delegating {operation} operation {step} step to: {participant}");
                // TODO fail the operation on timeout
                Ok(())
            }
            OperationAction::Restart {
                on_exec,
                on_success,
                on_error,
            } => {
                let step = &state.status;
                info!("Restarting in the context of {operation} operation {step} step");
                let cmd = RestartCommand::with_context(
                    target,
                    cmd_id.clone(),
                    state.clone(),
                    on_exec,
                    on_success,
                    on_error,
                );
                self.restart_sender.send(cmd).await?;
                Ok(())
            }
            OperationAction::Script(script, handlers) => {
                let step = &state.status;
                info!("Processing {operation} operation {step} step with script: {script}");

                let script_name = script.command.clone();
                let command = {
                    let command = Execute::new(script_name.clone(), script.args);
                    match (
                        handlers.graceful_timeout(),
                        handlers.forceful_timeout_extension(),
                    ) {
                        (Some(timeout), Some(extra)) => command
                            .with_graceful_timeout(timeout)
                            .with_forceful_timeout_extension(extra),
                        (Some(timeout), None) => command.with_graceful_timeout(timeout),
                        (None, _) => command,
                    }
                };
                let output = self.script_runner.await_response(command).await?;
                log_file.log_script_output(&output).await;

                let new_state = state.update_with_script_output(script_name, output, handlers);
                self.publish_command_state(operation, cmd_id, new_state)
                    .await
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
        let message = match response.resume_context() {
            None => response.command_message(&self.mqtt_schema),
            Some(context) => context.into_message(),
        };
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

struct CommandLog {
    path: Utf8PathBuf,
    file: Option<File>,
}

impl CommandLog {
    pub async fn new(log_dir: Utf8PathBuf, operation: &OperationType, cmd_id: &str) -> Self {
        let path = log_dir
            .clone()
            .join(format!("workflow-{}-{}.log", operation, cmd_id));
        match File::options()
            .append(true)
            .create(true)
            .open(path.clone())
            .await
        {
            Ok(file) => CommandLog {
                path,
                file: Some(file),
            },
            Err(err) => {
                error!("Fail to open log file {path}: {err}");
                CommandLog { path, file: None }
            }
        }
    }

    async fn log_state_action(&mut self, state: &GenericCommandState, action: &OperationAction) {
        let step = &state.status;
        let state = &state.payload.to_string();
        let message = format!(
            r#"
State: {state}
Action: {action}
"#
        );
        self.log_step(step, &message).await
    }

    async fn log_step(&mut self, step: &str, action: &str) {
        let now = OffsetDateTime::now_utc()
            .format(&format_description::well_known::Rfc3339)
            .unwrap();
        let message = format!(
            r#"------------------------------------
{step}: {now}
{action}

"#
        );
        if let Err(err) = self.write(&message).await {
            error!("Fail to log to {}: {err}", self.path)
        }
    }

    async fn log_script_output(&mut self, result: &Result<Output, std::io::Error>) {
        if let Err(err) = self.write_script_output(result).await {
            error!("Fail to log to {}: {err}", self.path)
        }
    }

    async fn write_script_output(
        &mut self,
        result: &Result<Output, std::io::Error>,
    ) -> Result<(), std::io::Error> {
        if let Some(file) = self.file.as_mut() {
            logged_command::LoggedCommand::log_outcome("", result, file).await?;
            file.sync_all().await?;
        }
        Ok(())
    }

    async fn write(&mut self, message: &str) -> Result<(), std::io::Error> {
        if let Some(file) = self.file.as_mut() {
            file.write_all(message.as_bytes()).await?;
            file.flush().await?;
            file.sync_all().await?;
        }
        Ok(())
    }
}
