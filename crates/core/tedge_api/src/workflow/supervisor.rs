use crate::mqtt_topics::Channel;
use crate::workflow::*;
use ::log::info;
use on_disk::OnDiskCommandBoard;
use serde::Serialize;

/// Dispatch actions to operation participants
#[derive(Default)]
pub struct WorkflowSupervisor {
    /// The user-defined operation workflow definitions
    workflows: HashMap<OperationType, WorkflowVersions>,

    /// Operation instances under execution
    commands: CommandBoard,
}

impl WorkflowSupervisor {
    /// Register a builtin workflow provided by thin-edge
    pub fn register_builtin_workflow(
        &mut self,
        operation: OperationType,
    ) -> Result<(), WorkflowRegistrationError> {
        self.register_custom_workflow(
            WorkflowSource::BuiltIn,
            OperationWorkflow::built_in(operation),
            "builtin".to_string(),
        )
    }

    /// Register a user-defined workflow
    pub fn register_custom_workflow(
        &mut self,
        source: WorkflowSource,
        workflow: OperationWorkflow,
        version: WorkflowVersion,
    ) -> Result<(), WorkflowRegistrationError> {
        let operation = workflow.operation.clone();
        if let Some(versions) = self.workflows.get_mut(&operation) {
            versions.add(source, version, workflow);
        } else {
            let versions = WorkflowVersions::new(source, version, workflow);
            self.workflows.insert(operation, versions);
        }
        Ok(())
    }

    /// Un-register a user-defined workflow
    ///
    /// Return None is this was the last version for that operation.
    /// Return Some(BuiltIn) is there is a builtin definition
    /// Return Some(InUseCopy) if the workflow has been deprecated but there is still a running command.
    pub fn unregister_custom_workflow(
        &mut self,
        operation: &OperationName,
        version: &WorkflowVersion,
    ) -> Option<WorkflowSource> {
        let operation = OperationType::from(operation.as_str());
        if let Some(versions) = self.workflows.get_mut(&operation) {
            versions.remove(version);
        }

        let current_source = match self.workflows.get(&operation) {
            None => None,
            Some(version) if version.is_empty() => None,
            Some(version) if version.is_builtin() => Some(BuiltIn),
            Some(_) => Some(InUseCopy),
        };

        if current_source.is_none() {
            self.workflows.remove(&operation);
        }

        current_source
    }

    /// The set of pending commands
    pub fn pending_commands(&self) -> &CommandBoard {
        &self.commands
    }

    /// Update on start the set of pending commands
    pub fn load_pending_commands(&mut self, commands: CommandBoard) -> Vec<GenericCommandState> {
        self.commands = commands;
        self.commands
            .iter()
            .filter_map(|(t, s)| self.resume_command(t, s))
            .collect()
    }

    /// List the capabilities provided by the registered workflows
    pub fn capability_messages(
        &self,
        schema: &MqttSchema,
        target: &EntityTopicId,
    ) -> Vec<MqttMessage> {
        // To ease testing the capability messages are emitted in a deterministic order
        let mut operations = self
            .workflows
            .values()
            .filter_map(|versions| versions.current_workflow())
            .collect::<Vec<_>>();
        operations.sort_by(|&a, &b| a.operation.to_string().cmp(&b.operation.to_string()));
        operations
            .iter()
            .filter_map(|workflow| workflow.capability_message(schema, target))
            .collect()
    }

    pub fn capability_message(
        &self,
        schema: &MqttSchema,
        target: &EntityTopicId,
        operation: &OperationName,
    ) -> Option<MqttMessage> {
        let operation = OperationType::from(operation.as_str());
        self.workflows
            .get(&operation)
            .and_then(|versions| versions.current_workflow())
            .and_then(|workflow| workflow.capability_message(schema, target))
    }

    pub fn deregistration_message(
        &self,
        schema: &MqttSchema,
        target: &EntityTopicId,
        operation: &OperationName,
    ) -> MqttMessage {
        let operation = OperationType::from(operation.as_str());
        let topic = schema.topic_for(target, &Channel::CommandMetadata { operation });
        MqttMessage {
            topic,
            payload: "".to_string().into(),
            qos: QoS::AtLeastOnce,
            retain: true,
        }
    }

    /// Update the state of the command board on reception of a message sent by a peer over MQTT
    ///
    /// Return the new CommandRequest state if any.
    pub fn apply_external_update(
        &mut self,
        operation: &OperationType,
        command_state: GenericCommandState,
    ) -> Result<Option<GenericCommandState>, WorkflowExecutionError> {
        let Some(workflow_versions) = self.workflows.get_mut(operation) else {
            return Err(WorkflowExecutionError::UnknownOperation {
                operation: operation.to_string(),
            });
        };
        if command_state.is_cleared() {
            // The command has been cleared
            self.commands.remove(&command_state.topic.name);
            Ok(Some(command_state))
        } else if command_state.is_init() {
            // This is a new command request
            if let Some(current_version) = workflow_versions.use_current_version() {
                let command_state = command_state.set_workflow_version(current_version);
                self.commands.insert(command_state.clone())?;
                Ok(Some(command_state))
            } else {
                return Err(WorkflowExecutionError::DeprecatedOperation {
                    operation: operation.to_string(),
                });
            }
        } else {
            // Ignore command updates published over MQTT
            //
            // TODO: There is one exception here - not implemented yet:
            //       when a step is delegated to an external process,
            //       this process will notify the outcome of its action over MQTT,
            //       and the agent will have then to react on this message.
            Ok(None)
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

        let version = command_state.workflow_version();
        self.workflows
            .get(&operation_name.as_str().into())
            .ok_or(WorkflowExecutionError::UnknownOperation {
                operation: operation_name.clone(),
            })
            .and_then(|versions| versions.get(version.as_ref()))
            .and_then(|workflow| workflow.get_action(command_state))
    }

    /// Return the current state of a command (identified by its topic)
    pub fn get_state(&self, command: &str) -> Option<&GenericCommandState> {
        self.commands.get_state(command).map(|(_, state)| state)
    }

    /// Rewrite the command state returned by a builtin operation actor
    ///
    /// Depending the operation is executing, successful or failed,
    /// set the new state using the user provided handlers
    ///
    /// This method also takes care of the fact that the builtin operations
    /// only return the state properties they care about.
    /// Hence the command state is merged into the persisted state of the command.
    ///
    /// Return the command state unchanged if there is an error or no appropriate handlers.
    pub fn adapt_builtin_response(
        &self,
        command_state: GenericCommandState,
    ) -> GenericCommandState {
        let command_id = &command_state.topic;
        if let Some(current_state) = self.get_state(command_id.as_ref()) {
            let new_state = command_state.merge_into(current_state.clone());
            if let Ok(current_action) = self.get_action(current_state) {
                return current_action.adapt_builtin_response(new_state);
            } else {
                return new_state;
            }
        };

        command_state
    }

    /// Return the state of the invoking command of a command, if any
    pub fn invoking_command_state(
        &self,
        sub_command: &GenericCommandState,
    ) -> Option<&GenericCommandState> {
        sub_command
            .invoking_command_topic()
            .and_then(|invoking_topic| self.get_state(invoking_topic))
    }

    /// Return the sub command of a command, if any
    pub fn sub_command_state(
        &self,
        command_state: &GenericCommandState,
    ) -> Option<&GenericCommandState> {
        self.commands
            .lookup_sub_command(command_state.command_topic())
    }

    /// Return the state of the root command which execution leads to the execution of a leaf-command
    ///
    /// Return None, if the given command is not a sub-command
    pub fn root_invoking_command_state(
        &self,
        leaf_command: &GenericCommandState,
    ) -> Option<&GenericCommandState> {
        let invoking_command = self.invoking_command_state(leaf_command)?;
        let root_command = self
            .root_invoking_command_state(invoking_command)
            .unwrap_or(invoking_command);
        Some(root_command)
    }

    /// Update the state of the command board on reception of new state for a command
    pub fn apply_internal_update(
        &mut self,
        new_command_state: GenericCommandState,
    ) -> Result<(), WorkflowExecutionError> {
        if new_command_state.is_cleared() {
            self.commands.remove(new_command_state.command_topic());
            Ok(())
        } else {
            self.commands.update(new_command_state)
        }
    }

    /// Resume the given command when the agent is restarting after an interruption
    fn resume_command(
        &self,
        timestamp: &Timestamp,
        command: &GenericCommandState,
    ) -> Option<GenericCommandState> {
        let Ok(action) = self.get_action(command) else {
            return None;
        };

        let epoch = format!("{}.{}", timestamp.unix_timestamp(), timestamp.millisecond());
        let command = command.clone().update_with_key_value("resumed_at", &epoch);
        match action {
            OperationAction::AwaitingAgentRestart(handlers) => {
                Some(command.update(handlers.on_success))
            }

            _ => {
                // TODO: Use the timestamp to filter out action pending since too long
                Some(command)
            }
        }
    }
}

/// A stack of known versions for a workflow
///
/// In practice, one might have 3 concurrent versions:
///
/// - The current version, i.e. the version to be used for new operation instances.
/// - One in-use version, that was used by some operation instance when the current version has been updated.
/// - The builtin version provided by the agent and which can be overridden by the users.
///
/// None of these versions are mandatory. For instance, an operation can have no builtin version,
/// no current version (because the user removed the definition file), but a version still in-use
/// (that has been started before the user deprecated the operation).
struct WorkflowVersions {
    operation: OperationName,
    current: Option<WorkflowVersion>,
    builtin: Option<WorkflowVersion>,
    in_use: Option<WorkflowVersion>,
    versions: HashMap<WorkflowVersion, OperationWorkflow>,
}

#[derive(Copy, Clone)]
pub enum WorkflowSource {
    BuiltIn,
    UserDefined,
    InUseCopy,
}

use WorkflowSource::*;

impl WorkflowVersions {
    fn new(source: WorkflowSource, version: WorkflowVersion, workflow: OperationWorkflow) -> Self {
        let operation = workflow.operation.to_string();

        let current = match source {
            BuiltIn | UserDefined => Some(version.clone()),
            InUseCopy => None,
        };
        let builtin = match source {
            BuiltIn => Some(version.clone()),
            UserDefined | InUseCopy => None,
        };
        let in_use = match source {
            InUseCopy => Some(version.clone()),
            UserDefined | BuiltIn => None,
        };
        let versions = HashMap::from([(version, workflow)]);
        WorkflowVersions {
            operation,
            current,
            builtin,
            in_use,
            versions,
        }
    }

    fn add(
        &mut self,
        source: WorkflowSource,
        version: WorkflowVersion,
        workflow: OperationWorkflow,
    ) {
        match source {
            BuiltIn => {
                self.builtin = Some(version.clone());

                if self.current.is_none() {
                    self.current = Some(version.clone());
                } else {
                    info!(
                        "The built-in {operation} operation has been customized",
                        operation = workflow.operation
                    );
                }
            }

            UserDefined => {
                self.current = Some(version.clone());
                if self.builtin.is_some() {
                    info!(
                        "The built-in {operation} operation has been customized",
                        operation = workflow.operation
                    );
                }
            }

            InUseCopy => self.in_use = Some(version.clone()),
        }

        self.versions.insert(version.clone(), workflow);
    }

    // Mark the current version as being in-use.
    fn use_current_version(&mut self) -> Option<&WorkflowVersion> {
        if self.current.is_some() && self.in_use != self.current {
            // remove the previous version in-use unless this is the builtin version
            if let Some(previous_version) = self.in_use.as_ref() {
                if Some(previous_version) != self.builtin.as_ref() {
                    self.versions.remove(previous_version);
                }
            }
            self.in_use.clone_from(&self.current);
        }
        self.current.as_ref()
    }

    // Remove a version from this list of versions, restoring the built-in version if any
    fn remove(&mut self, version: &WorkflowVersion) {
        self.versions.remove(version);
        self.current.clone_from(&self.builtin);
    }

    fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }

    fn is_builtin(&self) -> bool {
        self.builtin.is_some()
    }

    fn get(
        &self,
        version: Option<&WorkflowVersion>,
    ) -> Result<&OperationWorkflow, WorkflowExecutionError> {
        match version {
            None => self.current_workflow(),
            Some(version) => self.find(version),
        }
        .ok_or(WorkflowExecutionError::UnknownVersion {
            operation: self.operation.clone(),
            version: version
                .or(self.current.as_ref())
                .unwrap_or(&"current".to_string())
                .to_string(),
        })
    }

    fn current_workflow(&self) -> Option<&OperationWorkflow> {
        self.current
            .as_ref()
            .and_then(|version| self.versions.get(version))
    }

    fn find(&self, version: &WorkflowVersion) -> Option<&OperationWorkflow> {
        self.versions.get(version)
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

    pub fn get_state(&self, command: &str) -> Option<&(Timestamp, GenericCommandState)> {
        self.commands.get(command)
    }

    /// Return the sub command of a command, if any
    pub fn lookup_sub_command(&self, command_topic: &TopicName) -> Option<&GenericCommandState> {
        // Sequential search is okay because in practice there is no more than 10 concurrent commands
        self.commands
            .values()
            .find(|(_, command)| command.invoking_command_topic() == Some(command_topic))
            .map(|(_, command)| command)
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

#[cfg(test)]
mod tests {
    use super::*;
    use mqtt_channel::Topic;

    #[test]
    fn retrieve_invoking_command_hierarchy() {
        let mut workflows = WorkflowSupervisor::default();

        let level_1_op = OperationType::Custom("level_1".to_string());
        let level_2_op = OperationType::Custom("level_2".to_string());
        let level_3_op = OperationType::Custom("level_3".to_string());

        workflows
            .register_builtin_workflow(level_1_op.clone())
            .unwrap();
        workflows
            .register_builtin_workflow(level_2_op.clone())
            .unwrap();
        workflows
            .register_builtin_workflow(level_3_op.clone())
            .unwrap();

        // Start a level_1 operation
        let level_1_cmd = GenericCommandState::from_command_message(&MqttMessage::new(
            &Topic::new_unchecked("te/device/foo///cmd/level_1/id_1"),
            r#"{ "@version": "builtin", "status":"init" }"#,
        ))
        .unwrap();
        workflows
            .apply_external_update(&level_1_op, level_1_cmd.clone())
            .unwrap();

        // A level 1 command has no invoking command nor root invoking command
        assert!(workflows.invoking_command_state(&level_1_cmd).is_none());
        assert!(workflows
            .root_invoking_command_state(&level_1_cmd)
            .is_none());

        // Start a level_2 operation, sub-command of the previous level_1 command
        let level_2_cmd = GenericCommandState::from_command_message(&MqttMessage::new(
            &Topic::new_unchecked("te/device/foo///cmd/level_2/sub:level_1:id_1"),
            r#"{ "@version": "builtin", "status":"init" }"#,
        ))
        .unwrap();
        workflows
            .apply_external_update(&level_2_op, level_2_cmd.clone())
            .unwrap();

        // The invoking command of the level_2 command, is the previous level_1 command
        // The later is also the root invoking command
        assert_eq!(
            workflows.invoking_command_state(&level_2_cmd),
            Some(&level_1_cmd)
        );
        assert_eq!(
            workflows.root_invoking_command_state(&level_2_cmd),
            Some(&level_1_cmd)
        );

        // Start a level_3 operation, sub-command of the previous level_2 command
        let level_3_cmd = GenericCommandState::from_command_message(&MqttMessage::new(
            &Topic::new_unchecked("te/device/foo///cmd/level_3/sub:level_2:sub:level_1:id_1"),
            r#"{ "@version": "builtin", "status":"init" }"#,
        ))
        .unwrap();
        workflows
            .apply_external_update(&level_3_op, level_3_cmd.clone())
            .unwrap();

        // The invoking command of the level_3 command, is the previous level_2 command
        // The root invoking command of the level_3 command, is the original level_1 command
        assert_eq!(
            workflows.invoking_command_state(&level_3_cmd),
            Some(&level_2_cmd)
        );
        assert_eq!(
            workflows.root_invoking_command_state(&level_2_cmd),
            Some(&level_1_cmd)
        );
    }
}
