use crate::operation_workflows::message_box::CommandDispatcher;
use crate::operation_workflows::message_box::SyncSignalDispatcher;
use crate::operation_workflows::persist::WorkflowRepository;
use crate::state_repository::state::AgentStateRepository;
use crate::Capabilities;
use async_trait::async_trait;
use camino::Utf8PathBuf;
use log::error;
use log::info;
use log::warn;
use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
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
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicError;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::mqtt_topics::SignalType;
use tedge_api::workflow::extract_json_output;
use tedge_api::workflow::CommandBoard;
use tedge_api::workflow::CommandId;
use tedge_api::workflow::GenericCommandData;
use tedge_api::workflow::GenericCommandMetadata;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::GenericStateUpdate;
use tedge_api::workflow::OperationAction;
use tedge_api::workflow::OperationName;
use tedge_api::workflow::OperationStep;
use tedge_api::workflow::OperationStepRequest;
use tedge_api::workflow::OperationStepResponse;
use tedge_api::workflow::WorkflowExecutionError;
use tedge_api::CommandLog;
use tedge_downloader_ext::DownloadRequest;
use tedge_downloader_ext::DownloadResult;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_script_ext::Execute;
use tokio::time::sleep;

type DownloaderRequest = (String, DownloadRequest);
type DownloaderResult = (String, DownloadResult);

/// A generic command state that is published by the [TedgeOperationConverterActor]
/// to itself for further processing .i.e. after a state update
#[derive(Debug)]
pub struct InternalCommandState(GenericCommandState);

fan_in_message_type!(AgentInput[MqttMessage, InternalCommandState, GenericCommandData, FsWatchEvent] : Debug);

pub struct WorkflowActor {
    pub(crate) mqtt_schema: MqttSchema,
    pub(crate) device_topic_id: EntityTopicId,
    pub(crate) workflow_repository: WorkflowRepository,
    pub(crate) state_repository: AgentStateRepository<CommandBoard>,
    pub(crate) log_dir: Utf8PathBuf,
    pub(crate) capabilities: Capabilities,
    pub(crate) input_receiver: UnboundedLoggingReceiver<AgentInput>,
    pub(crate) builtin_command_dispatcher: CommandDispatcher,
    pub(crate) builtin_operation_step_executor: HashMap<
        (OperationType, OperationStep),
        ClientMessageBox<OperationStepRequest, OperationStepResponse>,
    >,
    pub(crate) sync_signal_dispatcher: SyncSignalDispatcher,
    pub(crate) command_sender: DynSender<InternalCommandState>,
    pub(crate) mqtt_publisher: LoggingSender<MqttMessage>,
    pub(crate) script_runner: ClientMessageBox<Execute, std::io::Result<Output>>,
    pub(crate) downloader: ClientMessageBox<DownloaderRequest, DownloaderResult>,
    pub(crate) tmp_dir: Utf8PathBuf,
}

#[async_trait]
impl Actor for WorkflowActor {
    fn name(&self) -> &str {
        "WorkflowActor"
    }

    async fn run(mut self) -> Result<(), RuntimeError> {
        self.workflow_repository.load().await;
        self.publish_operation_capabilities().await?;
        self.load_command_board().await?;

        while let Some(input) = self.input_receiver.recv().await {
            match input {
                AgentInput::MqttMessage(message) => {
                    self.process_mqtt_message(message).await?;
                }
                AgentInput::InternalCommandState(InternalCommandState(command_state)) => {
                    self.process_command_update(command_state).await?;
                }
                AgentInput::GenericCommandData(GenericCommandData::State(new_state)) => {
                    self.process_builtin_command_update(new_state).await?;
                }
                AgentInput::GenericCommandData(GenericCommandData::Metadata(
                    GenericCommandMetadata { operation, payload },
                )) => {
                    self.publish_builtin_capability(operation, payload).await?;
                }
                AgentInput::FsWatchEvent(file_update) => {
                    if let Some(updated_capability) = self
                        .workflow_repository
                        .update_operation_workflows(
                            &self.mqtt_schema,
                            &self.device_topic_id,
                            file_update,
                        )
                        .await
                    {
                        self.mqtt_publisher.send(updated_capability).await?
                    }
                }
            }
        }
        Ok(())
    }
}

impl WorkflowActor {
    async fn publish_operation_capabilities(&mut self) -> Result<(), RuntimeError> {
        for capability in self
            .workflow_repository
            .capability_messages(&self.mqtt_schema, &self.device_topic_id)
        {
            self.mqtt_publisher.send(capability).await?
        }
        Ok(())
    }

    async fn publish_builtin_capability(
        &mut self,
        operation: OperationName,
        payload: serde_json::Value,
    ) -> Result<(), RuntimeError> {
        let operation = operation.parse().unwrap();
        let meta_topic = self
            .mqtt_schema
            .capability_topic_for(&self.device_topic_id, operation);
        let message = MqttMessage::new(&meta_topic, payload.to_string())
            .with_retain()
            .with_qos(QoS::AtLeastOnce);
        self.mqtt_publisher.send(message).await?;
        Ok(())
    }

    /// Process a command update received from MQTT
    ///
    /// Beware, these updates are coming from external components (the mapper inits and clears commands),
    /// but also from *this* actor as all its state transitions are published over MQTT.
    /// Only the former will be actually processed with [Self::process_command_update].
    async fn process_mqtt_message(&mut self, message: MqttMessage) -> Result<(), RuntimeError> {
        let Ok((_, channel)) = self.mqtt_schema.entity_channel_of(&message.topic) else {
            log::error!("Unknown topic: {}", &message.topic.name);
            return Ok(());
        };
        match channel {
            Channel::Command { operation, cmd_id } => {
                self.process_command_message(message, operation, cmd_id)
                    .await
            }
            Channel::Signal { signal_type } => {
                self.process_signal_message(message, signal_type).await
            }
            _ => {
                error!("Unsupported channel: {}", channel);
                Ok(())
            }
        }
    }

    async fn process_signal_message(
        &mut self,
        _message: MqttMessage,
        signal_type: SignalType,
    ) -> Result<(), RuntimeError> {
        match signal_type {
            SignalType::Sync => {
                info!("Received sync signal, requesting all builtin actors to sync");
                self.sync_signal_dispatcher.sync_all().await?;
            }
            SignalType::SyncOperation(operation) => match operation {
                OperationType::ConfigSnapshot | OperationType::ConfigUpdate => {
                    warn!("Sync signal for sync_config_snapshot or sync_config_update isn't supported. Use 'sync_config' signal instead.");
                }
                operation => {
                    info!(
                        "Received sync signal for {}, requesting the corresponding actor to sync",
                        operation
                    );
                    self.sync_signal_dispatcher.sync(operation).await?;
                }
            },
            SignalType::Custom(signal) => {
                match signal.as_str() {
                    "sync_config" => {
                        info!("Received sync_config signal, requesting the builtin config actor to sync");
                        // Sending sync signal ConfigSnapshot but not ConfigUpdate is fine
                        // as the actor syncs the cmd metadata for both on receipt of either signal
                        self.sync_signal_dispatcher
                            .sync(OperationType::ConfigSnapshot)
                            .await?;
                    }
                    _ => {
                        // Custom signal types are not handled yet
                    }
                }
            }
        }
        Ok(())
    }

    async fn process_command_message(
        &mut self,
        message: MqttMessage,
        operation: OperationType,
        cmd_id: String,
    ) -> Result<(), RuntimeError> {
        if !self.is_operation_enabled(&operation) {
            info!("Ignoring {operation} operation because it is disabled in agent capabilities");
            return Ok(());
        }
        let Ok(state) = GenericCommandState::from_command_message(&message) else {
            log::error!("Invalid command payload: {}", &message.topic.name);
            return Ok(());
        };
        let step = state.status.clone();

        let mut log_file = self.open_command_log(&state, &operation, &cmd_id);

        match self
            .workflow_repository
            .apply_external_update(&operation, state)
            .await
        {
            Ok(None) => (),
            Ok(Some(new_state)) => {
                self.persist_command_board().await?;
                if new_state.is_init() {
                    self.process_command_update(new_state.with_log_path(&log_file.path))
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

    /// Process a command state update taking any action as defined by the workflow
    ///
    /// A new state can be received:
    /// - from MQTT as for init and clear messages
    /// - from the engine itself when a progress is made
    /// - from one of the builtin operation actors
    async fn process_command_update(
        &mut self,
        state: GenericCommandState,
    ) -> Result<(), RuntimeError> {
        let Ok((operation, cmd_id)) = self.extract_command_identifiers(&state.topic.name) else {
            log::error!("Unknown command channel: {}", state.topic.name);
            return Ok(());
        };
        if !self.is_operation_enabled(&operation) {
            info!("Ignoring {operation} operation because it is disabled in agent capabilities");
            return Ok(());
        }
        let mut log_file = self.open_command_log(&state, &operation, &cmd_id);

        let action = match self.workflow_repository.get_action(&state) {
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
                if let Some(invoking_command) =
                    self.workflow_repository.invoking_command_state(&state)
                {
                    log_file
                        .log_info(&format!(
                            "Resuming invoking command {}",
                            invoking_command.topic.as_ref()
                        ))
                        .await;
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
                self.publish_command_state(new_state, &mut log_file).await
            }
            OperationAction::BuiltIn(_, _) => {
                let step = &state.status;
                info!("Processing {operation} operation {step} step");

                Ok(self.builtin_command_dispatcher.send(state).await?)
            }
            OperationAction::BuiltInOperation(ref builtin_op, ref handlers) => {
                let step = &state.status;
                info!("Executing builtin:{builtin_op} operation {step} step");

                // Fork a builtin state
                let builtin_state = action.adapt_builtin_request(state.clone());

                // Move to the next state to await the builtin operation outcome
                let new_state = state.update(handlers.on_exec.clone());
                self.publish_command_state(new_state, &mut log_file).await?;

                // Forward the command to the builtin operation actor
                Ok(self.builtin_command_dispatcher.send(builtin_state).await?)
            }
            OperationAction::AwaitingAgentRestart(handlers) => {
                let step = &state.status;
                info!("{operation} operation {step} waiting for agent restart");
                // The following sleep is expected to be interrupted by a restart
                sleep(handlers.timeout.unwrap_or_default() + Duration::from_secs(60)).await;
                // As the sleep completes, it means the agent was not restarted
                // hence the operation is moved to its `on_timeout` target state
                let new_state = state.update(handlers.on_timeout);
                self.publish_command_state(new_state, &mut log_file).await
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
                self.publish_command_state(new_state, &mut log_file).await
            }
            OperationAction::BgScript(script, handlers) => {
                let next_state = &handlers.on_exec.status;
                info!(
                    "Moving {operation} operation to {next_state} state before running: {script}"
                );
                let new_state = state.update(handlers.on_exec);
                self.publish_command_state(new_state, &mut log_file).await?;

                // Run the command, but ignore its result
                let command = Execute::new(script.command, script.args);
                let output = self.script_runner.await_response(command).await?;
                log_file.log_script_output(&output).await;
                Ok(())
            }
            OperationAction::Download(input_excerpt, handlers) => {
                let step = &state.status;
                info!("Processing {operation} operation {step} step with download builtin action");

                let input = input_excerpt.extract_value_from(&state);
                let (url, url_source) =
                    if let Some(url) = GenericCommandState::extract_text_property(&input, "url") {
                        (url, "input.url")
                    } else if let Some(url) = state.get_text_property("tedgeUrl") {
                        (url, "tedgeUrl")
                    } else if let Some(url) = state.get_text_property("remoteUrl") {
                        (url, "remoteUrl")
                    } else {
                        let err_state = state
                            .update_with_builtin_action_result(
                                "download",
                                Err("No valid URL found in input.url, tedgeUrl, or remoteUrl"
                                    .to_string()),
                                handlers,
                                &mut log_file,
                            )
                            .await;
                        return self.publish_command_state(err_state, &mut log_file).await;
                    };

                log_file
                    .log_info(&format!("Using URL from {}: {}", url_source, url))
                    .await;

                let temp_filename = format!("{operation}_{cmd_id}");
                let temp_path = self.tmp_dir.join(&temp_filename);

                let download_request = DownloadRequest::new(url, temp_path.as_std_path());
                let (_topic, download_result) = self
                    .downloader
                    .await_response((state.topic.name.clone(), download_request))
                    .await?;

                let result = match download_result {
                    Ok(download_response) => {
                        let downloaded_path = download_response.file_path;
                        log_file
                            .log_info(&format!("Downloaded to: {}", downloaded_path.display()))
                            .await;

                        Ok(json!({"downloadedPath": downloaded_path}))
                    }
                    Err(err) => Err(format!("Download failed: {}", err)),
                };
                let new_state = state
                    .update_with_builtin_action_result("download", result, handlers, &mut log_file)
                    .await;
                self.publish_command_state(new_state, &mut log_file).await
            }
            OperationAction::BuiltInOperationStep(
                operation_name,
                operation_step,
                input_excerpt,
                handlers,
            ) => {
                let action = format!("builtin:{operation_name}:{operation_step}");
                let input = input_excerpt.extract_value_from(&state);
                let state = state.update_with_json(input);
                let step_request = OperationStepRequest {
                    command_step: operation_step.clone(),
                    command_state: state.clone(),
                };

                info!("Processing builtin operation step: {action}");

                let operation_type: OperationType = operation_name.as_str().into();
                let result = if let Some(handle) = self
                    .builtin_operation_step_executor
                    .get_mut(&(operation_type, operation_step.clone()))
                {
                    handle
                        .await_response(step_request)
                        .await?
                        .map(|opt_value| opt_value.unwrap_or(Value::Null))
                } else {
                    Err(format!(
                        "No builtin operation step handler registered for {operation} operation {operation_step} step"
                    ))
                };

                let new_state = state
                    .update_with_builtin_action_result(&action, result, handlers, &mut log_file)
                    .await;
                self.publish_command_state(new_state, &mut log_file).await
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
                                self.publish_command_state(err_state, &mut log_file).await?;
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
                self.publish_command_state(new_state, &mut log_file).await?;

                // Finally, init the sub-operation
                self.mqtt_publisher
                    .send(sub_cmd_init_state.into_message())
                    .await?;
                Ok(())
            }
            OperationAction::AwaitOperationCompletion(handlers, output_excerpt) => {
                let step = &state.status;
                info!("{operation} operation {step}: waiting for sub-operation completion");

                // Get the sub-operation state and resume this command when the sub-operation is in a terminal state
                if let Some(sub_state) = self
                    .workflow_repository
                    .sub_command_state(&state)
                    .map(|s| s.to_owned())
                {
                    let sub_operation = sub_state.operation().unwrap_or_default();
                    if sub_state.is_finished() {
                        let new_state = if sub_state.is_successful() {
                            log_file
                                .log_info(&format!(
                                    "=> {sub_operation} sub-operation is successful"
                                ))
                                .await;
                            let sub_cmd_output = output_excerpt.extract_value_from(&sub_state);
                            state
                                .update_with_json(sub_cmd_output)
                                .update(handlers.on_success)
                        } else {
                            log_file
                                .log_info(&format!(
                                    "=> {sub_operation} sub-operation failed: {}",
                                    sub_state.failure_reason().unwrap_or_default()
                                ))
                                .await;
                            state.update(handlers.on_error)
                        };
                        self.publish_command_state(new_state, &mut log_file).await?;
                        self.publish_command_state(sub_state.clear(), &mut log_file)
                            .await?;
                    } else {
                        // Nothing specific has to be done: the current state has been persisted
                        // and will be resumed on completion of the sub-operation
                        // TODO: Register a timeout event
                        log_file
                            .log_info(&format!(
                                "=> {sub_operation} sub-operation is still running"
                            ))
                            .await;
                    }
                };

                Ok(())
            }
            OperationAction::Iterate(target_json_path, handlers) => {
                match OperationAction::process_iterate(
                    state.clone(),
                    &target_json_path,
                    handlers.clone(),
                ) {
                    Ok(next_state) => {
                        self.publish_command_state(next_state, &mut log_file)
                            .await?
                    }
                    Err(err) => {
                        error!("Iteration failed due to: {err}");
                        let new_state = state.update(handlers.on_error);
                        self.publish_command_state(new_state, &mut log_file).await?;
                    }
                }
                Ok(())
            }
        }
    }

    /// Pre-process an update received from a builtin operation actor
    ///
    /// The actual work will be done by [Self::process_command_update].
    async fn process_builtin_command_update(
        &mut self,
        new_state: GenericCommandState,
    ) -> Result<(), RuntimeError> {
        if new_state.is_finished() {
            self.sync_listener_actors(&new_state).await?;
            self.finalize_builtin_command_update(new_state).await?;

            Ok(())
        } else {
            // As not finalized, the builtin state is sent back
            // to the builtin operation actor for further processing.
            let builtin_state = new_state.clone();
            Ok(self.builtin_command_dispatcher.send(builtin_state).await?)
        }
    }

    /// Finalize a builtin operation
    ///
    /// Moving to the next step calling [Self::process_command_update].
    async fn finalize_builtin_command_update(
        &mut self,
        new_state: GenericCommandState,
    ) -> Result<(), RuntimeError> {
        let adapted_state = self.workflow_repository.adapt_builtin_response(new_state);
        if let Err(err) = self
            .workflow_repository
            .apply_internal_update(adapted_state.clone())
        {
            error!("Fail to persist workflow operation state: {err}");
        }
        self.persist_command_board().await?;
        self.mqtt_publisher
            .send(adapted_state.clone().into_message())
            .await?;
        self.process_command_update(adapted_state).await
    }

    async fn sync_listener_actors(
        &mut self,
        command: &GenericCommandState,
    ) -> Result<(), RuntimeError> {
        if let Some(command) = command.operation() {
            self.sync_signal_dispatcher
                .sync_listener(command.as_str().into())
                .await?;
        }
        Ok(())
    }

    fn open_command_log(
        &mut self,
        state: &GenericCommandState,
        operation: &OperationType,
        cmd_id: &str,
    ) -> CommandLog {
        let (root_operation, root_cmd_id) = match self
            .workflow_repository
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

    async fn publish_command_state(
        &mut self,
        new_state: GenericCommandState,
        log_file: &mut CommandLog,
    ) -> Result<(), RuntimeError> {
        if let Err(err) = self
            .workflow_repository
            .apply_internal_update(new_state.clone())
        {
            error!("Fail to persist workflow operation state: {err}");
        }
        self.persist_command_board().await?;
        if !new_state.is_cleared() {
            log_file.log_next_step(&new_state.status).await;
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
                for command in self
                    .workflow_repository
                    .load_pending_commands(pending_commands)
                    .await
                {
                    // Make sure the latest state is visible over MQTT
                    self.mqtt_publisher
                        .send(command.clone().into_message())
                        .await?;
                    self.process_command_update(command.clone()).await?;
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
        let pending_commands = self.workflow_repository.pending_commands();
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

    fn is_operation_enabled(&self, operation: &OperationType) -> bool {
        match operation {
            OperationType::ConfigUpdate => self.capabilities.config_update,
            OperationType::ConfigSnapshot => self.capabilities.config_snapshot,
            OperationType::LogUpload => self.capabilities.log_upload,
            _ => true,
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
