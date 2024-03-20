use crate::software_manager::actor::SoftwareCommand;
use crate::state_repository::state::AgentStateRepository;
use async_trait::async_trait;
use camino::Utf8PathBuf;
use log::error;
use log::info;
use std::process::Output;
use std::time::Duration;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ClientMessageBox;
use tedge_actors::DynSender;
use tedge_actors::LoggingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::UnboundedLoggingReceiver;
use tedge_api::commands::RestartCommand;
use tedge_api::commands::SoftwareCommandMetadata;
use tedge_api::commands::SoftwareListCommand;
use tedge_api::commands::SoftwareUpdateCommand;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::CommandBoard;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::GenericStateUpdate;
use tedge_api::workflow::OperationAction;
use tedge_api::workflow::WorkflowExecutionError;
use tedge_api::workflow::WorkflowSupervisor;
use tedge_api::Jsonify;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_script_ext::Execute;
use time::format_description;
use time::OffsetDateTime;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::time::sleep;

fan_in_message_type!(AgentInput[MqttMessage, GenericCommandState, SoftwareCommand, RestartCommand] : Debug);

pub struct TedgeOperationConverterActor {
    pub(crate) mqtt_schema: MqttSchema,
    pub(crate) device_topic_id: EntityTopicId,
    pub(crate) workflows: WorkflowSupervisor,
    pub(crate) state_repository: AgentStateRepository<CommandBoard>,
    pub(crate) log_dir: Utf8PathBuf,
    pub(crate) input_receiver: UnboundedLoggingReceiver<AgentInput>,
    pub(crate) software_sender: LoggingSender<SoftwareCommand>,
    pub(crate) restart_sender: LoggingSender<RestartCommand>,
    pub(crate) command_sender: DynSender<GenericCommandState>,
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
        self.load_command_board().await?;

        while let Some(input) = self.input_receiver.recv().await {
            match input {
                AgentInput::MqttMessage(message) => {
                    self.process_mqtt_message(message).await?;
                }
                AgentInput::GenericCommandState(command_state) => {
                    self.process_command_state_update(command_state).await?;
                }
                AgentInput::SoftwareCommand(SoftwareCommand::SoftwareListCommand(res)) => {
                    self.process_software_list_response(res).await?;
                }
                AgentInput::SoftwareCommand(SoftwareCommand::SoftwareUpdateCommand(res)) => {
                    self.process_software_update_response(res).await?;
                }
                AgentInput::SoftwareCommand(SoftwareCommand::SoftwareCommandMetadata(payload)) => {
                    self.publish_software_operation_capabilities(payload)
                        .await?;
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

    async fn publish_software_operation_capabilities(
        &mut self,
        payload: SoftwareCommandMetadata,
    ) -> Result<(), RuntimeError> {
        for operation in [OperationType::SoftwareList, OperationType::SoftwareUpdate] {
            let meta_topic = self
                .mqtt_schema
                .capability_topic_for(&self.device_topic_id, operation);
            let message = MqttMessage::new(&meta_topic, payload.to_json())
                .with_retain()
                .with_qos(QoS::AtLeastOnce);
            self.mqtt_publisher.send(message).await?;
        }
        Ok(())
    }

    async fn process_mqtt_message(&mut self, message: MqttMessage) -> Result<(), RuntimeError> {
        let (operation, cmd_id) = match self.mqtt_schema.entity_channel_of(&message.topic) {
            Ok((_, Channel::Command { operation, cmd_id })) => (operation, cmd_id),

            _ => {
                log::error!("Unknown command channel: {}", message.topic.name);
                return Ok(());
            }
        };

        let mut log_file = CommandLog::new(self.log_dir.clone(), &operation, &cmd_id).await;
        match self.workflows.apply_external_update(&operation, &message) {
            Ok(None) => {
                if message.payload_bytes().is_empty() {
                    log_file
                        .log_step("", "The command has been fully processed")
                        .await;
                    self.persist_command_board().await?;
                }
            }
            Ok(Some(state)) => {
                self.persist_command_board().await?;
                self.process_command_state_update(state).await?;
            }
            Err(WorkflowExecutionError::UnknownOperation { operation }) => {
                info!("Ignoring {operation} operation which is not registered");
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

    async fn process_command_state_update(
        &mut self,
        state: GenericCommandState,
    ) -> Result<(), RuntimeError> {
        let (target, operation, cmd_id) = match self.mqtt_schema.entity_channel_of(&state.topic) {
            Ok((target, Channel::Command { operation, cmd_id })) => (target, operation, cmd_id),

            _ => {
                log::error!("Unknown command channel: {}", state.topic.name);
                return Ok(());
            }
        };
        let mut log_file = CommandLog::new(self.log_dir.clone(), &operation, &cmd_id).await;

        let action = match self.workflows.get_action(&state) {
            Ok(action) => action,
            Err(WorkflowExecutionError::UnknownStep { operation, step }) => {
                info!("No action defined for {operation} operation {step} step");
                log_file.log_step(&step, "No action defined").await;
                return Ok(());
            }
            Err(err) => {
                error!("{operation} operation request cannot be processed: {err}");
                log_file
                    .log_step("Unknown", &format!("Error: {err}\n"))
                    .await;
                return Ok(());
            }
        };

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
                self.publish_command_state(new_state).await
            }
            OperationAction::BuiltIn => {
                let step = &state.status;
                info!("Processing {operation} operation {step} step");
                self.process_internal_operation(target, operation, cmd_id, state.payload)
                    .await
            }
            OperationAction::AwaitingAgentRestart(handlers) => {
                let step = &state.status;
                info!("{operation} operation {step} waiting for agent restart");
                // The following sleep is expected to be interrupted by a restart
                sleep(handlers.timeout.unwrap_or_default() + Duration::from_secs(60)).await;
                // As the sleep completes, it means the agent was not restarted
                // hence the operation is moved to its `on_timeout` target state
                let new_state = state.update(
                    handlers
                        .on_timeout
                        .unwrap_or_else(GenericStateUpdate::timeout),
                );
                self.publish_command_state(new_state).await
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
                self.publish_command_state(new_state).await
            }
            OperationAction::BgScript(script, handlers) => {
                let next_state = &handlers.on_exec.status;
                info!(
                    "Moving {operation} operation to {next_state} state before running: {script}"
                );
                let new_state = state.update(handlers.on_exec);
                self.publish_command_state(new_state).await?;

                // Run the command, but ignore its result
                let command = Execute::new(script.command, script.args);
                let output = self.script_runner.await_response(command).await?;
                log_file.log_script_output(&output).await;
                Ok(())
            }
        }
    }

    async fn process_internal_operation(
        &mut self,
        target: EntityTopicId,
        operation: OperationType,
        cmd_id: String,
        message: serde_json::Value,
    ) -> Result<(), RuntimeError> {
        match operation {
            OperationType::SoftwareList => {
                match SoftwareListCommand::try_from_json(target, cmd_id, message) {
                    Ok(cmd) => {
                        self.software_sender.send(cmd.into()).await?;
                    }
                    Err(err) => error!("Incorrect software_list request payload: {err}"),
                }
            }

            OperationType::SoftwareUpdate => {
                match SoftwareUpdateCommand::try_from_json(target, cmd_id, message) {
                    Ok(cmd) => {
                        self.software_sender.send(cmd.into()).await?;
                    }
                    Err(err) => error!("Incorrect software_update request payload: {err}"),
                }
            }

            OperationType::Restart => {
                match RestartCommand::try_from_json(target, cmd_id, message) {
                    Ok(cmd) => {
                        self.restart_sender.send(cmd).await?;
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
        let new_state = response.into_generic_command(&self.mqtt_schema);
        self.publish_command_state(new_state).await
    }

    async fn process_software_update_response(
        &mut self,
        response: SoftwareUpdateCommand,
    ) -> Result<(), RuntimeError> {
        let new_state = response.into_generic_command(&self.mqtt_schema);
        self.publish_command_state(new_state).await
    }

    async fn process_restart_response(
        &mut self,
        response: RestartCommand,
    ) -> Result<(), RuntimeError> {
        let new_state = match response.resume_context() {
            None => response.into_generic_command(&self.mqtt_schema),
            Some(context) => context,
        };
        self.publish_command_state(new_state).await
    }

    async fn publish_command_state(
        &mut self,
        new_state: GenericCommandState,
    ) -> Result<(), RuntimeError> {
        if let Err(err) = self.workflows.apply_internal_update(new_state.clone()) {
            error!("Fail to persist workflow operation state: {err}");
        }
        self.persist_command_board().await?;
        self.command_sender.send(new_state.clone()).await?;
        self.mqtt_publisher.send(new_state.into_message()).await?;
        Ok(())
    }

    /// Reload from disk the current state of the pending command requests
    async fn load_command_board(&mut self) -> Result<(), RuntimeError> {
        match self.state_repository.load().await {
            Ok(Some(pending_commands)) => {
                self.workflows.load_pending_commands(pending_commands);
                for (timestamp, command) in self.workflows.pending_commands().iter() {
                    if let Some(resumed_command) = self.workflows.resume_command(timestamp, command)
                    {
                        self.command_sender.send(resumed_command).await?;
                    }
                }
            }
            Ok(None) => {}
            Err(err) => {
                error!(
                    "Fail to reload pending command requests from {} due to: {}",
                    self.state_repository.state_repo_path, err
                );
            }
        }
        Ok(())
    }

    /// Persist on-disk the current state of the pending command requests
    async fn persist_command_board(&mut self) -> Result<(), RuntimeError> {
        let pending_commands = self.workflows.pending_commands();
        if let Err(err) = self.state_repository.store(pending_commands).await {
            error!(
                "Fail to persist pending command requests in {} due to: {}",
                self.state_repository.state_repo_path, err
            );
        }

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
