use tedge_utils::paths::ManagedDir;

/// The shipped workflows driving the standard actions of a service.
///
/// A service declares the actions it supports, one `cmd/<action>` capability topic each, and these
/// workflows are what runs the standard ones. Each is installed as a template, so an administrator
/// can adapt a workflow, or drop it altogether, without it being restored on the next upgrade.
pub struct ServiceWorkflowsBuilder {}

impl ServiceWorkflowsBuilder {
    pub async fn try_new(ops_dir: &ManagedDir) -> Result<Self, anyhow::Error> {
        for (file_name, definition) in [
            (
                "service_start.toml",
                include_str!("../resources/service_start.toml"),
            ),
            (
                "service_stop.toml",
                include_str!("../resources/service_stop.toml"),
            ),
            (
                "service_restart.toml",
                include_str!("../resources/service_restart.toml"),
            ),
        ] {
            ops_dir
                .template_file(file_name)?
                .persist(definition)
                .await?;
        }

        Ok(Self {})
    }
}
