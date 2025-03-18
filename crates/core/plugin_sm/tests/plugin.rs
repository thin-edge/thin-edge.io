#[cfg(test)]
mod tests {
    use certificate::CloudRootCerts;
    use plugin_sm::plugin::deserialize_module_info;
    use plugin_sm::plugin::sm_path;
    use plugin_sm::plugin::ExternalPluginCommand;
    use std::io::Write;
    use std::path::Path;
    use std::path::PathBuf;
    use tedge_api::SoftwareError;
    use tedge_api::SoftwareModule;
    use tedge_config::SudoCommandBuilder;
    use tedge_config::TEdgeConfigLocation;
    use test_case::test_case;

    #[test_case("abc", Some("1.0")  ; "with version")]
    #[test_case("abc",None  ; "without version")]
    fn deserialize_plugin_result(module_name: &str, version: Option<&str>) {
        let mut data = String::from(module_name);
        if let Some(v) = version {
            data.push('\t');
            data.push_str(v);
        }

        let expected_software_list = vec![SoftwareModule {
            name: module_name.into(),
            version: version.map(|s| s.to_string()),
            module_type: Some("test".into()),
            file_path: None,
            url: None,
        }];

        let software_list = deserialize_module_info("test".into(), data.as_bytes()).unwrap();
        assert_eq!(expected_software_list, software_list);
    }

    #[test]
    fn deserialize_plugin_result_with_trailing_tab() {
        let data = "abc\t";

        let expected_software_list = vec![SoftwareModule {
            name: "abc".into(),
            version: None,
            module_type: Some("test".into()),
            file_path: None,
            url: None,
        }];

        let software_list = deserialize_module_info("test".into(), data.as_bytes()).unwrap();
        assert_eq!(expected_software_list, software_list);
    }

    #[test]
    fn plugin_call_name_and_path() -> Result<(), anyhow::Error> {
        let dummy_plugin_path = get_dummy_plugin_path();

        let tmpfile = make_config(100)?;
        let config_location =
            TEdgeConfigLocation::from_custom_root(tmpfile.path().to_str().unwrap());
        let config = tedge_config::TEdgeConfig::try_new(config_location)?;

        let plugin = ExternalPluginCommand::new(
            "test",
            &dummy_plugin_path,
            SudoCommandBuilder::enabled(false),
            config.software.plugin.max_packages,
            None,
            None,
            config.http.client.auth.identity()?,
            config.cloud_root_certs(),
        );
        assert_eq!(plugin.name, "test");
        assert_eq!(plugin.path, dummy_plugin_path);
        assert_eq!(plugin.max_packages, config.software.plugin.max_packages);
        Ok(())
    }

    #[test]
    fn plugin_check_module_type_both_same() {
        let dummy_plugin_path = get_dummy_plugin_path();

        let plugin = ExternalPluginCommand::new(
            "test",
            dummy_plugin_path,
            SudoCommandBuilder::enabled(false),
            100,
            None,
            None,
            None,
            CloudRootCerts::from([]),
        );

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
    fn plugin_check_module_type_both_different() {
        // Create dummy plugin.
        let dummy_plugin_path = get_dummy_plugin_path();

        // Create new plugin in the registry with name `test`.
        let plugin = ExternalPluginCommand::new(
            "test",
            dummy_plugin_path,
            SudoCommandBuilder::enabled(false),
            100,
            None,
            None,
            None,
            CloudRootCerts::from([]),
        );

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
    fn plugin_check_module_type_default() {
        // Create dummy plugin.
        let dummy_plugin_path = get_dummy_plugin_path();

        let plugin = ExternalPluginCommand::new(
            "test",
            dummy_plugin_path,
            SudoCommandBuilder::enabled(false),
            100,
            None,
            None,
            None,
            CloudRootCerts::from([]),
        );

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

    #[test_case("abc", &Some("1:2.3.4567-8~1234".to_string()), "/tmp", PathBuf::from("/tmp/abc_1%3a2.3.4567-8~1234") ; "with special character")]
    fn handle_special_characters_in_module_version(
        name: &str,
        version: &Option<String>,
        target_dir_path: impl AsRef<Path>,
        expected_path: PathBuf,
    ) {
        let res = sm_path(name, version, target_dir_path);

        assert_eq!(res, expected_path);
    }

    fn get_dummy_plugin_path() -> PathBuf {
        // To get the plugin binary path we need to find the `target` directory which is 3 levels above the `Cargo.toml` file of the package
        // CARGO_MANIFEST_DIR == ./thin-edge.io/crates/core/plugin_sm
        let dummy_plugin_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent() //./thin-edge.io/crates/core/
            .unwrap()
            .parent() // ./thin-edge.io/crates/
            .unwrap()
            .parent() // ./thin-edge.io/
            .unwrap()
            .join("target/debug/tedge-dummy-plugin");

        dummy_plugin_path
    }

    fn make_config(max_packages: u32) -> Result<tempfile::TempDir, anyhow::Error> {
        let dir = tempfile::TempDir::new().unwrap();
        let toml_conf = &format!("[software]\nmax_packages = {max_packages}");

        let config_location = TEdgeConfigLocation::from_custom_root(dir.path());
        let mut file = std::fs::File::create(config_location.tedge_config_file_path())?;
        file.write_all(toml_conf.as_bytes())?;
        Ok(dir)
    }
}
