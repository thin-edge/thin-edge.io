mod tests {

    use predicates::prelude::*; // Used for writing assertions

    fn tedge_command<I, S>(args: I) -> Result<assert_cmd::Command, Box<dyn std::error::Error>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let path: &str = "tedge";
        let mut cmd = assert_cmd::Command::cargo_bin(path)?;
        cmd.args(args);
        Ok(cmd)
    }

    #[test]
    fn run_help() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = tedge_command(&["--help"])?;

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("USAGE"));

        Ok(())
    }

    #[test]
    fn run_version() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = tedge_command(&["-V"])?;

        let version_string = format!("tedge {}", env!("CARGO_PKG_VERSION"));

        cmd.assert()
            .success()
            .stdout(predicate::str::starts_with(version_string));

        Ok(())
    }

    #[test]
    fn run_create_certificate() -> Result<(), Box<dyn std::error::Error>> {
        let tempdir = tempfile::tempdir()?;
        let device_id = "test";
        let cert_path = temp_path(&tempdir, "test-cert.pem");
        let key_path = temp_path(&tempdir, "test-key.pem");

        let mut create_cmd = tedge_command(&[
            "cert",
            "create",
            "--id",
            device_id,
            "--cert-path",
            &cert_path,
            "--key-path",
            &key_path,
        ])?;

        let mut show_cmd = tedge_command(&["cert", "show", "--cert-path", &cert_path])?;

        let mut remove_cmd = tedge_command(&[
            "cert",
            "remove",
            "--cert-path",
            &cert_path,
            "--key-path",
            &key_path,
        ])?;

        // The remove command can be run when there is no certificate
        remove_cmd.assert().success();

        // The create command create a certificate
        create_cmd.assert().success();

        // The certificate use the device id as CN
        show_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(format!("CN={},", device_id)));

        // When a certificate exists, it is not over-written by the create command
        create_cmd
            .assert()
            .failure()
            .stderr(predicate::str::contains("A certificate already exists"));

        // The remove command remove the certificate
        remove_cmd.assert().success();

        // which can no more be displayed
        show_cmd
            .assert()
            .failure()
            .stderr(predicate::str::contains("Missing file"));

        // The a new certificate can then be created.
        create_cmd.assert().success();

        Ok(())
    }

    #[test]
    fn run_config_set_get_unset() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_dir_path = temp_dir.path();
        let test_home_str = temp_dir_path.to_str().unwrap();

        let device_id = "test";

        let mut get_device_id_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "get", "device-id"])?;

        get_device_id_cmd
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "The provided config key: device-id is not set",
            ));

        let mut set_device_id_cmd = tedge_command_with_test_home(
            test_home_str,
            &["config", "set", "device-id", device_id],
        )?;

        set_device_id_cmd.assert().success();

        get_device_id_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(device_id));

        let mut unset_device_id_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "unset", "device-id"])?;

        unset_device_id_cmd.assert().success();

        get_device_id_cmd
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "The provided config key: device-id is not set",
            ));

        Ok(())
    }

    #[test]
    fn run_config_defaults() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_dir_path = temp_dir.path();
        let test_home_str = temp_dir_path.to_str().unwrap();

        let cert_path = temp_path(&temp_dir, "certificate/tedge-certificate.pem");
        let key_path = temp_path(&temp_dir, "certificate/tedge-private-key.pem");

        let mut get_device_id_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "get", "device-id"])?;

        get_device_id_cmd
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "The provided config key: device-id is not set",
            ));

        let mut get_cert_path_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "get", "device-cert-path"])?;

        get_cert_path_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(&cert_path));

        let mut get_key_path_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "get", "device-key-path"])?;

        get_key_path_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(&key_path));

        let mut get_c8y_url_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "get", "c8y-url"])?;

        get_c8y_url_cmd
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "The provided config key: c8y-url is not set",
            ));

        let mut get_c8y_root_cert_path_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "get", "c8y-root-cert-path"])?;

        get_c8y_root_cert_path_cmd
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "The provided config key: c8y-root-cert-path is not set",
            ));

        Ok(())
    }

    #[test]
    fn run_config_list_default() {
        let key_regex = r#"device-key-path=[\w/]*certificate/tedge-private-key.pem"#;
        let cert_regex = r#"device-cert-path=[\w/]*certificate/tedge-certificate.pem"#;

        let key_predicate_fn = predicate::str::is_match(key_regex).unwrap();
        let cert_predicate_fn = predicate::str::is_match(cert_regex).unwrap();

        let mut list_cmd = tedge_command(&["config", "list"]).unwrap();
        let assert = list_cmd.assert().success();
        let output = assert.get_output().clone();
        let str = String::from_utf8(output.clone().stdout).unwrap();

        assert_eq!(true, key_predicate_fn.eval(&str));
        assert_eq!(true, cert_predicate_fn.eval(&str));
    }

    #[test]
    fn run_config_list_all() {
        let key_regex = r#"device-key-path=[\w/]*certificate/tedge-private-key.pem"#;
        let cert_regex = r#"device-cert-path=[\w/]*certificate/tedge-certificate.pem"#;

        let key_predicate_fn = predicate::str::is_match(key_regex).unwrap();
        let cert_predicate_fn = predicate::str::is_match(cert_regex).unwrap();

        let mut list_cmd = tedge_command(&["config", "list", "--all"]).unwrap();
        let assert = list_cmd.assert().success();
        let output = assert.get_output().clone();
        let str = String::from_utf8(output.clone().stdout).unwrap();

        assert_eq!(true, key_predicate_fn.eval(&str));
        assert_eq!(true, cert_predicate_fn.eval(&str));
        for key in get_tedge_config_keys() {
            assert_eq!(true, str.contains(key));
        }
    }

    #[test]
    fn run_config_list_doc() {
        let mut list_cmd = tedge_command(&["config", "list", "--doc"]).unwrap();
        let assert = list_cmd.assert().success();
        let output = assert.get_output().clone();
        let str = String::from_utf8(output.clone().stdout).unwrap();

        for key in get_tedge_config_keys() {
            assert_eq!(true, str.contains(key));
        }
        assert_eq!(true, str.contains("Example"));
    }

    fn tedge_command_with_test_home<I, S>(
        test_home: &str,
        args: I,
    ) -> Result<assert_cmd::Command, Box<dyn std::error::Error>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let mut command = tedge_command(args)?;
        command.env("HOME", test_home);
        Ok(command)
    }

    fn temp_path(dir: &tempfile::TempDir, filename: &str) -> String {
        String::from(dir.path().join(filename).to_str().unwrap())
    }

    fn get_tedge_config_keys() -> Vec<&'static str> {
        let vec = vec![
            "device-id",
            "device-key-path",
            "device-cert-path",
            "c8y-url",
            "c8y-root-cert-path",
            "c8y-connect",
        ];
        return vec;
    }
}
