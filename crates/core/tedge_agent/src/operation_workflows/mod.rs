use anyhow::Context;
use camino::Utf8PathBuf;
use log::error;
use std::ffi::OsStr;
use std::path::Path;
use tedge_api::workflow::IllFormedOperationWorkflow;
use tedge_api::workflow::OperationWorkflow;
use tedge_api::workflow::WorkflowSupervisor;
use tedge_api::workflow::WorkflowVersion;
use tracing::info;

mod actor;
mod builder;
mod config;
mod message_box;

#[cfg(test)]
mod tests;

pub use builder::WorkflowActorBuilder;
pub use config::OperationConfig;

pub async fn load_operation_workflows(
    dir_path: &Utf8PathBuf,
) -> Result<WorkflowSupervisor, anyhow::Error> {
    let mut workflows = WorkflowSupervisor::default();
    for entry in std::fs::read_dir(dir_path)?.flatten() {
        let file = entry.path();
        if file.extension() == Some(OsStr::new("toml")) {
            match read_operation_workflow(&file)
                .await
                .and_then(|(workflow, version)| {
                    load_operation_workflow(&mut workflows, workflow, version)
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
    Ok(workflows)
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

fn load_operation_workflow(
    workflows: &mut WorkflowSupervisor,
    workflow: OperationWorkflow,
    version: WorkflowVersion,
) -> Result<String, anyhow::Error> {
    let name = workflow.operation.to_string();
    workflows.register_custom_workflow(workflow, version)?;
    Ok(name)
}
