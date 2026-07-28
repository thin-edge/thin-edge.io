use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::collections::HashMap;
use tedge_api::entity::EntityType;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::supervisor::WorkflowSource;
use tedge_api::workflow::version_is_builtin;
use tedge_api::workflow::CommandBoard;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::IllFormedOperationWorkflow;
use tedge_api::workflow::OperationAction;
use tedge_api::workflow::OperationName;
use tedge_api::workflow::OperationWorkflow;
use tedge_api::workflow::WorkflowExecutionError;
use tedge_api::workflow::WorkflowSupervisor;
use tedge_api::workflow::WorkflowVersion;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tracing::error;
use tracing::info;

/// Persist the workflow definitions.
///
/// The WorkflowRepository acts as a facade to WorkflowSupervisor
/// adding all disk related features:
/// - loading definitions from disk,
/// - caching definitions in-use,
/// - reloading definitions on changes.
pub struct WorkflowRepository {
    // Names of the builtin workflows, i.e. without any file representation
    builtin_workflows: Vec<OperationName>,

    // Directory for the user-defined workflows
    custom_workflows_dir: Utf8PathBuf,

    // Directory of user-defined workflow copies of the workflow in-use
    state_dir: Utf8PathBuf,

    // Map each user defined workflow to its version and workflow file
    //
    // Keyed by entity type too, as two workflows can share an operation name
    // when they apply to entities of different types.
    definitions: HashMap<(EntityType, OperationName), (WorkflowVersion, Utf8PathBuf)>,

    // Map each workflow version to the count of instance using it
    in_use_copies: HashMap<WorkflowVersion, u32>,

    // The in-memory representation of all the workflows (builtin, user-defined, in-use).
    workflows: WorkflowSupervisor,
}

impl WorkflowRepository {
    pub fn new(
        builtin_workflows: Vec<OperationName>,
        custom_workflows_dir: Utf8PathBuf,
        state_dir: Utf8PathBuf,
    ) -> Self {
        let workflows = WorkflowSupervisor::default();
        let state_dir = state_dir.join("workflows-in-use");
        let definitions = HashMap::new();
        let in_use_copies = HashMap::new();
        Self {
            builtin_workflows,
            custom_workflows_dir,
            state_dir,
            definitions,
            in_use_copies,
            workflows,
        }
    }

    pub async fn load(&mut self) {
        // First, all the user-defined workflows are loaded
        let dir_path = &self.custom_workflows_dir.clone();
        if let Err(err) = self
            .load_operation_workflows(WorkflowSource::UserDefined(dir_path))
            .await
        {
            error!("Fail to read the operation workflows from {dir_path}: {err:?}");
        }

        // Then, the definitions of the workflow still in-use are loaded
        let dir_path = &self.state_dir.clone();
        let _ = tokio::fs::create_dir(dir_path).await; // if the creation fails, this will be reported next line on read
        if let Err(err) = self
            .load_operation_workflows(WorkflowSource::InUseCopy(dir_path))
            .await
        {
            error!("Fail to reload the running operation workflows from {dir_path}: {err:?}");
        }

        // Finally, builtin workflows are installed if not better definition has been provided by the user
        self.load_builtin_workflows();
    }

    async fn load_operation_workflows(
        &mut self,
        source: WorkflowSource<&Utf8PathBuf>,
    ) -> Result<(), anyhow::Error> {
        let Some(dir_path) = source.inner() else {
            return Ok(());
        };
        for entry in dir_path.read_dir_utf8()?.flatten() {
            let file = entry.path();
            if file.extension() == Some("toml") {
                let result = read_operation_workflow(file)
                    .await
                    .and_then(|(workflow, version)| {
                        let file_source = source.set_inner(file.into());
                        self.load_operation_workflow(file_source, workflow, version)
                    });
                match result {
                    Ok((entity_type, cmd)) => {
                        info!(
                            "Using operation workflow definition from {file:?} for the '{cmd}' operation of a {entity_type}"
                        );
                    }
                    Err(err) => {
                        error!("Ignoring {file:?}: {err:?}")
                    }
                };
            }
        }
        Ok(())
    }

    fn load_operation_workflow(
        &mut self,
        source: WorkflowSource<Utf8PathBuf>,
        workflow: OperationWorkflow,
        version: WorkflowVersion,
    ) -> Result<(EntityType, OperationName), anyhow::Error> {
        let entity_type = workflow.entity_type;
        let operation_name = workflow.operation.to_string();
        let version = match source {
            WorkflowSource::UserDefined(definition) => {
                self.definitions.insert(
                    (entity_type, operation_name.clone()),
                    (version.clone(), definition),
                );
                WorkflowSource::UserDefined(version)
            }
            WorkflowSource::InUseCopy(_) => {
                self.in_use_copies
                    .entry(version.clone())
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
                WorkflowSource::InUseCopy(version)
            }
            WorkflowSource::BuiltIn => WorkflowSource::BuiltIn,
        };

        self.workflows.register_custom_workflow(version, workflow)?;
        Ok((entity_type, operation_name))
    }

    fn load_builtin_workflows(&mut self) {
        for capability in self.builtin_workflows.iter() {
            if let Err(err) = self
                .workflows
                .register_builtin_workflow(capability.as_str().into())
            {
                error!("Fail to register built-in workflow for {capability} operation: {err}");
            }
        }
    }

    /// Update the workflow definitions after some on-disk changes
    ///
    /// Return the operation capability deregistration message when the operation has been deprecated.
    pub async fn update_operation_workflows(
        &mut self,
        schema: &MqttSchema,
        target: &EntityTopicId,
        file_update: FsWatchEvent,
    ) -> Option<MqttMessage> {
        match file_update {
            // FsWatchEvent returns a duplicated Modified events along FileCreated events.
            FsWatchEvent::FileCreated(_) => {}

            FsWatchEvent::Modified(path) => {
                if let Ok(path) = Utf8PathBuf::try_from(path) {
                    if self.is_user_defined(&path) {
                        // Checking the path exists as FsWatchEvent returns misleading Modified events along FileDeleted events.
                        if path.exists() {
                            return self.reload_operation_workflow(&path).await.and_then(
                                |(entity_type, updated_operation)| {
                                    self.capability_message(
                                        schema,
                                        target,
                                        entity_type,
                                        &updated_operation,
                                    )
                                },
                            );
                        }
                    }
                }
            }

            FsWatchEvent::FileDeleted(path) => {
                if let Ok(path) = Utf8PathBuf::try_from(path) {
                    if self.is_user_defined(&path) {
                        return self.remove_operation_workflow(&path).await.and_then(
                            |(entity_type, deprecated_operation)| {
                                self.deregistration_message(
                                    schema,
                                    target,
                                    entity_type,
                                    &deprecated_operation,
                                )
                            },
                        );
                    }
                }
            }

            FsWatchEvent::DirectoryDeleted(_) | FsWatchEvent::DirectoryCreated(_) => {}
        }

        None
    }

    fn is_user_defined(&mut self, path: &Utf8PathBuf) -> bool {
        path.extension() == Some("toml") && path.parent() == Some(&self.custom_workflows_dir)
    }

    /// Reload a user defined workflow.
    ///
    /// Return the operation name if this is a new operation or workflow version.
    async fn reload_operation_workflow(
        &mut self,
        path: &Utf8PathBuf,
    ) -> Option<(EntityType, OperationName)> {
        match read_operation_workflow(path).await {
            Ok((workflow, version)) => {
                if let Ok((entity_type, cmd)) = self.load_operation_workflow(
                    WorkflowSource::UserDefined(path.clone()),
                    workflow,
                    version,
                ) {
                    info!("Using the updated operation workflow definition from {path} for the '{cmd}' operation of a {entity_type}");
                    return Some((entity_type, cmd));
                }
            }
            Err(err) => {
                error!("Fail to reload {path}: {err}")
            }
        }
        None
    }

    /// Remove a user defined workflow.
    ///
    /// Return the operation name if this was the last version for that operation,
    /// .i.e. there is no builtin workflow.
    async fn remove_operation_workflow(
        &mut self,
        removed_path: &Utf8PathBuf,
    ) -> Option<(EntityType, OperationName)> {
        // As this is not intended to be a frequent operation, there is no attempt to be efficient.
        let (key, removed_version) = self
            .definitions
            .iter()
            .find(|(_, (_, p))| p == removed_path)
            .map(|(key, (v, _))| (key.clone(), v.clone()))?;
        self.definitions.remove(&key);
        let (entity_type, operation) = key;
        let builtin_restored =
            self.workflows
                .unregister_custom_workflow(entity_type, &operation, &removed_version);
        if builtin_restored {
            info!("The builtin workflow for the '{operation}' operation has been restored");
            None
        } else {
            info!("The user defined workflow for the '{operation}' operation of a {entity_type} has been removed");
            Some((entity_type, operation))
        }
    }

    /// Copy the workflow definition file to the persisted state directory,
    /// unless this has already been done.
    async fn persist_workflow_definition(
        &mut self,
        entity_type: EntityType,
        operation: &str,
        version: &str,
    ) {
        if version_is_builtin(version) {
            return;
        }
        if let Some(count) = self.in_use_copies.get_mut(version) {
            *count += 1;
            return;
        };

        if let Some((_, source)) = self.definitions.get(&(entity_type, operation.to_string())) {
            let target = self.workflow_copy_path(operation, version);
            if let Err(err) = tokio::fs::copy(source.clone(), target.clone()).await {
                error!("Fail to persist a copy of {source} as {target}: {err}");
            } else {
                self.in_use_copies.insert(version.to_owned(), 1);
            }
        }
    }

    fn workflow_copy_path(&self, operation: &str, version: &str) -> Utf8PathBuf {
        let filename = format!("{operation}-{version}");
        self.state_dir.join(filename).with_extension("toml")
    }

    async fn load_latest_version(&mut self, entity_type: EntityType, operation: &OperationName) {
        if let Some((path, version, workflow)) =
            self.get_latest_version(entity_type, operation).await
        {
            if let Err(err) = self.load_operation_workflow(
                WorkflowSource::UserDefined(path.clone()),
                workflow,
                version,
            ) {
                error!("Fail to reload the latest version of the {operation} operation from {path}: {err:?}");
            }
        }
    }

    async fn release_in_use_copy(&mut self, operation: &str, version: &str) {
        if version_is_builtin(version) {
            return;
        }
        if let Some(count) = self.in_use_copies.get_mut(version) {
            *count -= 1;
            if *count > 0 {
                return;
            }
        }

        self.in_use_copies.remove(version);

        let target = self.workflow_copy_path(operation, version);
        if let Err(err) = tokio::fs::remove_file(target.clone()).await {
            error!("Fail to remove the workflow copy at {target}: {err}");
        }
    }

    async fn get_latest_version(
        &mut self,
        entity_type: EntityType,
        operation: &OperationName,
    ) -> Option<(Utf8PathBuf, WorkflowVersion, OperationWorkflow)> {
        if let Some((version, path)) = self.definitions.get(&(entity_type, operation.clone())) {
            if let Ok((workflow, latest)) = read_operation_workflow(path).await {
                if version != &latest {
                    return Some((path.to_owned(), latest, workflow));
                };
            };
        } else {
            let path = self
                .custom_workflows_dir
                .join(operation)
                .with_extension("toml");
            if let Ok((workflow, new)) = read_operation_workflow(&path).await {
                // The name of a file tells nothing of the type of the entities its workflow
                // applies to: a workflow for another type is not the latest version of this one
                if workflow.entity_type == entity_type {
                    return Some((path, new, workflow));
                }
            };
        }
        None
    }

    pub async fn load_pending_commands(
        &mut self,
        mut commands: CommandBoard,
    ) -> Vec<GenericCommandState> {
        // If the resumed commands have been triggered by an agent without workflow version management
        // then these commands are assigned the current version of the operation workflow.
        // These currents versions have also to be marked as in use and persisted.
        for (_, command) in commands.iter_mut() {
            if command.workflow_version().is_none() {
                if let Some(operation) = command.operation() {
                    if let Some(current_version) = self
                        .workflows
                        .use_current_version(EntityType::MainDevice, &operation)
                    {
                        self.persist_workflow_definition(
                            EntityType::MainDevice,
                            &operation,
                            &current_version,
                        )
                        .await;
                        command.set_workflow_version(&current_version);
                    }
                }
            }
        }

        self.workflows.load_pending_commands(commands)
    }

    pub fn pending_commands(&self) -> &CommandBoard {
        self.workflows.pending_commands()
    }

    pub fn capability_messages(
        &self,
        schema: &MqttSchema,
        target: &EntityTopicId,
        entity_type: EntityType,
    ) -> Vec<MqttMessage> {
        self.workflows
            .capability_messages(schema, target, entity_type)
    }

    /// The message declaring the capability of an updated operation
    ///
    /// Only for the workflows of the target itself: the capabilities of a service are
    /// declared by that service, not by the workflows installed on the device.
    fn capability_message(
        &self,
        schema: &MqttSchema,
        target: &EntityTopicId,
        entity_type: EntityType,
        operation: &OperationName,
    ) -> Option<MqttMessage> {
        if entity_type != EntityType::MainDevice {
            return None;
        }
        self.workflows
            .capability_message(schema, target, entity_type, operation)
    }

    /// The message clearing the capability of a deprecated operation
    ///
    /// Only for the workflows of the target itself: the capabilities of a service are
    /// declared by that service, not by the workflows installed on the device.
    fn deregistration_message(
        &self,
        schema: &MqttSchema,
        target: &EntityTopicId,
        entity_type: EntityType,
        operation: &OperationName,
    ) -> Option<MqttMessage> {
        if entity_type != EntityType::MainDevice {
            return None;
        }
        Some(
            self.workflows
                .deregistration_message(schema, target, operation),
        )
    }

    /// Update the state of the command board on reception of a message sent by a peer over MQTT
    ///
    /// If this is the first time a command of that type is created,
    /// then a copy of its definition is persisted in the state directory.
    /// The point is to be sure the command execution is ruled by its initial workflow unchanged
    /// even if the user pushes a new version meantime.
    pub async fn apply_external_update(
        &mut self,
        entity_type: EntityType,
        operation: &OperationType,
        command_state: GenericCommandState,
    ) -> Result<Option<GenericCommandState>, WorkflowExecutionError> {
        let operation_name = operation.name();
        if command_state.is_init() {
            // A new command instance must use the latest on-disk version of the operation workflow
            self.load_latest_version(entity_type, &operation_name).await;
        } else if command_state.is_finished() {
            // Clear the cache if this happens to be the latest instance using that version of the workflow
            if let Some(version) = command_state.workflow_version() {
                self.release_in_use_copy(&operation_name, version).await;
            }
        }

        match self
            .workflows
            .apply_external_update(entity_type, operation, command_state)?
        {
            None => Ok(None),

            Some(new_state) if new_state.is_init() => {
                if let Some(version) = new_state.workflow_version() {
                    self.persist_workflow_definition(entity_type, &operation_name, version)
                        .await;
                }
                Ok(Some(new_state))
            }

            Some(updated_state) => Ok(Some(updated_state)),
        }
    }

    pub fn apply_internal_update(
        &mut self,
        new_command_state: GenericCommandState,
    ) -> Result<(), WorkflowExecutionError> {
        self.workflows.apply_internal_update(new_command_state)
    }

    pub fn get_action(
        &self,
        command_state: &GenericCommandState,
    ) -> Result<OperationAction, WorkflowExecutionError> {
        self.workflows.get_action(command_state)
    }

    pub fn get_state(&self, command: &str) -> Option<&GenericCommandState> {
        self.workflows.get_state(command)
    }

    pub fn root_invoking_command_state(
        &self,
        leaf_command: &GenericCommandState,
    ) -> Option<&GenericCommandState> {
        self.workflows.root_invoking_command_state(leaf_command)
    }

    pub fn invoking_command_state(
        &self,
        sub_command: &GenericCommandState,
    ) -> Option<&GenericCommandState> {
        self.workflows.invoking_command_state(sub_command)
    }

    pub fn sub_command_state(
        &self,
        command_state: &GenericCommandState,
    ) -> Option<&GenericCommandState> {
        self.workflows.sub_command_state(command_state)
    }

    pub fn adapt_builtin_response(
        &self,
        command_state: GenericCommandState,
    ) -> GenericCommandState {
        self.workflows.adapt_builtin_response(command_state)
    }
}

async fn read_operation_workflow(
    path: &Utf8Path,
) -> Result<(OperationWorkflow, WorkflowVersion), anyhow::Error> {
    let bytes = tokio::fs::read(path).await.context("Fail to read file")?;
    let input = std::str::from_utf8(&bytes).context("Fail to extract UTF8 content")?;
    let version = sha256::digest(input);

    toml::from_str::<OperationWorkflow>(input)
        .context("Fail to parse TOML")
        .or_else(|err| {
            error!("Ill-formed operation workflow definition from {path:?}: {err:?}");
            let workflow = toml::from_str::<IllFormedOperationWorkflow>(input)
                .context("Extracting operation name")?;

            let reason = format!("Invalid operation workflow definition {path:?}: {err:?}");
            Ok(OperationWorkflow::ill_formed(
                workflow.entity_type,
                workflow.operation,
                reason,
            ))
        })
        .map(|workflow| (workflow, version))
}
