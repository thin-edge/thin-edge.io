#[cfg(test)]
mod tests {

    use plugin_sm::{
        plugin::Plugin,
        plugin_manager::{ExternalPlugins, Plugins},
    };
    use tempfile::NamedTempFile;

    #[test]
    fn plugin_manager_load_plugins_empty() {
        let temp_dir = tempfile::tempdir().unwrap();
        let plugin_dir = temp_dir.path().to_owned();
        let mut plugins = ExternalPlugins::open(plugin_dir).unwrap();
        let _ = plugins.load();
        assert!(plugins.empty());
    }

    #[test]
    fn plugin_manager_load_plugins_some() {
        let temp_dir = tempfile::tempdir().unwrap();
        let _file = create_some_plugin_in(&temp_dir);
        let plugin_dir = temp_dir.path().to_owned();
        let mut plugins = ExternalPlugins::open(plugin_dir).unwrap();
        let _ = plugins.load();
        assert!(!plugins.empty());
    }

    #[test]
    fn plugin_manager_load_plugins_some_by_plugins_none() {
        let temp_dir = tempfile::tempdir().unwrap();
        let _file = create_some_plugin_in(&temp_dir);
        let plugin_dir = temp_dir.path().to_owned();
        let mut plugins = ExternalPlugins::open(plugin_dir).unwrap();
        let _ = plugins.load();

        assert!(plugins.by_software_type("test").is_none());
        assert!(plugins.by_file_extension("test").is_none());
        assert!(plugins.default().is_none());
    }

    #[test]
    fn plugin_manager_load_plugins_some_by_plugins_some() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file = create_some_plugin_in(&temp_dir);
        let plugin_dir = temp_dir.path().to_owned();
        let mut plugins = ExternalPlugins::open(plugin_dir).unwrap();
        let _ = plugins.load();

        let plugin_name = file.path().file_name().unwrap().to_str().unwrap();

        assert!(plugins.by_software_type(plugin_name).is_some());
        assert!(plugins.by_file_extension(plugin_name).is_none());
        assert!(plugins.default().is_none());
    }

    fn create_some_plugin_in(dir: &tempfile::TempDir) -> NamedTempFile {
        tempfile::Builder::new().tempfile_in(dir).unwrap()
    }
}
