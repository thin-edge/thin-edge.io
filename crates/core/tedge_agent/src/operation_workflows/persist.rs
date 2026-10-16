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
use tedge_api::workflow::OperationWorkflows;
use tedge_api::workflow::WorkflowExecutionError;
use tedge_api::workflow::WorkflowSupervisor;
use tedge_api::workflow::WorkflowVersion;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tracing::error;
use tracing::info;
use tracing::warn;

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
    // An entity type and an operation name together identify a workflow.
    definitions: HashMap<(EntityType, OperationName), (WorkflowVersion, Utf8PathBuf)>,

    // Map each workflow version of an operation to the count of instance using it
    in_use_copies: HashMap<(OperationName, WorkflowVersion), u32>,

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
                let (definition, version) = match read_operation_workflows(file).await {
                    Ok(definition) => definition,
                    Err(err) => {
                        error!("Ignoring {file:?}: {err:?}");
                        continue;
                    }
                };
                for workflow in definition.workflows {
                    // A command in use has a copy of its workflow file, named after its operation: `stop-<version>.toml`.
                    // When the file lists several operations, the copy holds them all, but only `stop` is in use.
                    if matches!(source, WorkflowSource::InUseCopy(_))
                        && file
                            != self.workflow_copy_path(&workflow.operation.to_string(), &version)
                    {
                        continue;
                    }
                    let file_source = source.set_inner(file.into());
                    match self.load_operation_workflow(file_source, workflow, version.clone()) {
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
        if entity_type == EntityType::ChildDevice {
            warn!(
                "The {operation_name} workflow declares type = \"child-device\", \
                 which is not supported: use type = \"device\""
            );
        }
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
                    .entry((operation_name.clone(), version.clone()))
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
    /// Return the capability messages of the operations which workflow has been updated,
    /// and the deregistration messages of the operations which have been removed.
    pub async fn update_operation_workflows(
        &mut self,
        schema: &MqttSchema,
        target: &EntityTopicId,
        file_update: FsWatchEvent,
    ) -> Vec<MqttMessage> {
        match file_update {
            // FsWatchEvent returns a duplicated Modified events along FileCreated events.
            FsWatchEvent::FileCreated(_) => {}

            FsWatchEvent::Modified(path) => {
                if let Ok(path) = Utf8PathBuf::try_from(path) {
                    if self.is_user_defined(&path) {
                        // Checking the path exists as FsWatchEvent returns misleading Modified events along FileDeleted events.
                        if path.exists() {
                            if let Some(updated) = self.reload_operation_workflow(&path).await {
                                let removed =
                                    self.remove_operation_workflows(&path, &updated).await;
                                return self.update_messages(schema, target, &updated, &removed);
                            }
                        }
                    }
                }
            }

            FsWatchEvent::FileDeleted(path) => {
                if let Ok(path) = Utf8PathBuf::try_from(path) {
                    if self.is_user_defined(&path) {
                        let removed = self.remove_operation_workflows(&path, &[]).await;
                        return self.update_messages(schema, target, &[], &removed);
                    }
                }
            }

            FsWatchEvent::DirectoryDeleted(_) | FsWatchEvent::DirectoryCreated(_) => {}
        }

        vec![]
    }

    fn update_messages(
        &self,
        schema: &MqttSchema,
        target: &EntityTopicId,
        updated: &[(EntityType, OperationName)],
        removed: &[(EntityType, OperationName)],
    ) -> Vec<MqttMessage> {
        let mut messages = vec![];
        for (entity_type, operation) in updated {
            messages.extend(self.capability_message(schema, target, *entity_type, operation));
        }
        for (entity_type, operation) in removed {
            messages.extend(self.deregistration_message(schema, target, *entity_type, operation));
        }
        messages
    }

    fn is_user_defined(&mut self, path: &Utf8PathBuf) -> bool {
        path.extension() == Some("toml") && path.parent() == Some(&self.custom_workflows_dir)
    }

    /// Reload a user defined workflow file.
    ///
    /// Return the operations defined by the file, or None if the file cannot be read.
    async fn reload_operation_workflow(
        &mut self,
        path: &Utf8PathBuf,
    ) -> Option<Vec<(EntityType, OperationName)>> {
        match read_operation_workflows(path).await {
            Ok((definition, version)) => {
                let mut updated = vec![];
                for workflow in definition.workflows {
                    if let Ok((entity_type, cmd)) = self.load_operation_workflow(
                        WorkflowSource::UserDefined(path.clone()),
                        workflow,
                        version.clone(),
                    ) {
                        info!("Using the updated operation workflow definition from {path} for the '{cmd}' operation of a {entity_type}");
                        updated.push((entity_type, cmd));
                    }
                }
                Some(updated)
            }
            Err(err) => {
                error!("Fail to reload {path}: {err}");
                None
            }
        }
    }

    /// Unregister the workflows read from a file, except those of `kept_operations`.
    ///
    /// Return the operations with no version left, i.e. with no builtin workflow.
    async fn remove_operation_workflows(
        &mut self,
        removed_path: &Utf8PathBuf,
        kept_operations: &[(EntityType, OperationName)],
    ) -> Vec<(EntityType, OperationName)> {
        // As this is not intended to be a frequent operation, there is no attempt to be efficient.
        let mut unregistered: Vec<_> = self
            .definitions
            .iter()
            .filter(|(key, (_, p))| p == removed_path && !kept_operations.contains(key))
            .map(|(key, (v, _))| (key.clone(), v.clone()))
            .collect();
        unregistered.sort_by_key(|((_, operation), _)| operation.clone());

        let mut removed = vec![];
        for (key, removed_version) in unregistered {
            self.definitions.remove(&key);
            let (entity_type, operation) = key;
            // Unregister before reloading: a copy of the removed file has the same version,
            // and unregistering that version after the reload would remove the reloaded workflow.
            let builtin_restored = self.workflows.unregister_custom_workflow(
                entity_type,
                &operation,
                &removed_version,
            );
            if let Some(path) = self.reload_from_other_file(entity_type, &operation).await {
                info!("Using operation workflow definition from {path} for the '{operation}' operation of a {entity_type}");
            } else if builtin_restored {
                info!("The builtin workflow for the '{operation}' operation has been restored");
            } else {
                info!("The user defined workflow for the '{operation}' operation of a {entity_type} has been removed");
                removed.push((entity_type, operation));
            }
        }
        removed
    }

    /// Load the workflow of an operation from the first user defined file that defines it.
    ///
    /// Return the path of that file.
    async fn reload_from_other_file(
        &mut self,
        entity_type: EntityType,
        operation: &OperationName,
    ) -> Option<Utf8PathBuf> {
        let entries = self.custom_workflows_dir.read_dir_utf8().ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension() != Some("toml") {
                continue;
            }
            let Ok((definition, version)) = read_operation_workflows(path).await else {
                continue;
            };
            if let Some(workflow) = definition.find(entity_type, operation) {
                let source = WorkflowSource::UserDefined(path.to_owned());
                if self
                    .load_operation_workflow(source, workflow, version)
                    .is_ok()
                {
                    return Some(path.to_owned());
                }
            }
        }
        None
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
        let copy = (operation.to_string(), version.to_string());
        if let Some(count) = self.in_use_copies.get_mut(&copy) {
            *count += 1;
            return;
        };

        if let Some((_, source)) = self.definitions.get(&(entity_type, operation.to_string())) {
            let target = self.workflow_copy_path(operation, version);
            if let Err(err) = tokio::fs::copy(source.clone(), target.clone()).await {
                error!("Fail to persist a copy of {source} as {target}: {err}");
            } else {
                self.in_use_copies.insert(copy, 1);
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
        let copy = (operation.to_string(), version.to_string());
        if let Some(count) = self.in_use_copies.get_mut(&copy) {
            *count -= 1;
            if *count > 0 {
                return;
            }
        }

        self.in_use_copies.remove(&copy);

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
            if let Ok((definition, latest)) = read_operation_workflows(path).await {
                if version != &latest {
                    let workflow = definition.find(entity_type, operation)?;
                    return Some((path.to_owned(), latest, workflow));
                };
            };
        } else {
            let path = self
                .custom_workflows_dir
                .join(operation)
                .with_extension("toml");
            if let Ok((definition, new)) = read_operation_workflows(&path).await {
                let workflow = definition.find(entity_type, operation)?;
                return Some((path, new, workflow));
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
        for entry in commands.iter_mut() {
            if entry.state.workflow_version().is_none() {
                if let Some(operation) = entry.state.operation() {
                    if let Some(current_version) = self
                        .workflows
                        .use_current_version(entry.entity_type, &operation)
                    {
                        self.persist_workflow_definition(
                            entry.entity_type,
                            &operation,
                            &current_version,
                        )
                        .await;
                        entry.state.set_workflow_version(&current_version);
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
    ) -> Vec<MqttMessage> {
        self.workflows.capability_messages(schema, target)
    }

    fn capability_message(
        &self,
        schema: &MqttSchema,
        target: &EntityTopicId,
        workflow_type: EntityType,
        operation: &OperationName,
    ) -> Option<MqttMessage> {
        if workflow_type != EntityType::MainDevice {
            return None;
        }
        self.workflows.capability_message(schema, target, operation)
    }

    fn deregistration_message(
        &self,
        schema: &MqttSchema,
        target: &EntityTopicId,
        workflow_type: EntityType,
        operation: &OperationName,
    ) -> Option<MqttMessage> {
        if workflow_type != EntityType::MainDevice {
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

async fn read_operation_workflows(
    path: &Utf8Path,
) -> Result<(OperationWorkflows, WorkflowVersion), anyhow::Error> {
    let bytes = tokio::fs::read(path).await.context("Fail to read file")?;
    let input = std::str::from_utf8(&bytes).context("Fail to extract UTF8 content")?;
    let version = sha256::digest(input);

    toml::from_str::<OperationWorkflows>(input)
        .context("Fail to parse TOML")
        .or_else(|err| {
            error!("Ill-formed operation workflow definition from {path:?}: {err:?}");
            let workflow = toml::from_str::<IllFormedOperationWorkflow>(input)
                .context("Extracting operation name")?;

            let reason = format!("Invalid operation workflow definition {path:?}: {err:?}");
            Ok(workflow.into_workflows(reason))
        })
        .map(|workflow| (workflow, version))
}
