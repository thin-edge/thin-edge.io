#[cfg(test)]
mod tests {

    use plugin_sm::plugin_manager::{ExternalPlugins, Plugins};
    use tempfile::NamedTempFile;

    #[test]
    fn plugin_manager_load_plugins_empty() {
        // Create empty plugins directory.
        let temp_dir = tempfile::tempdir().unwrap();
        let plugin_dir = temp_dir.path().to_owned();

        // Call open and load to register all plugins from given directory.
        let mut plugins = ExternalPlugins::open(plugin_dir).unwrap();
        let _ = plugins.load();

        // Plugins registry should not register any plugin as no files in the directory are present.
        assert!(plugins.empty());
    }

    #[test]
    fn plugin_manager_load_plugins_some() {
        // Create empty plugins directory.
        let temp_dir = tempfile::tempdir().unwrap();

        // Add a plugin to the directory.
        let _file = create_some_plugin_in(&temp_dir);
        let plugin_dir = temp_dir.path().to_owned();

        // Call open and load to register all plugins from given directory.
        let mut plugins = ExternalPlugins::open(plugin_dir).unwrap();
        let _ = plugins.load();

        // Registry has registered at least one plugin.
        assert!(!plugins.empty());
    }

    #[test]
    fn plugin_manager_load_plugins_some_by_plugins_none() {
        // Create empty plugins directory.
        let temp_dir = tempfile::tempdir().unwrap();

        // Add a plugin to the directory.
        let _file = create_some_plugin_in(&temp_dir);
        let plugin_dir = temp_dir.path().to_owned();

        // Call open and load to register all plugins from given directory.
        let mut plugins = ExternalPlugins::open(plugin_dir).unwrap();
        let _ = plugins.load();

        // Check if registry has loaded plugin of type `test`.
        assert!(plugins.by_software_type("test").is_none());
        assert!(plugins.by_file_extension("test").is_none());
        assert!(plugins.default().is_none());
    }

    #[test]
    fn plugin_manager_load_plugins_some_by_plugins_some() {
        // Create empty plugins directory.
        let temp_dir = tempfile::tempdir().unwrap();

        // Add a plugin to the directory.
        let file = create_some_plugin_in(&temp_dir);
        let plugin_dir = temp_dir.path().to_owned();

        // Call open and load to register all plugins from given directory.
        let mut plugins = ExternalPlugins::open(plugin_dir).unwrap();
        let _ = plugins.load();

        // Prepare path of temp file as it serves as plugin module type.
        let plugin_name = file.path().file_name().unwrap().to_str().unwrap();

        // Plugin registry shall have registered plugin with name as the file in plugin directory.
        assert!(plugins.by_software_type(plugin_name).is_some());
        assert!(plugins.by_file_extension(plugin_name).is_none());
        assert!(plugins.default().is_none());
    }

    fn create_some_plugin_in(dir: &tempfile::TempDir) -> NamedTempFile {
        tempfile::Builder::new().tempfile_in(dir).unwrap()
    }
}
