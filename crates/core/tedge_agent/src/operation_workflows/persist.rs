use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use std::collections::HashMap;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
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

    // Map each workflow version to its workflow file
    //
    // For a fresh new workflow definition, this points to the user-defined file
    // When the workflow definition is in use, this points to a copy in the state directory.
    definitions: HashMap<String, (OperationName, Utf8PathBuf)>,

    // The in-memory representation of all the workflows (builtin, user-defined, i,n-use).
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
        Self {
            builtin_workflows,
            custom_workflows_dir,
            state_dir,
            definitions,
            workflows,
        }
    }

    pub async fn load(&mut self) {
        // Note that the loading order matters.

        // First, all the user-defined workflows are loaded
        let dir_path = &self.custom_workflows_dir.clone();
        if let Err(err) = self.load_operation_workflows(dir_path).await {
            error!("Fail to read the operation workflows from {dir_path}: {err:?}");
        }

        // Then, the definitions of the workflow still in-use are loaded
        // If a definition has not changed, then self.definitions is updated accordingly
        // so the known location of this definition is the copy not the original
        let dir_path = &self.state_dir.clone();
        let _ = tokio::fs::create_dir(dir_path).await; // if the creation fails, this will be reported next line on read
        if let Err(err) = self.load_operation_workflows(dir_path).await {
            error!("Fail to reload the running operation workflows from {dir_path}: {err:?}");
        }

        // Finally, builtin workflows are installed if not better definition has been provided by the user
        self.load_builtin_workflows();
    }

    async fn load_operation_workflows(
        &mut self,
        dir_path: &Utf8PathBuf,
    ) -> Result<(), anyhow::Error> {
        for entry in dir_path.read_dir_utf8()?.flatten() {
            let file = entry.path();
            if file.extension() == Some("toml") {
                match read_operation_workflow(file)
                    .await
                    .and_then(|(workflow, version)| {
                        self.load_operation_workflow(file.into(), workflow, version)
                    }) {
                    Ok(cmd) => {
                        info!(
                            "Using operation workflow definition from {file:?} for '{cmd}' operation"
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
        definition: Utf8PathBuf,
        workflow: OperationWorkflow,
        version: WorkflowVersion,
    ) -> Result<String, anyhow::Error> {
        let operation_name = workflow.operation.to_string();
        self.definitions
            .insert(version.clone(), (operation_name.clone(), definition));
        self.workflows.register_custom_workflow(workflow, version)?;
        Ok(operation_name)
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
        file_update: FsWatchEvent,
    ) -> Option<OperationName> {
        match file_update {
            FsWatchEvent::Modified(path) | FsWatchEvent::FileCreated(path) => {
                if let Ok(path) = Utf8PathBuf::try_from(path) {
                    if self.is_user_defined(&path) {
                        if path.exists() {
                            self.reload_operation_workflow(&path).await
                        } else {
                            // FsWatchEvent returns misleading Modified events along FileDeleted events.
                            return self.remove_operation_workflow(&path).await;
                        }
                    }
                }
            }

            FsWatchEvent::FileDeleted(path) => {
                if let Ok(path) = Utf8PathBuf::try_from(path) {
                    if self.is_user_defined(&path) {
                        return self.remove_operation_workflow(&path).await;
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

    async fn reload_operation_workflow(&mut self, path: &Utf8PathBuf) {
        match read_operation_workflow(path).await {
            Ok((workflow, version)) => {
                if let Ok(cmd) = self.load_operation_workflow(path.clone(), workflow, version) {
                    info!("Using the updated operation workflow definition from {path} for '{cmd}' operation");
                }
            }
            Err(err) => {
                error!("Fail to reload {path}: {err}")
            }
        }
    }

    /// Remove a user defined workflow.
    ///
    /// Return the operation name if this was the last version for that operation,
    /// .i.e. there is no builtin workflow.
    async fn remove_operation_workflow(
        &mut self,
        removed_path: &Utf8PathBuf,
    ) -> Option<OperationName> {
        // As this is not intended to be a frequent operation, there is no attempt to be efficient.
        let (operation, removed_version) = self
            .definitions
            .iter()
            .find(|(_, (_, p))| p == removed_path)
            .map(|(v, (n, _))| (n.clone(), v.clone()))?;

        self.definitions.remove(&removed_version);
        let deprecated = self
            .workflows
            .unregister_custom_workflow(&operation, &removed_version);
        if deprecated {
            info!("The user defined workflow for the '{operation}' operation has been removed");
            Some(operation)
        } else {
            None
        }
    }

    /// Copy the workflow definition file to the persisted state directory,
    /// unless this has already been done.
    async fn persist_workflow_definition(
        &mut self,
        operation: &OperationName,
        version: &WorkflowVersion,
    ) {
        if let Some((_, source)) = self.definitions.get(version) {
            if !source.starts_with(&self.state_dir) {
                let target = self.state_dir.join(operation).with_extension("toml");
                if let Err(err) = tokio::fs::copy(source.clone(), target.clone()).await {
                    error!("Fail to persist a copy of {source} as {target}: {err}");
                } else {
                    self.definitions
                        .insert(version.clone(), (operation.clone(), target));
                }
            }
        }
    }

    pub fn load_pending_commands(&mut self, commands: CommandBoard) -> Vec<GenericCommandState> {
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

    pub fn deregistration_message(
        &self,
        schema: &MqttSchema,
        target: &EntityTopicId,
        operation: &OperationName,
    ) -> MqttMessage {
        self.workflows
            .deregistration_message(schema, target, operation)
    }

    /// Update the state of the command board on reception of a message sent by a peer over MQTT
    ///
    /// If this is the first time a command of that type is created,
    /// then a copy of its definition is persisted in the state directory.
    /// The point is to be sure the command execution is ruled by its initial workflow unchanged
    /// even if the user pushes a new version meantime.
    pub async fn apply_external_update(
        &mut self,
        operation: &OperationType,
        command_state: GenericCommandState,
    ) -> Result<Option<GenericCommandState>, WorkflowExecutionError> {
        match self
            .workflows
            .apply_external_update(operation, command_state)?
        {
            None => Ok(None),

            Some(new_state) if new_state.is_init() => {
                if let Some(version) = new_state.workflow_version() {
                    self.persist_workflow_definition(&operation.to_string(), &version)
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
            Ok(OperationWorkflow::ill_formed(workflow.operation, reason))
        })
        .map(|workflow| (workflow, version))
}
