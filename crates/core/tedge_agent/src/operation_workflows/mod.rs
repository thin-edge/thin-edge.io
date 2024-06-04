use anyhow::Context;
use camino::Utf8PathBuf;
use log::error;
use std::ffi::OsStr;
use std::path::Path;
use tedge_api::workflow::OperationWorkflow;
use tedge_api::workflow::WorkflowSupervisor;
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
                .and_then(|workflow| load_operation_workflow(&mut workflows, workflow))
            {
                Ok(cmd) => {
                    info!(
                        "Using operation workflow definition from {file:?} for '{cmd}' operation"
                    );
                }
                Err(err) => {
                    error!("Ignoring operation workflow definition from {file:?}: {err:?}")
                }
            };
        }
    }
    Ok(workflows)
}

async fn read_operation_workflow(path: &Path) -> Result<OperationWorkflow, anyhow::Error> {
    let bytes = tokio::fs::read(path)
        .await
        .context("Reading file content")?;
    let input = std::str::from_utf8(&bytes).context("Expecting UTF8 content")?;
    let workflow = toml::from_str::<OperationWorkflow>(input).context("Parsing TOML content")?;
    Ok(workflow)
}

fn load_operation_workflow(
    workflows: &mut WorkflowSupervisor,
    workflow: OperationWorkflow,
) -> Result<String, anyhow::Error> {
    let name = workflow.operation.to_string();
    workflows.register_custom_workflow(workflow)?;
    Ok(name)
}
