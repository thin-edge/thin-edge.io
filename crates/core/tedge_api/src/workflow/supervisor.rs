use crate::mqtt_topics::Channel;
use crate::workflow::*;
use on_disk::OnDiskCommandBoard;
use serde::Serialize;
use std::string::ToString;
use tracing::error;
use tracing::info;

// Key should include EntityType because the same operation name can be used for a device and for a service
pub type WorkflowKey = (EntityType, OperationType);

/// Dispatch actions to operation participants
#[derive(Default)]
pub struct WorkflowSupervisor {
    /// The user-defined operation workflow definitions
    workflows: HashMap<WorkflowKey, WorkflowVersions>,

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
        )
    }

    /// Register a user-defined workflow
    pub fn register_custom_workflow(
        &mut self,
        version: WorkflowSource<WorkflowVersion>,
        workflow: OperationWorkflow,
    ) -> Result<(), WorkflowRegistrationError> {
        let key = (workflow.entity_type, workflow.operation.clone());
        if let Some(versions) = self.workflows.get_mut(&key) {
            versions.add(version, workflow);
        } else {
            let versions = WorkflowVersions::new(version, workflow);
            self.workflows.insert(key, versions);
        }
        Ok(())
    }

    /// Un-register a user-defined workflow
    ///
    /// Return true if a builtin version has been restored
    pub fn unregister_custom_workflow(
        &mut self,
        entity_type: EntityType,
        operation: &OperationName,
        version: &WorkflowVersion,
    ) -> bool {
        let key = (entity_type, OperationType::from(operation.as_str()));
        if let Some(versions) = self.workflows.get_mut(&key) {
            versions.remove(version);
        }

        let (empty, builtin_restored) = match self.workflows.get(&key) {
            None => (true, false),
            Some(version) if version.is_empty() => (true, false),
            Some(version) if version.is_builtin() => (false, true),
            Some(_) => (false, false),
        };

        if empty {
            self.workflows.remove(&key);
        }

        builtin_restored
    }

    /// The set of pending commands
    pub fn pending_commands(&self) -> &CommandBoard {
        &self.commands
    }

    /// Update on start the set of pending commands
    pub fn load_pending_commands(&mut self, commands: CommandBoard) -> Vec<GenericCommandState> {
        self.commands = commands;
        let resumed_commands: Vec<GenericCommandState> = self
            .commands
            .iter()
            .filter_map(|(t, s)| self.resume_command(t, s.clone()))
            .collect();

        // The commands should be updated with the resumed states,
        // and not only the caller notified of these new states.
        //
        // A resumed command can be the sub-command of another resumed command
        // which has then to observe the actual state of its sub-command
        // and not the state persisted before the agent was interrupted.
        for command in resumed_commands.iter() {
            if let Err(err) = self.commands.update(command.clone()) {
                error!("Fail to resume command: {err}");
            }
        }

        resumed_commands
    }

    /// List the capabilities provided by the registered workflows
    pub fn capability_messages(
        &self,
        schema: &MqttSchema,
        target: &EntityTopicId,
        entity_type: EntityType,
    ) -> Vec<MqttMessage> {
        // Only the workflows of the target's own type are capabilities of that target
        // To ease testing the capability messages are emitted in a deterministic order
        let mut operations = self
            .workflows
            .iter()
            .filter(|((workflow_type, _), _)| workflow_type == &entity_type)
            .filter_map(|(_, versions)| versions.current_workflow())
            .collect::<Vec<_>>();
        operations.sort_by_key(|&a| a.operation.to_string());
        operations
            .iter()
            .filter_map(|workflow| workflow.capability_message(schema, target))
            .collect()
    }

    pub fn capability_message(
        &self,
        schema: &MqttSchema,
        target: &EntityTopicId,
        entity_type: EntityType,
        operation: &OperationName,
    ) -> Option<MqttMessage> {
        let operation = OperationType::from(operation.as_str());
        self.workflows
            .get(&(entity_type, operation))
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

    /// Mark the current version of an operation workflow as being in use.
    ///
    /// Return the current version if any.
    pub fn use_current_version(
        &mut self,
        entity_type: EntityType,
        operation: &OperationName,
    ) -> Option<WorkflowVersion> {
        self.workflows
            .get_mut(&(entity_type, operation.as_str().into()))?
            .use_current_version()
            .cloned()
    }

    /// Update the state of the command board on reception of a message sent by a peer over MQTT
    ///
    /// Return the new CommandRequest state if any.
    pub fn apply_external_update(
        &mut self,
        entity_type: EntityType,
        operation: &OperationType,
        command_state: GenericCommandState,
    ) -> Result<Option<GenericCommandState>, WorkflowExecutionError> {
        let Some(workflow_versions) = self.workflows.get_mut(&(entity_type, operation.clone()))
        else {
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
                let updated_state = command_state.with_workflow_version(current_version);
                self.commands.insert(updated_state.clone())?;
                Ok(Some(updated_state))
            } else {
                Err(WorkflowExecutionError::DeprecatedOperation {
                    operation: operation.to_string(),
                })
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

        let Some(version) = command_state.workflow_version() else {
            return Err(WorkflowExecutionError::MissingVersion);
        };

        // A command state does not have an entity type, so all the workflows registered
        // under this operation name are candidates.
        let operation = OperationType::from(operation_name.as_str());
        let mut candidates = self
            .workflows
            .iter()
            .filter(|((_, workflow_operation), _)| workflow_operation == &operation)
            .map(|(_, versions)| versions)
            .peekable();

        if candidates.peek().is_none() {
            return Err(WorkflowExecutionError::UnknownOperation {
                operation: operation_name.clone(),
            });
        }

        for versions in candidates {
            if let Ok(workflow) = versions.get(version) {
                return workflow.get_action(command_state);
            }
        }

        Err(WorkflowExecutionError::UnknownVersion {
            operation: operation_name.clone(),
            version: version.to_string(),
        })
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
        command: GenericCommandState,
    ) -> Option<GenericCommandState> {
        let action = match self.get_action(&command) {
            Ok(action) => action,
            Err(err) => {
                return Some(command.fail_with(format!("Fail to resume on start: {err:?}")));
            }
        };

        let epoch = format!("{}.{}", timestamp.unix_timestamp(), timestamp.millisecond());
        let command = command.with_key_value("resumed_at", &epoch);
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

/// The set of in-use workflow versions for an operation
///
/// - The current version is the version that will be used for a new command instance.
/// - The current version might be none. This is the case when the command has been deprecated.
/// - When a new command instance is initialized, the current version is stored as being in use.
/// - When all the commands using a given version are finalized, these copies are removed.
/// - Among all the versions, the `"builtin"` version is specific.
/// - The `"builtin"` version is never removed, and is used as the current version if none is available.
struct WorkflowVersions {
    operation: OperationName,
    current: Option<(WorkflowVersion, OperationWorkflow)>,
    in_use: HashMap<WorkflowVersion, OperationWorkflow>,
}

pub enum WorkflowSource<T> {
    BuiltIn,
    UserDefined(T),
    InUseCopy(T),
}

impl<T> WorkflowSource<T> {
    pub fn inner(&self) -> Option<&T> {
        match self {
            BuiltIn => None,
            UserDefined(inner) | InUseCopy(inner) => Some(inner),
        }
    }

    pub fn set_inner<U>(&self, target: U) -> WorkflowSource<U> {
        match self {
            BuiltIn => BuiltIn,
            UserDefined(_) => UserDefined(target),
            InUseCopy(_) => InUseCopy(target),
        }
    }
}

use WorkflowSource::*;

impl WorkflowVersions {
    fn new(source: WorkflowSource<WorkflowVersion>, workflow: OperationWorkflow) -> Self {
        let operation = workflow.operation.to_string();
        let (current, in_use) = match source {
            BuiltIn => (None, HashMap::from([(BUILT_IN.to_string(), workflow)])),
            UserDefined(version) => (Some((version, workflow)), HashMap::new()),
            InUseCopy(version) => (None, HashMap::from([(version, workflow)])),
        };

        WorkflowVersions {
            operation,
            current,
            in_use,
        }
    }

    fn add(&mut self, source: WorkflowSource<WorkflowVersion>, workflow: OperationWorkflow) {
        match source {
            BuiltIn => {
                self.in_use.insert(BUILT_IN.to_string(), workflow);
            }
            UserDefined(version) => {
                self.current = Some((version, workflow));
            }
            InUseCopy(version) => {
                self.in_use.insert(version, workflow);
            }
        };

        if self.current.is_some() && self.in_use.contains_key(BUILT_IN) {
            info!(
                "The built-in {operation} operation has been customized",
                operation = self.operation
            );
        }
    }

    /// Mark the current version as being in-use.
    fn use_current_version(&mut self) -> Option<&WorkflowVersion> {
        match self.current.as_ref() {
            Some((version, workflow)) => {
                if !self.in_use.contains_key(version) {
                    self.in_use.insert(version.clone(), workflow.clone());
                };
                Some(version)
            }

            None => self
                .in_use
                .get_key_value(BUILT_IN)
                .map(|(builtin, _)| builtin),
        }
    }

    /// Remove the current version from this list of versions, restoring the built-in version if any
    fn remove(&mut self, version: &WorkflowVersion) {
        if self.current.as_ref().map(|(v, _)| v == version) == Some(true) {
            self.current = None;
        } else if version != BUILT_IN {
            self.in_use.remove(version);
        }
    }

    fn is_empty(&self) -> bool {
        self.in_use.is_empty()
    }

    fn is_builtin(&self) -> bool {
        self.in_use.contains_key(BUILT_IN)
    }

    fn get(&self, version: &str) -> Result<&OperationWorkflow, WorkflowExecutionError> {
        self.in_use
            .get(version)
            .ok_or(WorkflowExecutionError::UnknownVersion {
                operation: self.operation.clone(),
                version: version.to_string(),
            })
    }

    fn current_workflow(&self) -> Option<&OperationWorkflow> {
        self.current
            .as_ref()
            .map(|(_, workflow)| workflow)
            .or_else(|| self.in_use.get(BUILT_IN))
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

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut (Timestamp, GenericCommandState)> {
        self.commands.values_mut()
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
    fn a_device_and_a_service_workflow_share_an_operation_name() {
        let mut workflows = WorkflowSupervisor::default();
        let restart = OperationType::Restart;

        // A workflow with no `type` is a device workflow, as before this field existed
        workflows
            .register_custom_workflow(
                UserDefined("device-version".to_string()),
                restart_workflow_of_type("", "device_step"),
            )
            .unwrap();

        workflows
            .register_custom_workflow(
                UserDefined("service-version".to_string()),
                restart_workflow_of_type(r#"type = "service""#, "service_step"),
            )
            .unwrap();

        // A command addressed to the device is driven by the device workflow
        let device_cmd = GenericCommandState::from_command_message(&MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/restart/id_1"),
            r#"{ "status":"init" }"#,
        ))
        .unwrap();
        let device_cmd = workflows
            .apply_external_update(EntityType::MainDevice, &restart, device_cmd)
            .unwrap()
            .unwrap();
        assert_eq!(device_cmd.workflow_version(), Some("device-version"));
        assert_eq!(
            workflows.get_action(&device_cmd).unwrap(),
            OperationAction::MoveTo("device_step".into())
        );

        // A command addressed to a service is driven by the service workflow
        let service_cmd = GenericCommandState::from_command_message(&MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/collectd/cmd/restart/id_2"),
            r#"{ "status":"init" }"#,
        ))
        .unwrap();
        let service_cmd = workflows
            .apply_external_update(EntityType::Service, &restart, service_cmd)
            .unwrap()
            .unwrap();
        assert_eq!(service_cmd.workflow_version(), Some("service-version"));
        assert_eq!(
            workflows.get_action(&service_cmd).unwrap(),
            OperationAction::MoveTo("service_step".into())
        );
    }

    #[test]
    fn a_service_workflow_does_not_drive_a_device_command() {
        let mut workflows = WorkflowSupervisor::default();

        workflows
            .register_custom_workflow(
                UserDefined("service-version".to_string()),
                restart_workflow_of_type(r#"type = "service""#, "service_step"),
            )
            .unwrap();

        let device_cmd = GenericCommandState::from_command_message(&MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/restart/id_1"),
            r#"{ "status":"init" }"#,
        ))
        .unwrap();
        let error = workflows
            .apply_external_update(EntityType::MainDevice, &OperationType::Restart, device_cmd)
            .unwrap_err();

        assert_matches::assert_matches!(error, WorkflowExecutionError::UnknownOperation { .. });
    }

    #[test]
    fn the_device_does_not_declare_the_workflow_of_a_service() {
        let mut workflows = WorkflowSupervisor::default();

        workflows
            .register_custom_workflow(
                UserDefined("service-version".to_string()),
                restart_workflow_of_type(r#"type = "service""#, "service_step"),
            )
            .unwrap();

        let schema = MqttSchema::default();
        let device = EntityTopicId::default_main_device();

        let capabilities = workflows.capability_messages(&schema, &device, EntityType::MainDevice);
        assert!(capabilities.is_empty());
    }

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
            .apply_external_update(EntityType::MainDevice, &level_1_op, level_1_cmd.clone())
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
            .apply_external_update(EntityType::MainDevice, &level_2_op, level_2_cmd.clone())
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
            .apply_external_update(EntityType::MainDevice, &level_3_op, level_3_cmd.clone())
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

    /// On restart, the state of a command awaiting the agent restart must be moved
    /// to its `on_success` state and not only in the returned states.
    #[test]
    fn agent_restart_is_persisted_on_the_command_board() {
        let mut workflows = restart_workflows();
        let cmd_topic = "te/device/main///cmd/restart-agent/robot-1";
        let board = command_board(vec![pending_command(cmd_topic, "restarting")]);

        let resumed = workflows.load_pending_commands(board);

        assert_eq!(resumed.len(), 1);
        assert_eq!(resumed[0].status, "successful");
        assert_eq!(
            workflows.get_state(cmd_topic).map(|s| s.status.as_ref()),
            Some("successful")
        );
    }

    /// On restart, a command awaiting the completion of a sub-command which was itself
    /// awaiting the agent restart, must observe the resumed state of that sub-command.
    #[test]
    fn agent_restart_is_visible_from_the_invoking_command() {
        let mut workflows = restart_workflows();
        let wrapper_topic = "te/device/main///cmd/restart-agent-wrapper/robot-1";
        let sub_cmd_topic = "te/device/main///cmd/restart-agent/sub:restart-agent-wrapper:robot-1";
        let wrapper_cmd = pending_command(wrapper_topic, "restarting");
        let board = command_board(vec![
            wrapper_cmd.clone(),
            pending_command(sub_cmd_topic, "restarting"),
        ]);

        workflows.load_pending_commands(board);

        // The invoking command must see its sub-command as successful, whatever the order
        // in which the pending commands have been resumed.
        let sub_cmd_state = workflows.sub_command_state(&wrapper_cmd);
        assert_eq!(
            sub_cmd_state.map(|s| s.status.as_ref()),
            Some("successful"),
            "the invoking command still sees a sub-command awaiting the agent restart"
        );
    }

    /// A workflow awaiting the agent restart, moving on success to a state which is not a step
    /// of this workflow but the final state of a `cleanup` action.
    const RESTART_WORKFLOW: &str = r#"
operation = "restart-agent"

[init]
action = "proceed"
on_success = "restarting"

[restarting]
action = "await-agent-restart"
on_success = "successful"
on_timeout = "failed"

[successful]
action = "cleanup"

[failed]
action = "cleanup"
"#;

    /// A workflow awaiting the completion of the restart-agent sub-operation
    const WRAPPER_WORKFLOW: &str = r#"
operation = "restart-agent-wrapper"

[init]
operation = "restart-agent"
on_exec = "restarting"

[restarting]
action = "await-operation-completion"
on_success = "successful"

[successful]
action = "cleanup"

[failed]
action = "cleanup"
"#;

    const VERSION: &str = "1.0";

    fn pending_command(topic: &str, status: &str) -> GenericCommandState {
        GenericCommandState::from_command_message(&MqttMessage::new(
            &Topic::new_unchecked(topic),
            format!(r#"{{ "@version": "{VERSION}", "status": "{status}" }}"#),
        ))
        .unwrap()
    }

    fn restart_workflows() -> WorkflowSupervisor {
        let mut workflows = WorkflowSupervisor::default();
        for definition in [RESTART_WORKFLOW, WRAPPER_WORKFLOW] {
            let workflow: OperationWorkflow = toml::from_str(definition).unwrap();
            let operation = workflow.operation.to_string();
            let entity_type = workflow.entity_type;
            workflows
                .register_custom_workflow(UserDefined(VERSION.to_string()), workflow)
                .unwrap();
            // Mark the version as in-use, as done when a command is created
            workflows.use_current_version(entity_type, &operation);
        }
        workflows
    }

    fn command_board(commands: Vec<GenericCommandState>) -> CommandBoard {
        let now = time::OffsetDateTime::now_utc();
        CommandBoard::new(
            commands
                .into_iter()
                .map(|command| (command.topic.name.clone(), (now, command)))
                .collect(),
        )
    }

    fn restart_workflow_of_type(type_field: &str, next_state: &str) -> OperationWorkflow {
        toml::from_str(&format!(
            r#"
operation = "restart"
{type_field}

[init]
action = "proceed"
on_success = "{next_state}"

[{next_state}]
action = "proceed"
on_success = "successful"
"#
        ))
        .unwrap()
    }
}
