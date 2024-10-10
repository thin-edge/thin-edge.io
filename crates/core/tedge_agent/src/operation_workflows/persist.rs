use anyhow::Context;
use camino::Utf8PathBuf;
use std::ffi::OsStr;
use std::path::Path;
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
    builtin_workflows: Vec<OperationName>,
    custom_workflows_dir: Utf8PathBuf,
    state_dir: Utf8PathBuf,
    workflows: WorkflowSupervisor,
}

impl WorkflowRepository {
    pub fn new(
        builtin_workflows: Vec<OperationName>,
        custom_workflows_dir: Utf8PathBuf,
        state_dir: Utf8PathBuf,
    ) -> Self {
        let workflows = WorkflowSupervisor::default();
        Self {
            builtin_workflows,
            custom_workflows_dir,
            state_dir,
            workflows,
        }
    }

    pub async fn load(&mut self) {
        let dir_path = &self.custom_workflows_dir.clone();
        if let Err(err) = self.load_operation_workflows(dir_path).await {
            error!("Fail to read the operation workflows from {dir_path}: {err:?}");
        }

        let dir_path = &self.state_dir.clone();
        if let Err(err) = self.load_operation_workflows(dir_path).await {
            error!("Fail to reload the running operation workflows from {dir_path}: {err:?}");
        }
        self.load_builtin_workflows();
    }

    async fn load_operation_workflows(
        &mut self,
        dir_path: &Utf8PathBuf,
    ) -> Result<(), anyhow::Error> {
        for entry in std::fs::read_dir(dir_path)?.flatten() {
            let file = entry.path();
            if file.extension() == Some(OsStr::new("toml")) {
                match read_operation_workflow(&file)
                    .await
                    .and_then(|(workflow, version)| self.load_operation_workflow(workflow, version))
                {
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
        workflow: OperationWorkflow,
        version: WorkflowVersion,
    ) -> Result<String, anyhow::Error> {
        let name = workflow.operation.to_string();
        self.workflows.register_custom_workflow(workflow, version)?;
        Ok(name)
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

    pub fn apply_external_update(
        &mut self,
        operation: &OperationType,
        command_state: GenericCommandState,
    ) -> Result<Option<GenericCommandState>, WorkflowExecutionError> {
        self.workflows
            .apply_external_update(operation, command_state)
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
    path: &Path,
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
