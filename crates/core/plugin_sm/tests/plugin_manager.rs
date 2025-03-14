#[cfg(test)]
mod tests {

    use plugin_sm::plugin_manager::ExternalPlugins;
    use plugin_sm::plugin_manager::Plugins;
    use std::fs::File;
    use tedge_config::SudoCommandBuilder;
    use tedge_config::TEdgeConfigLocation;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn plugin_manager_load_plugins_empty() {
        // Create empty plugins directory.
        let temp_dir = tempfile::tempdir().unwrap();
        let plugin_dir = temp_dir.path().to_owned();

        // Call open and load to register all plugins from given directory.
        let mut plugins = ExternalPlugins::open(
            plugin_dir,
            None,
            SudoCommandBuilder::enabled(false),
            TEdgeConfigLocation::default(),
        )
        .await
        .unwrap();
        let _ = plugins.load().await;

        // Plugins registry should not register any plugin as no files in the directory are present.
        assert!(plugins.empty());
    }

    #[tokio::test]
    async fn plugin_manager_load_plugins_some_by_plugins_none() {
        // Create empty plugins directory.
        let temp_dir = tempfile::tempdir().unwrap();

        // Add a plugin to the directory.
        let _file = create_some_plugin_in(&temp_dir);
        let _file = create_some_plugin_in(&temp_dir);
        let plugin_dir = temp_dir.path().to_owned();

        // Call open and load to register all plugins from given directory.
        let mut plugins = ExternalPlugins::open(
            plugin_dir,
            None,
            SudoCommandBuilder::enabled(false),
            TEdgeConfigLocation::default(),
        )
        .await
        .unwrap();
        let _ = plugins.load().await;

        // Check if registry has loaded plugin of type `test`.
        assert!(plugins.by_software_type("test").is_none());
        assert!(plugins.by_file_extension("test").is_none());
        assert!(plugins.default().is_none());
    }

    #[tokio::test]
    async fn invalid_default_plugin_pass_through() -> anyhow::Result<()> {
        let plugin_dir = tempfile::tempdir().unwrap();
        let plugin_file_path = plugin_dir.path().join("apt");
        let _ = File::create(plugin_file_path).unwrap();

        let result = ExternalPlugins::open(
            plugin_dir.into_path(),
            Some("dummy".into()),
            SudoCommandBuilder::enabled(false),
            TEdgeConfigLocation::default(),
        )
        .await?;
        assert!(result.empty());
        assert!(result.default().is_none());

        Ok(())
    }

    fn create_some_plugin_in(dir: &tempfile::TempDir) -> NamedTempFile {
        tempfile::Builder::new()
            .suffix(".0")
            .tempfile_in(dir)
            .unwrap()
    }
}
