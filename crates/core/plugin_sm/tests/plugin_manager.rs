#[cfg(test)]
mod tests {
    use plugin_sm::plugin_manager::ExternalPlugins;
    use plugin_sm::plugin_manager::Plugins;
    use std::fs::File;
    use tedge_config::SudoCommandBuilder;
    use tedge_test_utils::fs::TempTedgeDir;

    #[tokio::test]
    async fn plugin_manager_load_plugins_empty() {
        // Create empty plugins directory.
        let config_dir = TempTedgeDir::new();
        let plugin_dir = config_dir.dir("sm-plugins");

        // Call open and load to register all plugins from given directory.
        let mut plugins = ExternalPlugins::open(
            plugin_dir.path(),
            None,
            SudoCommandBuilder::enabled(false),
            config_dir.utf8_path_buf(),
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
        let config_dir = TempTedgeDir::new();
        let plugin_dir = config_dir.dir("sm-plugins");

        // Add a plugin to the directory.
        plugin_dir.file("a-plugin.0");
        plugin_dir.file("another-plugin.0");

        // Call open and load to register all plugins from given directory.
        let mut plugins = ExternalPlugins::open(
            plugin_dir.path(),
            None,
            SudoCommandBuilder::enabled(false),
            config_dir.utf8_path_buf(),
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
        let config_dir = TempTedgeDir::new();
        let plugin_dir = config_dir.dir("sm-plugins");
        let plugin_file_path = plugin_dir.path().join("apt");
        let _ = File::create(plugin_file_path).unwrap();

        let result = ExternalPlugins::open(
            plugin_dir.path(),
            Some("dummy".into()),
            SudoCommandBuilder::enabled(false),
            config_dir.utf8_path_buf(),
        )
        .await?;
        assert!(result.empty());
        assert!(result.default().is_none());

        Ok(())
    }
}
