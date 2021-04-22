mod os_related;

mod tests {
    use predicates::prelude::*;
    use std::path::Path; // Used for writing assertions

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
        let home_dir = tempdir.path().to_str().unwrap();

        let mut get_device_id_cmd =
            tedge_command_with_test_home(home_dir, &["config", "get", "device.id"])?;
        let mut set_cert_path_cmd = tedge_command_with_test_home(
            home_dir,
            &["config", "set", "device.cert.path", &cert_path],
        )?;
        let mut set_key_path_cmd = tedge_command_with_test_home(
            home_dir,
            &["config", "set", "device.key.path", &key_path],
        )?;

        let mut create_cmd =
            tedge_command_with_test_home(home_dir, &["cert", "create", "--device-id", device_id])?;
        let mut show_cmd = tedge_command_with_test_home(home_dir, &["cert", "show"])?;
        let mut remove_cmd = tedge_command_with_test_home(home_dir, &["cert", "remove"])?;

        // Configure tedge to use specific paths for the private key and the certificate
        set_cert_path_cmd.assert().success();
        set_key_path_cmd.assert().success();

        // The remove command can be run when there is no certificate
        remove_cmd.assert().success();

        // We start we no certificate, hence no device id
        get_device_id_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "The provided config key: 'device.id' is not set",
            ));

        // The create command created a certificate
        create_cmd.assert().success();

        // The certificate use the device id as CN
        show_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(format!("CN={},", device_id)));

        // The create command updated the config with the device.id
        get_device_id_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(device_id));

        // When a certificate exists, it is not over-written by the create command
        create_cmd
            .assert()
            .failure()
            .stderr(predicate::str::contains("A certificate already exists"));

        // The remove command removed the certificate
        remove_cmd.assert().success();

        // which can no more be displayed
        show_cmd
            .assert()
            .failure()
            .stderr(predicate::str::contains("Missing file"));

        // The remove command also removed the device id from the config
        get_device_id_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "The provided config key: 'device.id' is not set",
            ));

        // The a new certificate can then be created.
        create_cmd.assert().success();

        Ok(())
    }

    #[test]
    fn run_config_set_get_unset_read_only_key() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_dir_path = temp_dir.path();
        let test_home_str = temp_dir_path.to_str().unwrap();

        let device_id = "test";

        // allowed to get read-only key by CLI
        let mut get_device_id_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "get", "device.id"])?;

        get_device_id_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "The provided config key: 'device.id' is not set",
            ));

        // forbidden to set read-only key by CLI
        let mut set_device_id_cmd = tedge_command_with_test_home(
            test_home_str,
            &["config", "set", "device.id", device_id],
        )?;

        set_device_id_cmd.assert().failure();

        // forbidden to unset read-only key by CLI
        let mut unset_device_id_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "unset", "device.id"])?;

        unset_device_id_cmd.assert().failure();

        Ok(())
    }

    #[test]
    fn run_config_set_get_unset_read_write_key() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_dir_path = temp_dir.path();
        let test_home_str = temp_dir_path.to_str().unwrap();

        let c8y_url = "mytenant.cumulocity.com";

        let mut get_c8y_url_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "get", "c8y.url"])?;

        get_c8y_url_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "The provided config key: 'c8y.url' is not set",
            ));

        let mut set_c8y_url_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "set", "c8y.url", c8y_url])?;

        set_c8y_url_cmd.assert().success();

        get_c8y_url_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(c8y_url));

        let mut unset_c8y_url_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "unset", "c8y.url"])?;

        unset_c8y_url_cmd.assert().success();

        get_c8y_url_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "The provided config key: 'c8y.url' is not set",
            ));

        Ok(())
    }

    #[test]
    fn run_config_defaults() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_dir_path = temp_dir.path();
        let test_home_str = temp_dir_path.to_str().unwrap();

        let cert_path = temp_path(
            &temp_dir,
            &join_paths(".tedge", "device-certs/tedge-certificate.pem"),
        );
        let key_path = temp_path(
            &temp_dir,
            &join_paths(".tedge", "device-certs/tedge-private-key.pem"),
        );

        let mut get_device_id_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "get", "device.id"])?;

        get_device_id_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "The provided config key: 'device.id' is not set",
            ));

        let mut get_cert_path_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "get", "device.cert.path"])?;

        get_cert_path_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(&cert_path));

        let mut get_key_path_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "get", "device.key.path"])?;

        get_key_path_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(&key_path));

        let mut get_c8y_url_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "get", "c8y.url"])?;

        get_c8y_url_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "The provided config key: 'c8y.url' is not set",
            ));

        let mut get_c8y_root_cert_path_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "get", "c8y.root.cert.path"])?;

        get_c8y_root_cert_path_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains("/etc/ssl/cert"));

        Ok(())
    }

    #[test]
    fn run_config_list_default() {
        let temp_dir = tempfile::tempdir().unwrap();
        let test_home_str = temp_dir.path().to_str().unwrap();

        let mut list_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "list"]).unwrap();
        let assert = list_cmd.assert().success();
        let output = assert.get_output().clone();
        let output_str = String::from_utf8(output.stdout).unwrap();

        let key_path = extract_config_value(&output_str, "device.key.path");
        assert!(key_path.ends_with("tedge-private-key.pem"));
        assert!(key_path.contains(".tedge"));

        let cert_path = extract_config_value(&output_str, "device.cert.path");
        assert!(cert_path.ends_with("tedge-certificate.pem"));
        assert!(cert_path.contains(".tedge"));
    }

    fn extract_config_value(output: &String, key: &str) -> String {
        output
            .lines()
            .map(|line| line.splitn(2, "=").collect::<Vec<_>>())
            .find(|pair| pair[0] == key)
            .unwrap()[1]
            .into()
    }

    #[test]
    fn tedge_disconnect_c8y_no_bridge_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_dir_path = temp_dir.path();
        let test_home_str = temp_dir_path.to_str().unwrap();

        // If file doesn't exist exit code will be 0.
        tedge_command_with_test_home(test_home_str, &["disconnect", "c8y"])
            .unwrap()
            .assert()
            .success();
    }

    #[test]
    fn run_config_list_all() {
        let temp_dir = tempfile::tempdir().unwrap();
        let test_home_str = temp_dir.path().to_str().unwrap();

        let mut list_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "list", "--all"]).unwrap();
        let assert = list_cmd.assert().success();
        let output = assert.get_output();
        let output_str = String::from_utf8(output.clone().stdout).unwrap();

        let key_path = extract_config_value(&output_str, "device.key.path");
        assert!(key_path.ends_with("tedge-private-key.pem"));
        assert!(key_path.contains(".tedge"));

        let cert_path = extract_config_value(&output_str, "device.cert.path");
        assert!(cert_path.ends_with("tedge-certificate.pem"));
        assert!(cert_path.contains(".tedge"));

        for key in get_tedge_config_keys() {
            assert_eq!(true, output_str.contains(key));
        }
    }

    #[test]
    fn run_config_list_doc() {
        let temp_dir = tempfile::tempdir().unwrap();
        let test_home_str = temp_dir.path().to_str().unwrap();

        let mut list_cmd =
            tedge_command_with_test_home(test_home_str, &["config", "list", "--doc"]).unwrap();
        let assert = list_cmd.assert().success();
        let output = assert.get_output().clone();
        let output_str = String::from_utf8(output.clone().stdout).unwrap();

        for key in get_tedge_config_keys() {
            assert_eq!(true, output_str.contains(key));
        }
        assert_eq!(true, output_str.contains("Example"));
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

    fn join_paths(prefix_path: &str, trailer_path: &str) -> String {
        Path::new(prefix_path)
            .join(trailer_path)
            .to_str()
            .unwrap()
            .into()
    }

    fn temp_path(dir: &tempfile::TempDir, filename: &str) -> String {
        String::from(dir.path().join(filename).to_str().unwrap())
    }

    fn get_tedge_config_keys() -> Vec<&'static str> {
        let vec = vec![
            "device.id",
            "device.key.path",
            "device.cert.path",
            "c8y.url",
            "c8y.root.cert.path",
        ];
        return vec;
    }
}
