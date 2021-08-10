#[cfg(test)]
mod tests {
    // Tests: calls to each of Plugin API commands: prepare, list, install, remove, finalize, version
    // Check exit codes
    // Check some sample output
    // use Dummy plugin
    // Add crash test due to no timeout plugin may never return
    // Multiple version of invalid output, eg: JSON Lines use '\r\n' separator, JSON instead, garbage
    // Try 10000 lines output
    //

    use json_sm::{SoftwareError, SoftwareModule};
    use plugin_sm::plugin::{ExternalPluginCommand, Plugin};
    use std::{fs, io::Write, path::PathBuf, str::FromStr};
    #[tokio::test]
    async fn plugin_get_command_prepare() {
        let (plugin, _plugin_path) = get_dummy_plugin("test");

        let res = plugin.prepare().await;

        assert_eq!(res, Ok(()));
    }

    #[tokio::test]
    async fn plugin_get_command_finalize() {
        let (plugin, _plugin_path) = get_dummy_plugin("test");

        let res = plugin.finalize().await;

        assert_eq!(res, Ok(()));
    }

    #[tokio::test]
    async fn plugin_get_command_list() {
        let (plugin, _plugin_path) = get_dummy_plugin("test");
        let path = PathBuf::from_str("/tmp/.tedge_dummy_plugin").unwrap();
        if !&path.exists() {
            let () = fs::create_dir(&path).unwrap();
        }

        let mut file = tempfile::Builder::new()
            .suffix(".0")
            .tempfile_in(path)
            .unwrap();

        let content = r#"{"name":"abc","version":"1.0"}"#;
        let _a = file.write_all(content.as_bytes()).unwrap();

        let res = plugin.list().await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn plugin_get_command_install() {
        let (plugin, _plugin_path) = get_dummy_plugin("test");
        let path = get_dummy_plugin_tmp_path();

        let mut file = tempfile::Builder::new()
            .suffix(".0")
            .tempfile_in(path)
            .unwrap();

        let content = r#"{"name":"abc","version":"1.0"}"#;
        let _a = file.write_all(content.as_bytes()).unwrap();

        let module = SoftwareModule {
            module_type: Some("test".into()),
            name: "test".into(),
            version: None,
            url: None,
        };
        let res = plugin.install(&module).await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn plugin_get_command_remove() {
        let (plugin, _plugin_path) = get_dummy_plugin("test");
        let path = get_dummy_plugin_tmp_path();

        let mut file = tempfile::Builder::new()
            .suffix(".0")
            .tempfile_in(path)
            .unwrap();

        let content = r#"{"name":"abc","version":"1.0"}"#;
        let _a = file.write_all(content.as_bytes()).unwrap();

        let module = SoftwareModule {
            module_type: Some("test".into()),
            name: "test".into(),
            version: None,
            url: None,
        };
        let res = plugin.remove(&module).await;
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
        let res = plugin.check_module_type(&module);

        assert_eq!(res, Ok(()));
    }

    #[test]
    fn plugin_check_module_type_both_different() {
        let dummy_plugin_path = get_dummy_plugin_path();
        let plugin = ExternalPluginCommand::new("test", &dummy_plugin_path);
        let module = SoftwareModule {
            module_type: Some("test2".into()),
            name: "test2".into(),
            version: None,
            url: None,
        };
        let res = plugin.check_module_type(&module);

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
        let dummy_plugin_path = get_dummy_plugin_path();
        let plugin = ExternalPluginCommand::new("test", &dummy_plugin_path);
        let module = SoftwareModule {
            module_type: None,
            name: "test".into(),
            version: None,
            url: None,
        };
        let res = plugin.check_module_type(&module);

        assert_eq!(
            res,
            Err(SoftwareError::WrongModuleType {
                actual: "test".into(),
                expected: "default".into()
            })
        );
    }

    fn get_dummy_plugin_path() -> PathBuf {
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
