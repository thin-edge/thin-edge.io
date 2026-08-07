use tedge_utils::paths::ManagedDir;

/// The shipped workflows driving the standard actions of a service.
///
/// These are the five actions an init system defines in `system.toml`. Any other action is a
/// custom one, for which a user writes a workflow of their own.
const SHIPPED_WORKFLOWS: [(&str, &str); 5] = [
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
    (
        "service_enable.toml",
        include_str!("../resources/service_enable.toml"),
    ),
    (
        "service_disable.toml",
        include_str!("../resources/service_disable.toml"),
    ),
];

/// The shipped workflows driving the standard actions of a service.
///
/// A service declares the actions it supports, one `cmd/<action>` capability topic each, and these
/// workflows are what runs the standard ones. Each is installed as a template, so an administrator
/// can adapt a workflow, or drop it altogether, without it being restored on the next upgrade.
pub struct ServiceWorkflowsBuilder {}

impl ServiceWorkflowsBuilder {
    pub async fn try_new(ops_dir: &ManagedDir) -> Result<Self, anyhow::Error> {
        for (file_name, definition) in SHIPPED_WORKFLOWS {
            ops_dir
                .template_file(file_name)?
                .persist(definition)
                .await?;
        }

        Ok(Self {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_api::entity::EntityType;
    use tedge_api::workflow::OperationWorkflow;

    /// A workflow is picked by the `operation` field inside it, never by its file name, so a
    /// definition copied from another action would silently drive that other action.
    #[test]
    fn every_shipped_workflow_drives_the_action_its_file_name_gives() {
        for (file_name, definition) in SHIPPED_WORKFLOWS {
            let workflow: OperationWorkflow = toml::from_str(definition)
                .unwrap_or_else(|err| panic!("{file_name} is not a valid workflow: {err}"));

            let action = file_name
                .strip_prefix("service_")
                .and_then(|name| name.strip_suffix(".toml"))
                .unwrap_or_else(|| panic!("{file_name} is not named service_<action>.toml"));

            assert_eq!(workflow.operation.to_string(), action, "{file_name}");
            assert_eq!(workflow.entity_type, EntityType::Service, "{file_name}");
        }
    }
}
