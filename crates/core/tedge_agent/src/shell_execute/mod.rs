use tedge_utils::paths::ManagedDir;

pub struct ShellExecuteBuilder {}

impl ShellExecuteBuilder {
    pub async fn try_new(ops_dir: &ManagedDir) -> Result<Self, anyhow::Error> {
        let workflow_definition = include_str!("../resources/shell_execute.toml");
        let workflow = ops_dir.template_file("shell_execute.toml")?;
        let active = workflow.path().to_owned();
        let template = format!("{active}.template");
        let disabled = format!("{active}.disabled");

        // The workflow used to be provided by the tedge-command-plugin package,
        // which owns shell_execute.toml but not the template created by the agent.
        // Removing that package deletes shell_execute.toml, which the template pattern
        // would otherwise take for a deliberate removal and never restore.
        // Removing the stale template lets the workflow be deployed again,
        // unless it has been disabled on purpose.
        let active_exists = tokio::fs::try_exists(&active).await?;
        let disabled_exists = tokio::fs::try_exists(&disabled).await?;
        if !active_exists && !disabled_exists {
            tokio::fs::remove_file(&template)
                .await
                .or_else(tedge_utils::paths::ok_if_not_found)?;
        }

        // Initialize shell_execute.toml with template pattern:
        // - Always update shell_execute.toml.template with the latest definition
        // - Only update shell_execute.toml if it doesn't exist or hasn't been customized by the user
        workflow.persist(workflow_definition).await?;

        Ok(Self {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;
    use tedge_utils::paths::TedgePaths;

    const BUILTIN: &str = include_str!("../resources/shell_execute.toml");

    fn ops_dir(ttd: &TempTedgeDir) -> ManagedDir {
        TedgePaths::from_root_with_defaults(ttd.path(), "", "").root_dir()
    }

    fn read(ttd: &TempTedgeDir, name: &str) -> String {
        std::fs::read_to_string(ttd.path().join(name)).unwrap()
    }

    #[tokio::test]
    async fn deploys_the_workflow() {
        let ttd = TempTedgeDir::new();
        ShellExecuteBuilder::try_new(&ops_dir(&ttd)).await.unwrap();

        assert_eq!(read(&ttd, "shell_execute.toml"), BUILTIN);
        assert_eq!(read(&ttd, "shell_execute.toml.template"), BUILTIN);
    }

    #[tokio::test]
    async fn restores_a_removed_workflow() {
        let ttd = TempTedgeDir::new();
        ShellExecuteBuilder::try_new(&ops_dir(&ttd)).await.unwrap();
        std::fs::remove_file(ttd.path().join("shell_execute.toml")).unwrap();

        ShellExecuteBuilder::try_new(&ops_dir(&ttd)).await.unwrap();

        assert_eq!(read(&ttd, "shell_execute.toml"), BUILTIN);
    }

    /// While the tedge-command-plugin is installed, its own definition takes precedence.
    /// Once removed, the built-in definition is deployed.
    #[tokio::test]
    async fn adopts_the_workflow_of_the_community_package_once_removed() {
        let ttd = TempTedgeDir::new();
        std::fs::write(ttd.path().join("shell_execute.toml"), "community").unwrap();

        ShellExecuteBuilder::try_new(&ops_dir(&ttd)).await.unwrap();
        assert_eq!(read(&ttd, "shell_execute.toml"), "community");

        std::fs::remove_file(ttd.path().join("shell_execute.toml")).unwrap();
        ShellExecuteBuilder::try_new(&ops_dir(&ttd)).await.unwrap();
        assert_eq!(read(&ttd, "shell_execute.toml"), BUILTIN);
    }

    #[tokio::test]
    async fn preserves_a_customized_workflow() {
        let ttd = TempTedgeDir::new();
        ShellExecuteBuilder::try_new(&ops_dir(&ttd)).await.unwrap();
        std::fs::write(ttd.path().join("shell_execute.toml"), "customized").unwrap();

        ShellExecuteBuilder::try_new(&ops_dir(&ttd)).await.unwrap();

        assert_eq!(read(&ttd, "shell_execute.toml"), "customized");
        assert_eq!(read(&ttd, "shell_execute.toml.template"), BUILTIN);
    }

    #[tokio::test]
    async fn never_restores_a_disabled_workflow() {
        let ttd = TempTedgeDir::new();
        ShellExecuteBuilder::try_new(&ops_dir(&ttd)).await.unwrap();
        std::fs::remove_file(ttd.path().join("shell_execute.toml")).unwrap();
        ttd.file("shell_execute.toml.disabled");

        ShellExecuteBuilder::try_new(&ops_dir(&ttd)).await.unwrap();

        assert!(!ttd.path().join("shell_execute.toml").exists());
    }
}
