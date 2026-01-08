#[cfg(test)]
mod tests {
    use predicates::prelude::*;
    use test_case::test_case;

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
        let mut cmd = tedge_command(["--help"])?;

        cmd.assert()
            .success()
            .stdout(predicate::str::contains("Usage"));

        Ok(())
    }

    #[test]
    fn run_version() -> Result<(), Box<dyn std::error::Error>> {
        let mut cmd = tedge_command(["-V"])?;

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
            tedge_command_with_test_home(["--config-dir", home_dir, "config", "get", "device.id"])?;
        let mut set_cert_path_cmd = tedge_command_with_test_home([
            "--config-dir",
            home_dir,
            "config",
            "set",
            "device.cert_path",
            &cert_path,
        ])?;
        let mut set_key_path_cmd = tedge_command_with_test_home([
            "--config-dir",
            home_dir,
            "config",
            "set",
            "device.key_path",
            &key_path,
        ])?;

        let mut create_cmd = tedge_command_with_test_home([
            "--config-dir",
            home_dir,
            "cert",
            "create",
            "--device-id",
            device_id,
        ])?;
        let mut show_cmd =
            tedge_command_with_test_home(["--config-dir", home_dir, "cert", "show"])?;
        let mut remove_cmd =
            tedge_command_with_test_home(["--config-dir", home_dir, "cert", "remove"])?;

        // Configure tedge to use specific paths for the private key and the certificate
        set_cert_path_cmd.assert().success();
        set_key_path_cmd.assert().success();

        // The remove command can be run when there is no certificate
        remove_cmd.assert().success();

        // We start with no certificate, hence no device id
        get_device_id_cmd
            .assert()
            .failure()
            .stderr(predicate::str::contains("'device.id' is not configured"));

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
            .stderr(predicate::str::contains("No such file"));

        // The remove command also removed the device id from the config
        get_device_id_cmd
            .assert()
            .failure()
            .stderr(predicate::str::contains("device.id"));

        // A new certificate can then be created.
        create_cmd.assert().success();

        Ok(())
    }

    // #[test_case(config key, config value, expected unset value)]
    #[test_case(
        "c8y.url",
        "mytenant.cumulocity.com",
        "The provided config key: \'c8y.url\' is not set\n",
        false
    )]
    #[test_case("mqtt.bind.port", "8880", "1883", true)]
    fn run_config_set_get_unset_read_write_key(
        config_key: &str,
        config_value: &str,
        default_value_or_error_message: &str,
        is_default: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_dir_path = temp_dir.path();
        let test_home_str = temp_dir_path.to_str().unwrap();

        let mut get_config_command = tedge_command_with_test_home([
            "--config-dir",
            test_home_str,
            "config",
            "get",
            config_key,
        ])?;

        let get_config_command = get_config_command.assert();

        if is_default {
            get_config_command
                .stdout(predicate::str::contains(default_value_or_error_message))
                .success();
        } else {
            get_config_command
                .stderr(predicate::str::contains(default_value_or_error_message))
                .failure();
        }

        let mut set_config_command = tedge_command_with_test_home([
            "--config-dir",
            test_home_str,
            "config",
            "set",
            config_key,
            config_value,
        ])?;

        set_config_command.assert().success();

        let mut get_config_command = tedge_command_with_test_home([
            "--config-dir",
            test_home_str,
            "config",
            "get",
            config_key,
        ])?;

        get_config_command
            .assert()
            .success()
            .stdout(predicate::str::contains(config_value));

        let mut unset_config_command = tedge_command_with_test_home([
            "--config-dir",
            test_home_str,
            "config",
            "unset",
            config_key,
        ])?;

        unset_config_command.assert().success();

        let mut get_config_command = tedge_command_with_test_home([
            "--config-dir",
            test_home_str,
            "config",
            "get",
            config_key,
        ])?;

        let get_config_command = get_config_command.assert();

        if is_default {
            get_config_command
                .stdout(predicate::str::contains(default_value_or_error_message))
                .success();
        } else {
            get_config_command
                .stderr(predicate::str::contains(default_value_or_error_message))
                .failure();
        }

        Ok(())
    }

    #[test]
    fn run_config_defaults() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_dir_path = temp_dir.path();
        let test_home_str = temp_dir_path.to_str().unwrap();

        let cert_path = temp_path(&temp_dir, "device-certs/tedge-certificate.pem");
        let key_path = temp_path(&temp_dir, "device-certs/tedge-private-key.pem");

        let mut get_device_id_cmd = tedge_command_with_test_home([
            "--config-dir",
            test_home_str,
            "config",
            "get",
            "device.id",
        ])?;

        get_device_id_cmd
            .assert()
            .failure()
            .stderr(predicate::str::contains("device.id"));

        let mut get_cert_path_cmd = tedge_command_with_test_home([
            "--config-dir",
            test_home_str,
            "config",
            "get",
            "device.cert_path",
        ])?;

        get_cert_path_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(cert_path));

        let mut get_key_path_cmd = tedge_command_with_test_home([
            "--config-dir",
            test_home_str,
            "config",
            "get",
            "device.key_path",
        ])?;

        get_key_path_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains(key_path));

        let mut get_c8y_url_cmd = tedge_command_with_test_home([
            "--config-dir",
            test_home_str,
            "config",
            "get",
            "c8y.url",
        ])?;

        get_c8y_url_cmd
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "The provided config key: 'c8y.url' is not set",
            ));

        let mut get_c8y_root_cert_path_cmd = tedge_command_with_test_home([
            "--config-dir",
            test_home_str,
            "config",
            "get",
            "c8y.root_cert_path",
        ])?;

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
            tedge_command_with_test_home(["--config-dir", test_home_str, "config", "list"])
                .unwrap();
        let assert = list_cmd.assert().success();
        let output = assert.get_output().clone();
        let output_str = String::from_utf8(output.stdout).unwrap();

        let key_path = extract_config_value(&output_str, "device.key_path");
        assert!(key_path.ends_with("tedge-private-key.pem"));
        assert!(key_path.contains(test_home_str));

        let cert_path = extract_config_value(&output_str, "device.cert_path");
        assert!(cert_path.ends_with("tedge-certificate.pem"));
        assert!(cert_path.contains(test_home_str));
    }

    fn extract_config_value<'a>(output: &'a str, key: &str) -> &'a str {
        output
            .lines()
            .map(|line| line.splitn(2, '=').collect::<Vec<_>>())
            .find(|pair| pair[0] == key)
            .unwrap_or_else(|| panic!("couldn't find config value for '{key}'"))[1]
    }

    #[test]
    fn tedge_disconnect_c8y_no_bridge_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let temp_dir_path = temp_dir.path();
        let test_home_str = temp_dir_path.to_str().unwrap();

        // If file doesn't exist exit code will be 0.
        tedge_command_with_test_home(["--config-dir", test_home_str, "disconnect", "c8y"])
            .unwrap()
            .assert()
            .success();
    }

    #[test]
    fn run_config_list_all() {
        let temp_dir = tempfile::tempdir().unwrap();
        let test_home_str = temp_dir.path().to_str().unwrap();

        let mut list_cmd =
            tedge_command(["--config-dir", test_home_str, "config", "list", "--all"]).unwrap();

        let assert = list_cmd.assert().success();
        let output = assert.get_output();
        let output_str = String::from_utf8(output.clone().stdout).unwrap();

        let key_path = extract_config_value(&output_str, "device.key_path");
        assert!(key_path.ends_with("tedge-private-key.pem"));
        assert!(key_path.contains(test_home_str));

        let cert_path = extract_config_value(&output_str, "device.cert_path");
        assert!(cert_path.ends_with("tedge-certificate.pem"));
        assert!(cert_path.contains(test_home_str));

        for key in get_tedge_config_keys() {
            assert!(
                output_str.contains(key),
                "couldn't find '{key}' in output of tedge config list --all"
            );
        }
    }

    #[test]
    fn run_config_list_doc() {
        let temp_dir = tempfile::tempdir().unwrap();
        let test_home_str = temp_dir.path().to_str().unwrap();

        let mut list_cmd = tedge_command_with_test_home([
            "--config-dir",
            test_home_str,
            "config",
            "list",
            "--doc",
        ])
        .unwrap();
        let assert = list_cmd.assert().success();
        let output = assert.get_output().clone();
        let output_str = String::from_utf8(output.stdout).unwrap();

        for key in get_tedge_config_keys() {
            assert!(
                output_str.contains(key),
                "couldn't find '{key}' in output of tedge config list --doc"
            );
        }
        assert!(output_str.contains("Example"));
    }

    #[test_case("set", "c8y.url", "new.example.com", "c8y"; "set c8y")]
    #[test_case("unset", "c8y.url", "", "c8y"; "unset c8y")]
    #[test_case("add", "c8y.smartrest.templates", "template1", "c8y"; "add c8y")]
    #[test_case("set", "az.url", "new.example.com", "az"; "set az")]
    #[test_case("unset", "az.url", "", "az"; "unset az")]
    #[test_case("set", "aws.url", "new.example.com", "aws"; "set aws")]
    #[test_case("unset", "aws.url", "", "aws"; "unset aws")]
    fn legacy_config_allows_mutation_commands(
        cmd: &str,
        config_key: &str,
        value: &str,
        cloud: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;

        // Setup legacy config for the specific cloud being tested
        setup_legacy_config(&temp_dir, cloud);

        let test_home = temp_dir.path().to_str().unwrap();

        // Attempt the command
        let mut args = vec!["--config-dir", test_home, "config", cmd, config_key];
        if !value.is_empty() {
            args.push(value);
        }

        let mut command = tedge_command_with_test_home(args)?;

        // Verify it succeeds
        command.assert().success();

        Ok(())
    }

    #[test]
    fn non_cloud_configs_unaffected_by_migration() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;

        // Setup: All three clouds migrated
        setup_migrated_default(&temp_dir, "c8y");
        setup_migrated_default(&temp_dir, "az");
        setup_migrated_default(&temp_dir, "aws");

        let test_home = temp_dir.path().to_str().unwrap();

        // Try to set a non-cloud config (mqtt.port)
        let mut set_mqtt_cmd = tedge_command_with_test_home([
            "--config-dir",
            test_home,
            "config",
            "set",
            "mqtt.bind.port",
            "1234",
        ])?;

        // Should succeed despite all clouds being migrated
        set_mqtt_cmd.assert().success();

        // Verify the value was set
        let mut get_mqtt_cmd = tedge_command_with_test_home([
            "--config-dir",
            test_home,
            "config",
            "get",
            "mqtt.bind.port",
        ])?;

        get_mqtt_cmd
            .assert()
            .success()
            .stdout(predicate::str::contains("1234"));

        Ok(())
    }

    #[test_case::test_case("aws")]
    #[test_case::test_case("c8y")]
    #[test_case::test_case("az")]
    fn migrated_configs_are_written_to_by_tedge_config_set(
        cloud: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;

        setup_migrated_default(&temp_dir, cloud);
        let test_home = temp_dir.path().to_str().unwrap();

        let mut set_config_cmd = tedge_command_with_test_home([
            "--config-dir",
            test_home,
            "config",
            "set",
            &format!("{cloud}.url"),
            "new.example.com",
        ])?;
        set_config_cmd.assert().success();

        let migrated_toml =
            std::fs::read_to_string(temp_dir.path().join(format!("mappers/{cloud}/tedge.toml")))?;
        assert_eq!(migrated_toml.trim(), "url = \"new.example.com\"");

        // tedge.toml will be created when `tedge config set` runs
        let tedge_toml = std::fs::read_to_string(temp_dir.path().join("tedge.toml"))?;
        assert_eq!(tedge_toml.trim(), "");
        Ok(())
    }

    #[test_case::test_case("aws")]
    #[test_case::test_case("c8y")]
    #[test_case::test_case("az")]
    fn migrated_profiled_configs_can_be_created_by_tedge_config_set(
        cloud: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;

        setup_migrated_default(&temp_dir, cloud);
        let test_home = temp_dir.path().to_str().unwrap();

        let mut set_config_command = tedge_command_with_test_home([
            "--config-dir",
            test_home,
            "config",
            "set",
            &format!("{cloud}.url"),
            "new.example.com",
            "--profile",
            "profile",
        ])?;
        set_config_command.assert().success();

        let migrated_toml = std::fs::read_to_string(
            temp_dir
                .path()
                .join(format!("mappers/{cloud}.profile/tedge.toml")),
        )
        .unwrap();
        assert_eq!(migrated_toml.trim(), "url = \"new.example.com\"");

        // tedge.toml will be created when `tedge config set` runs
        let tedge_toml = std::fs::read_to_string(temp_dir.path().join("tedge.toml"))?;
        assert_eq!(tedge_toml.trim(), "");
        Ok(())
    }

    #[test_case::test_case("aws")]
    #[test_case::test_case("c8y")]
    #[test_case::test_case("az")]
    fn migrated_profiled_configs_can_be_updated_by_tedge_config_set(
        cloud: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = tempfile::tempdir()?;

        setup_migrated_profile(&temp_dir, cloud, "profile");
        let test_home = temp_dir.path().to_str().unwrap();

        let mut set_config_command = tedge_command_with_test_home([
            "--config-dir",
            test_home,
            "config",
            "set",
            &format!("{cloud}.url"),
            "new.example.com",
            "--profile",
            "profile",
        ])?;
        set_config_command.assert().success();

        let migrated_toml = std::fs::read_to_string(
            temp_dir
                .path()
                .join(format!("mappers/{cloud}.profile/tedge.toml")),
        )
        .unwrap();
        assert_eq!(migrated_toml.trim(), "url = \"new.example.com\"");

        // tedge.toml will be created when `tedge config set` runs
        let tedge_toml = std::fs::read_to_string(temp_dir.path().join("tedge.toml"))?;
        assert_eq!(tedge_toml.trim(), "");
        Ok(())
    }

    // Helper functions for restrict_cloud_config_update tests
    fn setup_legacy_config(temp_dir: &tempfile::TempDir, cloud: &str) {
        let tedge_toml = temp_dir.path().join("tedge.toml");
        let content = format!("[{cloud}]\nurl = \"example.com\"\n");
        std::fs::write(tedge_toml, content).unwrap();
    }

    fn setup_migrated_default(temp_dir: &tempfile::TempDir, cloud: &str) {
        let cloud_dir = temp_dir.path().join("mappers").join(cloud);
        std::fs::create_dir_all(&cloud_dir).unwrap();
        let config_path = cloud_dir.join("tedge.toml");
        std::fs::write(config_path, "url = \"example.com\"\n").unwrap();
    }

    fn setup_migrated_profile(temp_dir: &tempfile::TempDir, cloud: &str, profile: &str) {
        let profile_dir = temp_dir
            .path()
            .join("mappers")
            .join(format!("{cloud}.{profile}"));
        std::fs::create_dir_all(&profile_dir).unwrap();
        let config_path = profile_dir.join("tedge.toml");
        std::fs::write(config_path, "url = \"example.com\"\n").unwrap();
    }

    fn tedge_command_with_test_home<I, S>(
        args: I,
    ) -> Result<assert_cmd::Command, Box<dyn std::error::Error>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let command = tedge_command(args)?;
        Ok(command)
    }

    fn temp_path(dir: &tempfile::TempDir, filename: &str) -> String {
        String::from(dir.path().join(filename).to_str().unwrap())
    }

    fn get_tedge_config_keys() -> Vec<&'static str> {
        let vec = vec![
            "device.id",
            "device.key_path",
            "device.cert_path",
            "c8y.url",
            "c8y.root_cert_path",
        ];
        vec
    }
}
