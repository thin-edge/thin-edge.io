#[cfg(test)]
mod tests {

    use agent_mapper_interface::{SoftwareError, SoftwareModule, SoftwareModuleUpdate};
    use assert_matches::assert_matches;
    use plugin_sm::plugin::{deserialize_module_info, ExternalPluginCommand, Plugin};
    use serial_test::serial;
    use std::{fs, io::Write, path::PathBuf, str::FromStr};
    use test_case::test_case;
    use tokio::fs::File;
    use tokio::io::BufWriter;

    #[tokio::test]
    #[serial]
    async fn plugin_get_command_prepare() {
        // Prepare dummy plugin.
        let (plugin, _plugin_path) = get_dummy_plugin("test");

        // Call dummy plugin via plugin api.
        let mut logger = dev_null().await;
        let res = plugin.prepare(&mut logger).await;

        // Expect to get Ok as plugin should exit with code 0.
        assert_eq!(res, Ok(()));
    }

    #[tokio::test]
    #[serial]
    async fn plugin_get_command_finalize() {
        // Prepare dummy plugin.
        let (plugin, _plugin_path) = get_dummy_plugin("test");

        // Call dummy plugin via plugin api.
        let mut logger = dev_null().await;
        let res = plugin.finalize(&mut logger).await;

        // Expect Ok as plugin should exit with code 0. If Ok, there is no more checks to be done.
        assert_eq!(res, Ok(()));
    }

    #[test_case("abc", Some("1.0")  ; "with version")]
    #[test_case("abc",None  ; "without version")]
    fn desrialize_plugin_result(module_name: &str, version: Option<&str>) {
        let mut data = String::from(module_name);
        match version {
            Some(v) => {
                data.push_str("\t");
                data.push_str(v)
            }
            None => {}
        }

        let mut expected_software_list = Vec::new();

        expected_software_list.push(SoftwareModule {
            name: module_name.into(),
            version: version.map(|s| s.to_string()),
            module_type: Some("test".into()),
            file_path: None,
            url: None,
        });

        let software_list = deserialize_module_info("test".into(), data.as_bytes()).unwrap();
        assert_eq!(expected_software_list, software_list);
    }

    #[test]
    fn desrialize_plugin_result_with_trailing_tab() {
        let data = "abc\t";

        let mut expected_software_list = Vec::new();

        expected_software_list.push(SoftwareModule {
            name: "abc".into(),
            version: None,
            module_type: Some("test".into()),
            file_path: None,
            url: None,
        });

        let software_list = deserialize_module_info("test".into(), data.as_bytes()).unwrap();
        assert_eq!(expected_software_list, software_list);
    }

    #[tokio::test]
    #[serial]
    async fn plugin_get_command_list_with_version() {
        // Prepare dummy plugin with .0 which will give specific exit code ==0.
        let (plugin, _plugin_path) = get_dummy_plugin("test");
        let path = get_dummy_plugin_tmp_path();

        let mut file = tempfile::Builder::new()
            .suffix(".0")
            .tempfile_in(path)
            .unwrap();

        // Add content of the expected stdout to the dummy plugin.
        let content = "abc\t1.0";
        let _a = file.write_all(content.as_bytes()).unwrap();

        // Create expected response.
        let module = SoftwareModule {
            module_type: Some("test".into()),
            name: "abc".into(),
            version: Some("1.0".into()),
            url: None,
            file_path: None,
        };
        let expected_response = vec![module];

        // Call plugin via API.
        let mut logger = dev_null().await;
        let res = plugin.list(&mut logger).await;

        // Expect Ok as plugin should exit with code 0.
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), expected_response);
    }

    #[tokio::test]
    #[serial]
    async fn plugin_get_command_list_without_version() {
        // Prepare dummy plugin with .0 which will give specific exit code ==0.
        let (plugin, _plugin_path) = get_dummy_plugin("test");
        let path = get_dummy_plugin_tmp_path();

        let mut file = tempfile::Builder::new()
            .suffix(".0")
            .tempfile_in(path)
            .unwrap();

        // Add content of the expected stdout to the dummy plugin.
        let content = "abc";
        let _a = file.write_all(content.as_bytes()).unwrap();

        // Create expected response.
        let module = SoftwareModule {
            module_type: Some("test".into()),
            name: "abc".into(),
            version: None,
            url: None,
            file_path: None,
        };
        let expected_response = vec![module];

        // Call plugin via API.
        let mut logger = dev_null().await;
        let res = plugin.list(&mut logger).await;

        // Expect Ok as plugin should exit with code 0.
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), expected_response);
    }

    #[tokio::test]
    #[serial]
    async fn plugin_get_command_install() {
        // Prepare dummy plugin with .0 which will give specific exit code ==0.
        let (plugin, _plugin_path) = get_dummy_plugin("test");
        let path = get_dummy_plugin_tmp_path();

        let mut file = tempfile::Builder::new()
            .suffix(".0")
            .tempfile_in(path)
            .unwrap();

        // Add content of the expected stdout to the dummy plugin.
        let content = "abc\t1.0";
        let _a = file.write_all(content.as_bytes()).unwrap();

        // Create module to perform plugin install API call containing valid input.
        let module = SoftwareModule {
            module_type: Some("test".into()),
            name: "test".into(),
            version: None,
            url: None,
            file_path: None,
        };

        // Call plugin install via API.
        let mut logger = dev_null().await;
        let res = plugin.install(&module, &mut logger).await;

        // Expect Ok as plugin should exit with code 0. If Ok, there is no response to assert.
        assert!(res.is_ok());
    }

    #[tokio::test]
    #[serial]
    async fn plugin_get_command_remove() {
        // Prepare dummy plugin with .0 which will give specific exit code ==0.
        let (plugin, _plugin_path) = get_dummy_plugin("test");
        let path = get_dummy_plugin_tmp_path();

        let mut file = tempfile::Builder::new()
            .suffix(".0")
            .tempfile_in(path)
            .unwrap();

        // Add content of the expected stdout to the dummy plugin.
        let content = "abc\t1.0";
        let _a = file.write_all(content.as_bytes()).unwrap();

        // Create module to perform plugin install API call containing valid input.
        let module = SoftwareModule {
            module_type: Some("test".into()),
            name: "test".into(),
            version: None,
            url: None,
            file_path: None,
        };

        // Call plugin remove API .
        let mut logger = dev_null().await;
        let res = plugin.remove(&module, &mut logger).await;

        // Expect Ok as plugin should exit with code 0. If Ok, no more output to be validated.
        assert!(res.is_ok());
    }

    #[test]
    #[serial]
    fn plugin_call_name_and_path() {
        let dummy_plugin_path = get_dummy_plugin_path();
        let plugin = ExternalPluginCommand::new("test", &dummy_plugin_path);
        assert_eq!(plugin.name, "test");
        assert_eq!(plugin.path, dummy_plugin_path);
    }

    #[test]
    #[serial]
    fn plugin_check_module_type_both_same() {
        let dummy_plugin_path = get_dummy_plugin_path();
        let plugin = ExternalPluginCommand::new("test", &dummy_plugin_path);
        let module = SoftwareModule {
            module_type: Some("test".into()),
            name: "test".into(),
            version: None,
            url: None,
            file_path: None,
        };

        // Call plugin check_module_type API to validate if plugin exists.
        let res = plugin.check_module_type(&module);

        // Expect Ok as plugin registry shall return no error. If Ok, no more output to be validated.
        assert_eq!(res, Ok(()));
    }

    #[test]
    #[serial]
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
            file_path: None,
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
    #[serial]
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
            file_path: None,
        };
        let res = plugin.check_module_type(&module);

        // A software module without an explicit type can be handled by any plugin, which in practice is the default plugin.
        assert_eq!(res, Ok(()));
    }

    #[tokio::test]
    #[serial]
    async fn plugin_get_command_update_list() {
        // Prepare dummy plugin with .0 which will give specific exit code ==0.
        let (plugin, _plugin_path) = get_dummy_plugin("test");

        // Create list of modules to perform plugin update-list API call containing valid input.
        let module1 = SoftwareModule {
            module_type: Some("test".into()),
            name: "test1".into(),
            version: None,
            url: None,
            file_path: None,
        };
        let module2 = SoftwareModule {
            module_type: Some("test".into()),
            name: "test2".into(),
            version: None,
            url: None,
            file_path: None,
        };

        let mut logger = dev_null().await;
        // Call plugin update-list via API.
        let res = plugin
            .update_list(
                &vec![
                    SoftwareModuleUpdate::Install { module: module1 },
                    SoftwareModuleUpdate::Remove { module: module2 },
                ],
                &mut logger,
            )
            .await;

        // Expect Ok as plugin should exit with code 0. If Ok, there is no response to assert.
        assert_matches!(res, Err(SoftwareError::UpdateListNotSupported(_)));
    }

    // Test validating if the plugin will fall back to `install` and `remove` options if the `update-list` option is not supported
    #[tokio::test]
    #[serial]
    async fn plugin_command_update_list_fallback() {
        // Prepare dummy plugin with .0 which will give specific exit code ==0.
        let (plugin, _plugin_path) = get_dummy_plugin("test");

        // Create list of modules to perform plugin update-list API call containing valid input.
        let module1 = SoftwareModule {
            module_type: Some("test".into()),
            name: "test1".into(),
            version: None,
            url: None,
            file_path: None,
        };
        let module2 = SoftwareModule {
            module_type: Some("test".into()),
            name: "test2".into(),
            version: None,
            url: None,
            file_path: None,
        };

        let mut logger = dev_null().await;
        // Call plugin update-list via API.
        let errors = plugin
            .apply_all(
                vec![
                    SoftwareModuleUpdate::Install { module: module1 },
                    SoftwareModuleUpdate::Remove { module: module2 },
                ],
                &mut logger,
            )
            .await;

        // Expect Ok as plugin should exit with code 0. If Ok, there is no response to assert.
        assert!(errors.is_empty());
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
            .join("target/debug/tedge_dummy_plugin");

        dummy_plugin_path
    }

    fn get_dummy_plugin(name: &str) -> (ExternalPluginCommand, PathBuf) {
        let dummy_plugin_path = get_dummy_plugin_path();
        let plugin = ExternalPluginCommand {
            name: name.into(),
            path: dummy_plugin_path.clone(),
            sudo: None,
        };
        (plugin, dummy_plugin_path)
    }

    fn get_dummy_plugin_tmp_path() -> PathBuf {
        let path = PathBuf::from_str("/tmp/.tedge_dummy_plugin").unwrap();
        if !&path.exists() {
            let () = fs::create_dir(&path).unwrap();
        }
        path
    }

    async fn dev_null() -> BufWriter<File> {
        let log_file = File::create("/dev/null").await.unwrap();
        BufWriter::new(log_file)
    }
}
