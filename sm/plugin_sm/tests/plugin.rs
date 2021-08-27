#[cfg(test)]
mod tests {

    use json_sm::{SoftwareError, SoftwareModule};
    use plugin_sm::plugin::{ExternalPluginCommand, Plugin};
    use std::{fs, io::Write, path::PathBuf, str::FromStr};
    #[tokio::test]
    #[cfg(not(tarpaulin))]
    async fn plugin_get_command_prepare() {
        // Prepare dummy plugin.
        let (plugin, _plugin_path) = get_dummy_plugin("test");

        // Call dummy plugin via plugin api.
        let res = plugin.prepare().await;

        // Expect to get Ok as plugin should exit with code 0.
        assert_eq!(res, Ok(()));
    }

    #[tokio::test]
    #[cfg(not(tarpaulin))]
    async fn plugin_get_command_finalize() {
        // Prepare dummy plugin.
        let (plugin, _plugin_path) = get_dummy_plugin("test");

        // Call dummy plugin via plugin api.
        let res = plugin.finalize().await;

        // Expect Ok as plugin should exit with code 0. If Ok, there is no more checks to be done.
        assert_eq!(res, Ok(()));
    }

    #[tokio::test]
    #[cfg(not(tarpaulin))]
    async fn plugin_get_command_list() {
        // Prepare dummy plugin with .0 which will give specific exit code ==0.
        let (plugin, _plugin_path) = get_dummy_plugin("test");
        let path = PathBuf::from_str("/tmp/.tedge_dummy_plugin").unwrap();
        if !&path.exists() {
            let () = fs::create_dir(&path).unwrap();
        }

        let mut file = tempfile::Builder::new()
            .suffix(".0")
            .tempfile_in(path)
            .unwrap();

        // Add content of the expected stdout to the dummy plugin.
        let content = r#"{"name":"abc","version":"1.0"}"#;
        let _a = file.write_all(content.as_bytes()).unwrap();

        // Create expected response.
        let module = SoftwareModule {
            module_type: Some("test".into()),
            name: "abc".into(),
            version: Some("1.0".into()),
            url: None,
        };
        let expected_response = vec![module];

        // Call plugin via API.
        let res = plugin.list().await;

        // Expect Ok as plugin should exit with code 0.
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), expected_response);
    }

    #[tokio::test]
    #[cfg(not(tarpaulin))]
    async fn plugin_get_command_install() {
        // Prepare dummy plugin with .0 which will give specific exit code ==0.
        let (plugin, _plugin_path) = get_dummy_plugin("test");
        let path = get_dummy_plugin_tmp_path();

        let mut file = tempfile::Builder::new()
            .suffix(".0")
            .tempfile_in(path)
            .unwrap();

        // Add content of the expected stdout to the dummy plugin.
        let content = r#"{"name":"abc","version":"1.0"}"#;
        let _a = file.write_all(content.as_bytes()).unwrap();

        // Create module to perform plugin install API call containing valid input.
        let module = SoftwareModule {
            module_type: Some("test".into()),
            name: "test".into(),
            version: None,
            url: None,
        };

        // Call plugin install via API.
        let res = plugin.install(&module).await;

        // Expect Ok as plugin should exit with code 0. If Ok, there is no response to assert.
        assert!(res.is_ok());
    }

    #[tokio::test]
    #[cfg(not(tarpaulin))]
    async fn plugin_get_command_remove() {
        // Prepare dummy plugin with .0 which will give specific exit code ==0.
        let (plugin, _plugin_path) = get_dummy_plugin("test");
        let path = get_dummy_plugin_tmp_path();

        let mut file = tempfile::Builder::new()
            .suffix(".0")
            .tempfile_in(path)
            .unwrap();

        // Add content of the expected stdout to the dummy plugin.
        let content = r#"{"name":"abc","version":"1.0"}"#;
        let _a = file.write_all(content.as_bytes()).unwrap();

        // Create module to perform plugin install API call containing valid input.
        let module = SoftwareModule {
            module_type: Some("test".into()),
            name: "test".into(),
            version: None,
            url: None,
        };

        // Call plugin remove API .
        let res = plugin.remove(&module).await;

        // Expect Ok as plugin should exit with code 0. If Ok, no more output to be validated.
        assert!(res.is_ok());
    }

    #[test]
    fn plugin_call_name_and_path() {
        let dummy_plugin_path = get_dummy_plugin_path();
        let plugin = ExternalPluginCommand::new("test", &dummy_plugin_path);
        assert_eq!(plugin.name, "test");
        assert_eq!(plugin.path, dummy_plugin_path);
    }

    #[test]
    fn plugin_check_module_type_both_same() {
        let dummy_plugin_path = get_dummy_plugin_path();
        let plugin = ExternalPluginCommand::new("test", &dummy_plugin_path);
        let module = SoftwareModule {
            module_type: Some("test".into()),
            name: "test".into(),
            version: None,
            url: None,
        };

        // Call plugin check_module_type API to validate if plugin exists.
        let res = plugin.check_module_type(&module);

        // Expect Ok as plugin registry shall return no error. If Ok, no more output to be validated.
        assert_eq!(res, Ok(()));
    }

    #[test]
    fn plugin_check_module_type_both_different() {
        // Create dummy plugin.
        let dummy_plugin_path = get_dummy_plugin_path();

        // Create new plugin in the registry with name `test`.
        let plugin = ExternalPluginCommand::new("test", &dummy_plugin_path);

        // Create test module with name `test2`.
        let module = SoftwareModule {
            module_type: Some("test2".into()),
            name: "test2".into(),
            version: None,
            url: None,
        };

        // Call plugin API to check if the plugin with name `test2` is registered.
        let res = plugin.check_module_type(&module);

        // Plugin is with name `test2` is not registered.
        assert_eq!(
            res,
            Err(SoftwareError::WrongModuleType {
                actual: "test".into(),
                expected: "test2".into()
            })
        );
    }

    #[test]
    fn plugin_check_module_type_default() {
        // Create dummy plugin.
        let dummy_plugin_path = get_dummy_plugin_path();

        let plugin = ExternalPluginCommand::new("test", &dummy_plugin_path);

        // Create software module without an explicit type.
        let module = SoftwareModule {
            module_type: None,
            name: "test".into(),
            version: None,
            url: None,
        };
        let res = plugin.check_module_type(&module);

        // A software module without an explicit type can be handled by any plugin, which in practice is the default plugin.
        assert_eq!(res, Ok(()));
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

    fn get_dummy_plugin(name: &str) -> (ExternalPluginCommand, PathBuf) {
        let dummy_plugin_path = get_dummy_plugin_path();
        let plugin = ExternalPluginCommand::new(name, &dummy_plugin_path);
        (plugin, dummy_plugin_path)
    }

    fn get_dummy_plugin_tmp_path() -> PathBuf {
        let path = PathBuf::from_str("/tmp/.tedge_dummy_plugin").unwrap();
        if !&path.exists() {
            let () = fs::create_dir(&path).unwrap();
        }
        path
    }
}
