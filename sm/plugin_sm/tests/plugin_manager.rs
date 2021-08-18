#[cfg(test)]
mod tests {

    use std::fs::File;

    use assert_matches::assert_matches;
    use json_sm::SoftwareError;
    use plugin_sm::plugin_manager::{ExternalPlugins, Plugins};
    use std::{path::PathBuf, str::FromStr};
    use tempfile::NamedTempFile;

    #[test]
    fn plugin_manager_load_plugins_empty() {
        // Create empty plugins directory.
        let temp_dir = tempfile::tempdir().unwrap();
        let plugin_dir = temp_dir.path().to_owned();

        // Call open and load to register all plugins from given directory.
        let mut plugins = ExternalPlugins::open(plugin_dir, None).unwrap();
        let _ = plugins.load();

        // Plugins registry should not register any plugin as no files in the directory are present.
        assert!(plugins.empty());
    }

    #[test]
    fn plugin_manager_load_plugins_some_non_executables() {
        // Create empty plugins directory.
        let temp_dir = tempfile::tempdir().unwrap();

        // Add a plugin to the directory.
        let _file = create_some_plugin_in(&temp_dir);
        let plugin_dir = temp_dir.path().to_owned();

        // Call open and load to register all plugins from given directory.
        let mut plugins = ExternalPlugins::open(plugin_dir, None).unwrap();
        let _ = plugins.load();

        // Registry has registered no plugins.
        assert!(plugins.empty());
    }

    #[test]
    fn plugin_manager_load_plugins_some_by_plugins_none() {
        // Create empty plugins directory.
        let temp_dir = tempfile::tempdir().unwrap();

        // Add a plugin to the directory.
        let _file = create_some_plugin_in(&temp_dir);
        let _file = create_some_plugin_in(&temp_dir);
        let plugin_dir = temp_dir.path().to_owned();

        // Call open and load to register all plugins from given directory.
        let mut plugins = ExternalPlugins::open(plugin_dir, None).unwrap();
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
        let plugin1 = create_some_plugin_in(&temp_dir);
        let plugin2 = create_some_plugin_in(&temp_dir);

        // Prepare path of temp file as it serves as plugin module type.
        let plugin_name1 = plugin1
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let plugin_name2 = plugin2
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();

        // Make the file an actual executable, copy dummy_plugin data into.
        let _res = std::fs::copy(get_dummy_plugin_path(), plugin1.path());
        let _res = std::fs::copy(get_dummy_plugin_path(), plugin2.path());

        // Keep the file to close the handle.
        let (_, _path) = plugin1.keep().unwrap();
        let (_, _path) = plugin2.keep().unwrap();

        let plugin_dir = temp_dir.path().to_owned();
        dbg!(&plugin_dir);

        // Call open and load to register all plugins from given directory.
        let mut plugins = ExternalPlugins::open(plugin_dir, None).unwrap();
        let _ = plugins.load();

        // Plugin registry shall have registered plugin with name as the file in plugin directory.
        assert!(plugins.by_software_type(&plugin_name1).is_some());
        assert!(plugins.by_software_type(&plugin_name2).is_some());
        assert!(plugins.by_file_extension(&plugin_name1).is_none());
        assert!(plugins.default().is_none());
    }

    #[test]
    fn explicit_default_plugin() {
        let plugin_dir = tempfile::tempdir().unwrap();
        let _ = File::create(plugin_dir.path().join("apt")).unwrap();
        let _ = File::create(plugin_dir.path().join("snap")).unwrap();
        let _ = File::create(plugin_dir.path().join("docker")).unwrap();

        let mut plugins =
            ExternalPlugins::open(plugin_dir.into_path(), Some("apt".into())).unwrap();
        plugins.load().unwrap();

        assert_eq!(plugins.by_software_type("default").unwrap().name, "apt");
        assert_eq!(plugins.default().unwrap().name, "apt");
    }

    #[test]
    fn implicit_default_plugin_with_only_one_plugin() {
        let plugin_dir = tempfile::tempdir().unwrap();
        let plugin_file_path = plugin_dir.path().join("apt");
        let _ = File::create(plugin_file_path).unwrap();

        let mut plugins = ExternalPlugins::open(plugin_dir.into_path(), None).unwrap();
        plugins.load().unwrap();

        assert_eq!(plugins.by_software_type("default").unwrap().name, "apt");
        assert_eq!(plugins.default().unwrap().name, "apt");
    }

    #[test]
    fn invalid_default_plugin() {
        let plugin_dir = tempfile::tempdir().unwrap();
        let plugin_file_path = plugin_dir.path().join("apt");
        let _ = File::create(plugin_file_path).unwrap();

        let result = ExternalPlugins::open(plugin_dir.into_path(), Some("dummy".into()));
        assert_matches!(result.unwrap_err(), SoftwareError::InvalidDefaultPlugin(_));
    }

    fn create_some_plugin_in(dir: &tempfile::TempDir) -> NamedTempFile {
        tempfile::Builder::new()
            .suffix(".0")
            .tempfile_in(dir)
            .unwrap()
    }

    fn get_dummy_plugin_path() -> PathBuf {
        // Return a path to a dummy plugin in target directory.
        let package_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let dummy_plugin_path = PathBuf::from_str(package_dir.as_str())
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("target/debug/tedge_dummy_plugin");

        dummy_plugin_path
    }
}
