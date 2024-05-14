use crate::software_manager::actor::SoftwareCommand;
use crate::state_repository::state::AgentStateRepository;
use crate::tedge_operation_converter::message_box::CommandDispatcher;
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
use tedge_api::mqtt_topics::EntityTopicError;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::extract_json_output;
use tedge_api::workflow::CommandBoard;
use tedge_api::workflow::CommandId;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::GenericStateUpdate;
use tedge_api::workflow::OperationAction;
use tedge_api::workflow::OperationName;
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

/// A generic command state that is published by the [TedgeOperationConverterActor]
/// to itself for further processing .i.e. after a state update
#[derive(Debug)]
pub struct InternalCommandState(GenericCommandState);

fan_in_message_type!(AgentInput[MqttMessage, InternalCommandState, SoftwareCommand, RestartCommand] : Debug);

pub struct TedgeOperationConverterActor {
    pub(crate) mqtt_schema: MqttSchema,
    pub(crate) device_topic_id: EntityTopicId,
    pub(crate) workflows: WorkflowSupervisor,
    pub(crate) state_repository: AgentStateRepository<CommandBoard>,
    pub(crate) log_dir: Utf8PathBuf,
    pub(crate) input_receiver: UnboundedLoggingReceiver<AgentInput>,
    pub(crate) command_dispatcher: CommandDispatcher,
    pub(crate) command_sender: DynSender<InternalCommandState>,
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
                AgentInput::InternalCommandState(InternalCommandState(command_state)) => {
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
        let Ok((operation, cmd_id)) = self.extract_command_identifiers(&message.topic.name) else {
            log::error!("Unknown command channel: {}", &message.topic.name);
            return Ok(());
        };

        let Ok(state) = GenericCommandState::from_command_message(&message) else {
            log::error!("Invalid command payload: {}", &message.topic.name);
            return Ok(());
        };
        let step = state.status.clone();

        let mut log_file = self.open_command_log(&state, &operation, &cmd_id);

        match self.workflows.apply_external_update(&operation, state) {
            Ok(None) => (),
            Ok(Some(new_state)) => {
                self.persist_command_board().await?;
                if new_state.is_init() {
                    self.process_command_state_update(new_state.set_log_path(&log_file.path))
                        .await?;
                }
            }
            Err(WorkflowExecutionError::UnknownOperation { operation }) => {
                info!("Ignoring {operation} operation which is not registered");
            }
            Err(err) => {
                error!("{operation} operation request cannot be processed: {err}");
                log_file.log_step(&step, &format!("Error: {err}\n")).await;
            }
        }

        Ok(())
    }

    async fn process_command_state_update(
        &mut self,
        state: GenericCommandState,
    ) -> Result<(), RuntimeError> {
        let Ok((operation, cmd_id)) = self.extract_command_identifiers(&state.topic.name) else {
            log::error!("Unknown command channel: {}", state.topic.name);
            return Ok(());
        };
        let mut log_file = self.open_command_log(&state, &operation, &cmd_id);

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
                    .log_step(&state.status, &format!("Error: {err}\n"))
                    .await;
                return Ok(());
            }
        };

        log_file.log_state_action(&state, &action).await;

        match action {
            OperationAction::Clear => {
                if let Some(invoking_command) = self.workflows.invoking_command_state(&state) {
                    info!(
                        "Resuming invoking command {}",
                        invoking_command.topic.as_ref()
                    );
                    self.command_sender
                        .send(InternalCommandState(invoking_command.clone()))
                        .await?;
                } else {
                    info!(
                        "Waiting {} {operation} operation to be cleared",
                        state.status
                    );
                }
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
                Ok(self.command_dispatcher.send(state).await?)
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
            OperationAction::Operation(sub_operation, input_script, input_excerpt, handlers) => {
                let next_state = &handlers.on_exec.status;
                info!(
                    "Triggering {sub_operation} command, and moving {operation} operation to {next_state} state"
                );

                // Run the input script, if any, to generate the init state of the sub-operation
                let generated_init_state = match input_script {
                    None => GenericStateUpdate::empty_payload(),
                    Some(script) => {
                        let command = Execute::new(script.command.clone(), script.args);
                        let output = self.script_runner.await_response(command).await?;
                        log_file.log_script_output(&output).await;
                        match extract_json_output(&script.command, output) {
                            Ok(init_state) => init_state,
                            Err(reason) => {
                                let err_state = state.update(GenericStateUpdate::failed(reason));
                                self.publish_command_state(err_state).await?;
                                return Ok(());
                            }
                        }
                    }
                };

                // Create the sub-operation init state with a reference to its invoking command
                let sub_cmd_input = input_excerpt.extract_value_from(&state);
                let sub_cmd_init_state = GenericCommandState::sub_command_init_state(
                    &self.mqtt_schema,
                    &self.device_topic_id,
                    operation,
                    cmd_id,
                    sub_operation,
                )
                .update_with_json(generated_init_state)
                .update_with_json(sub_cmd_input)
                .update_with_json(GenericStateUpdate::init_payload());

                // Persist the new state for this command
                let new_state = state.update(handlers.on_exec);
                self.publish_command_state(new_state).await?;

                // Finally, init the sub-operation
                self.mqtt_publisher
                    .send(sub_cmd_init_state.into_message())
                    .await?;
                Ok(())
            }
            OperationAction::AwaitOperationCompletion(handlers, output_excerpt) => {
                let step = &state.status;
                info!("{operation} operation {step} waiting for sub-operation completion");

                // Get the sub-operation state and resume this command when the sub-operation is in a terminal state
                if let Some(sub_state) = self
                    .workflows
                    .sub_command_state(&state)
                    .map(|s| s.to_owned())
                {
                    if sub_state.is_successful() {
                        let sub_cmd_output = output_excerpt.extract_value_from(&sub_state);
                        let new_state = state
                            .update_with_json(sub_cmd_output)
                            .update(handlers.on_success);
                        self.publish_command_state(new_state).await?;
                        self.publish_command_state(sub_state.clear()).await?;
                    } else if sub_state.is_failed() {
                        let new_state = state.update(handlers.on_error.unwrap_or_else(|| {
                            GenericStateUpdate::failed("sub-operation failed".to_string())
                        }));
                        self.publish_command_state(new_state).await?;
                        self.publish_command_state(sub_state.clear()).await?;
                    } else {
                        // Nothing specific has to be done: the current state has been persisted
                        // and will be resumed on completion of the sub-operation
                        // TODO: Register a timeout event
                    }
                };

                Ok(())
            }
        }
    }

    fn open_command_log(
        &mut self,
        state: &GenericCommandState,
        operation: &OperationType,
        cmd_id: &str,
    ) -> CommandLog {
        let (root_operation, root_cmd_id) = match self
            .workflows
            .root_invoking_command_state(state)
            .map(|s| s.topic.as_ref())
            .and_then(|root_topic| self.extract_command_identifiers(root_topic).ok())
        {
            None => (None, None),
            Some((op, id)) => (Some(op.to_string()), Some(id)),
        };

        CommandLog::new(
            self.log_dir.clone(),
            operation.to_string(),
            cmd_id.to_string(),
            state.invoking_operation_names(),
            root_operation,
            root_cmd_id,
        )
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
        let new_state = response.into_generic_command(&self.mqtt_schema);
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
        if !new_state.is_cleared() {
            self.command_sender
                .send(InternalCommandState(new_state.clone()))
                .await?;
        }
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
                        self.command_sender
                            .send(InternalCommandState(resumed_command))
                            .await?;
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

    fn extract_command_identifiers(
        &self,
        topic: impl AsRef<str>,
    ) -> Result<(OperationType, CommandId), CommandTopicError> {
        let (_, channel) = self.mqtt_schema.entity_channel_of(topic)?;
        match channel {
            Channel::Command { operation, cmd_id } => Ok((operation, cmd_id)),
            _ => Err(CommandTopicError::InvalidCommandTopic),
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum CommandTopicError {
    #[error(transparent)]
    InvalidTopic(#[from] EntityTopicError),

    #[error("Not a command topic")]
    InvalidCommandTopic,
}

/// Log all command steps
struct CommandLog {
    /// Path to the command log file
    path: Utf8PathBuf,

    /// operation name
    operation: OperationName,

    /// the chain of operations leading to this command
    invoking_operations: Vec<OperationName>,

    /// command id
    cmd_id: CommandId,

    /// The log file of the root command invoking this command
    ///
    /// None, if not open yet.
    file: Option<File>,
}

impl CommandLog {
    pub fn new(
        log_dir: Utf8PathBuf,
        operation: OperationName,
        cmd_id: CommandId,
        invoking_operations: Vec<OperationName>,
        root_operation: Option<OperationName>,
        root_cmd_id: Option<CommandId>,
    ) -> Self {
        let root_operation = root_operation.unwrap_or(operation.clone());
        let root_cmd_id = root_cmd_id.unwrap_or(cmd_id.clone());

        let path = log_dir.join(format!("workflow-{}-{}.log", root_operation, root_cmd_id));
        CommandLog {
            path,
            operation,
            invoking_operations,
            cmd_id,
            file: None,
        }
    }

    async fn open(&mut self) -> Result<&mut File, std::io::Error> {
        if self.file.is_none() {
            self.file = Some(
                File::options()
                    .append(true)
                    .create(true)
                    .open(self.path.clone())
                    .await?,
            );
        }
        Ok(self.file.as_mut().unwrap())
    }

    async fn log_header(&mut self, topic: &str) {
        let now = OffsetDateTime::now_utc()
            .format(&format_description::well_known::Rfc3339)
            .unwrap();
        let cmd_id = &self.cmd_id;
        let operation = &self.operation;
        let header_message = format!(
            r#"
==================================================================
Triggered {operation} workflow
==================================================================

topic:     {topic}
operation: {operation}
cmd_id:    {cmd_id}
time:      {now}

==================================================================
"#
        );
        if let Err(err) = self.write(&header_message).await {
            error!("Fail to log to {}: {err}", self.path)
        }
    }

    async fn log_state_action(&mut self, state: &GenericCommandState, action: &OperationAction) {
        if state.is_init() && self.invoking_operations.is_empty() {
            self.log_header(state.topic.name.as_str()).await;
        }
        let step = &state.status;
        let state = &state.payload.to_string();
        let message = format!(
            r#"
State:    {state}

Action:   {action}
"#
        );
        self.log_step(step, &message).await
    }

    async fn log_step(&mut self, step: &str, action: &str) {
        let now = OffsetDateTime::now_utc()
            .format(&format_description::well_known::Rfc3339)
            .unwrap();
        let operation = &self.operation;
        let parent_operation = if self.invoking_operations.is_empty() {
            operation.to_string()
        } else {
            format!("{} > {}", self.invoking_operations.join(" > "), operation)
        };

        let message = format!(
            r#"
----------------------[ {parent_operation} @ {step} | time={now} ]----------------------
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
        let file = self.open().await?;
        logged_command::LoggedCommand::log_outcome("", result, file).await?;
        file.sync_all().await?;
        Ok(())
    }

    async fn write(&mut self, message: &str) -> Result<(), std::io::Error> {
        let file = self.open().await?;
        file.write_all(message.as_bytes()).await?;
        file.flush().await?;
        file.sync_all().await?;
        Ok(())
    }
}
