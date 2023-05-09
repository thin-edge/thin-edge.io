#[cfg(test)]
mod tests {

    use plugin_sm::plugin_manager::ExternalPlugins;
    use plugin_sm::plugin_manager::Plugins;
    use std::fs::File;
    use std::path::PathBuf;
    use std::str::FromStr;
    use tempfile::NamedTempFile;

    #[test]
    fn plugin_manager_load_plugins_empty() {
        // Create empty plugins directory.
        let temp_dir = tempfile::tempdir().unwrap();
        let plugin_dir = temp_dir.path().to_owned();

        // Call open and load to register all plugins from given directory.
        let mut plugins = ExternalPlugins::open(plugin_dir, None, None).unwrap();
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
        let mut plugins = ExternalPlugins::open(plugin_dir, None, None).unwrap();
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
        let mut plugins = ExternalPlugins::open(plugin_dir, None, None).unwrap();
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

        // Call open and load to register all plugins from given directory.
        let mut plugins = ExternalPlugins::open(plugin_dir, None, None).unwrap();
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
        let plugin1 = create_some_plugin_in(&plugin_dir);
        let _res = std::fs::copy(get_dummy_plugin_path(), plugin1.path());
        let (_, _path) = plugin1.keep().unwrap();

        let plugin2 = create_some_plugin_in(&plugin_dir);
        let _res = std::fs::copy(get_dummy_plugin_path(), plugin2.path());
        let plugin_name2 = plugin2
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let (_, _path) = plugin2.keep().unwrap();

        let plugin3 = create_some_plugin_in(&plugin_dir);
        let _res = std::fs::copy(get_dummy_plugin_path(), plugin3.path());
        let (_, _path) = plugin3.keep().unwrap();

        let mut plugins =
            ExternalPlugins::open(plugin_dir.into_path(), Some(plugin_name2.clone()), None)
                .unwrap();
        plugins.load().unwrap();

        assert_eq!(
            plugins.by_software_type("default").unwrap().name,
            plugin_name2
        );
        assert_eq!(plugins.default().unwrap().name, plugin_name2);
    }

    #[test]
    fn implicit_default_plugin_with_only_one_plugin() {
        let plugin_dir = tempfile::tempdir().unwrap();

        let plugin = create_some_plugin_in(&plugin_dir);
        let _res = std::fs::copy(get_dummy_plugin_path(), plugin.path());
        let plugin_name = plugin
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let (_, _path) = plugin.keep().unwrap();

        let mut plugins = ExternalPlugins::open(plugin_dir.into_path(), None, None).unwrap();
        plugins.load().unwrap();

        assert_eq!(
            plugins.by_software_type("default").unwrap().name,
            plugin_name
        );
        assert_eq!(plugins.default().unwrap().name, plugin_name);
    }

    #[test]
    fn invalid_default_plugin_pass_through() -> anyhow::Result<()> {
        let plugin_dir = tempfile::tempdir().unwrap();
        let plugin_file_path = plugin_dir.path().join("apt");
        let _ = File::create(plugin_file_path).unwrap();

        let result = ExternalPlugins::open(plugin_dir.into_path(), Some("dummy".into()), None)?;
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

    fn get_dummy_plugin_path() -> PathBuf {
        // Return a path to a dummy plugin in target directory.
        let package_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();

        // To get the plugin binary path we need to find the `target` directory which is 3 levels above the `Cargo.toml` file of the package
        // CARGO_MANIFEST_DIR == ./thin-edge.io/crates/core/plugin_sm
        let dummy_plugin_path = PathBuf::from_str(package_dir.as_str())
            .unwrap()
            .parent() //./thin-edge.io/crates/core/
            .unwrap()
            .parent() // ./thin-edge.io/crates/
            .unwrap()
            .parent() // ./thin-edge.io/
            .unwrap()
            .join("target/debug/tedge-dummy-plugin");

        dummy_plugin_path
    }
}
