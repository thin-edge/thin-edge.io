use crate::workflow::*;
use log::info;
use on_disk::OnDiskCommandBoard;
use serde::Serialize;

/// Dispatch actions to operation participants
#[derive(Default)]
pub struct WorkflowSupervisor {
    /// The user-defined operation workflow definitions
    workflows: HashMap<OperationType, OperationWorkflow>,

    /// Operation instances under execution
    commands: CommandBoard,
}

impl WorkflowSupervisor {
    /// Register a builtin workflow provided by thin-edge
    pub fn register_builtin_workflow(
        &mut self,
        operation: OperationType,
    ) -> Result<(), WorkflowRegistrationError> {
        self.register_custom_workflow(OperationWorkflow::built_in(operation))
    }

    /// Register a user-defined workflow
    pub fn register_custom_workflow(
        &mut self,
        workflow: OperationWorkflow,
    ) -> Result<(), WorkflowRegistrationError> {
        if let Some(previous) = self.workflows.get(&workflow.operation) {
            if previous.built_in == workflow.built_in {
                return Err(WorkflowRegistrationError::DuplicatedWorkflow {
                    operation: workflow.operation.to_string(),
                });
            }

            info!(
                "The built-in {} operation has been customized",
                workflow.operation
            );
            if workflow.built_in {
                return Ok(());
            }
        }
        self.workflows.insert(workflow.operation.clone(), workflow);
        Ok(())
    }

    /// The set of pending commands
    pub fn pending_commands(&self) -> &CommandBoard {
        &self.commands
    }

    /// The set of pending commands
    pub fn load_pending_commands(&mut self, commands: CommandBoard) {
        self.commands = commands
    }

    /// List the capabilities provided by the registered workflows
    pub fn capability_messages(
        &self,
        schema: &MqttSchema,
        target: &EntityTopicId,
    ) -> Vec<MqttMessage> {
        // To ease testing the capability messages are emitted in a deterministic order
        let mut operations = self.workflows.values().collect::<Vec<_>>();
        operations.sort_by(|&a, &b| a.operation.to_string().cmp(&b.operation.to_string()));
        operations
            .iter()
            .filter_map(|workflow| workflow.capability_message(schema, target))
            .collect()
    }

    /// Update the state of the command board on reception of a message sent by a peer over MQTT
    ///
    /// Return the new CommandRequest state if any.
    pub fn apply_external_update(
        &mut self,
        operation: &OperationType,
        message: &MqttMessage,
    ) -> Result<Option<GenericCommandState>, WorkflowExecutionError> {
        if !self.workflows.contains_key(operation) {
            return Err(WorkflowExecutionError::UnknownOperation {
                operation: operation.to_string(),
            });
        };
        match GenericCommandState::from_command_message(message)? {
            None => {
                // The command has been cleared
                self.commands.remove(&message.topic.name);
                Ok(None)
            }
            Some(command_state) if command_state.status == "init" => {
                // This is a new command request
                self.commands.insert(command_state.clone())?;
                Ok(Some(command_state))
            }
            Some(_) => {
                // Ignore command updates published over MQTT
                //
                // TODO: There is exception here - not implemented yet:
                //       when a step is delegated to an external process,
                //       this process will notify the outcome of its action over MQTT,
                //       and the agent will have then to react on this message.
                Ok(None)
            }
        }
    }

    /// Return the action to be performed on a given command state
    pub fn get_action(
        &self,
        command_state: &GenericCommandState,
    ) -> Result<OperationAction, WorkflowExecutionError> {
        let Some(operation_name) = command_state.operation() else {
            return Err(WorkflowExecutionError::InvalidCmdTopic {
                topic: command_state.topic.name.clone(),
            });
        };

        self.workflows
            .get(&operation_name.as_str().into())
            .ok_or(WorkflowExecutionError::UnknownOperation {
                operation: operation_name,
            })
            .and_then(|workflow| workflow.get_action(command_state))
    }

    /// Update the state of the command board on reception of new state for a command
    ///
    /// Return the next CommandRequest state if any is required.
    pub fn apply_internal_update(
        &mut self,
        new_command_state: GenericCommandState,
    ) -> Result<(), WorkflowExecutionError> {
        self.commands.update(new_command_state)
    }

    /// Resume the given command when the agent is restarting after an interruption
    pub fn resume_command(
        &self,
        _timestamp: &Timestamp,
        command: &GenericCommandState,
    ) -> Option<GenericCommandState> {
        let Ok(action) = self.get_action(command) else {
            return None;
        };

        match action {
            OperationAction::AwaitingAgentRestart(handlers) => {
                Some(command.clone().update(handlers.on_success))
            }

            _ => {
                // TODO: Use the timestamp to filter out action pending since too long
                Some(command.clone())
            }
        }
    }
}

/// A view of all the operation instances under execution.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "OnDiskCommandBoard", into = "OnDiskCommandBoard")]
pub struct CommandBoard {
    /// For each command instance (uniquely identified by its cmd topic):
    /// - the full state of the command
    /// - a timestamp marking since when the command request is in this state
    ///
    /// TODO: use the timestamp to mark faulty any request making no progress
    #[serde(flatten)]
    commands: HashMap<TopicName, (Timestamp, GenericCommandState)>,
}

pub type TopicName = String;
pub type Timestamp = time::OffsetDateTime;

impl CommandBoard {
    pub fn new(commands: HashMap<TopicName, (Timestamp, GenericCommandState)>) -> Self {
        CommandBoard { commands }
    }

    /// Iterate over the pending commands
    pub fn iter(&self) -> impl Iterator<Item = &(Timestamp, GenericCommandState)> {
        self.commands.values()
    }

    /// Insert a new operation request into the [CommandBoard]
    ///
    /// Reject the request if there is already an entry with the same command id, but in a different state
    pub fn insert(
        &mut self,
        new_command: GenericCommandState,
    ) -> Result<(), WorkflowExecutionError> {
        match self.commands.get(&new_command.topic.name) {
            Some((_, command)) if command == &new_command => Ok(()),
            Some(_) => Err(WorkflowExecutionError::DuplicatedRequest {
                topic: new_command.topic.name,
            }),
            None => {
                let timestamp = time::OffsetDateTime::now_utc();
                self.commands
                    .insert(new_command.topic.name.clone(), (timestamp, new_command));
                Ok(())
            }
        }
    }

    /// Update the current state of an operation request
    ///
    /// Reject the update if the command has never been inserted
    pub fn update(
        &mut self,
        updated_command: GenericCommandState,
    ) -> Result<(), WorkflowExecutionError> {
        match self.commands.get_mut(&updated_command.topic.name) {
            None => Err(WorkflowExecutionError::UnknownRequest {
                topic: updated_command.topic.name,
            }),
            Some((timestamp, command_state)) => {
                *timestamp = time::OffsetDateTime::now_utc();
                *command_state = updated_command;
                Ok(())
            }
        }
    }

    /// Remove from the board an operation request
    pub fn remove(&mut self, topic_name: &String) {
        self.commands.remove(topic_name);
    }
}
