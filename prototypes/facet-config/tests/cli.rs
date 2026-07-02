use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use std::path::Path;
use tempfile::TempDir;

mod cli_basics {
    use super::*;

    #[test]
    fn read_returns_default_for_device_type() {
        TestEnv::new()
            .cmd()
            .args(["get", "device.type"])
            .assert()
            .success()
            .stdout("thin-edge.io\n");
    }

    #[test]
    fn read_returns_default_for_mqtt_port() {
        TestEnv::new()
            .cmd()
            .args(["get", "mqtt.port"])
            .assert()
            .success()
            .stdout("1883\n");
    }

    #[test]
    fn read_unset_key_without_default_exits_nonzero() {
        TestEnv::new()
            .cmd()
            .args(["get", "device.id"])
            .assert()
            .failure();
    }

    #[test]
    fn set_then_get_round_trips_a_value() {
        let env = TestEnv::new();
        env.cmd()
            .args(["set", "device.id", "my-device"])
            .assert()
            .success();
        env.cmd()
            .args(["get", "device.id"])
            .assert()
            .success()
            .stdout("my-device\n");
    }

    #[test]
    fn set_then_read_returns_set_value_over_default() {
        let env = TestEnv::new();
        env.cmd()
            .args(["set", "device.type", "custom-type"])
            .assert()
            .success();
        env.cmd()
            .args(["get", "device.type"])
            .assert()
            .success()
            .stdout("custom-type\n");
    }

    #[test]
    fn unset_clears_previously_set_value() {
        let env = TestEnv::new();
        env.cmd()
            .args(["set", "mqtt.port", "9999"])
            .assert()
            .success();
        env.cmd().args(["unset", "mqtt.port"]).assert().success();
        env.cmd()
            .args(["get", "mqtt.port"])
            .assert()
            .success()
            .stdout("1883\n");
    }

    #[test]
    fn set_invalid_value_exits_nonzero() {
        TestEnv::new()
            .cmd()
            .args(["set", "mqtt.port", "not-a-number"])
            .assert()
            .failure();
    }

    #[test]
    fn unknown_key_exits_nonzero() {
        TestEnv::new()
            .cmd()
            .args(["get", "nonexistent.key"])
            .assert()
            .failure();
    }
}

mod raw_toml_input {
    use super::*;

    #[test]
    fn toml_root_mqtt_port_is_read() {
        let env = TestEnv::new();
        env.write_root_toml("[mqtt]\nport = 9999\n");
        env.cmd()
            .args(["get", "mqtt.port"])
            .assert()
            .success()
            .stdout("9999\n");
    }

    #[test]
    fn toml_root_device_type_overrides_default() {
        let env = TestEnv::new();
        env.write_root_toml("[device]\ntype = \"custom\"\n");
        env.cmd()
            .args(["get", "device.type"])
            .assert()
            .success()
            .stdout("custom\n");
    }

    #[test]
    fn toml_c8y_mapper_url_is_read() {
        let env = TestEnv::new();
        env.write_mapper_toml("c8y", "url = \"tenant.example.com\"\n");
        env.cmd()
            .args(["get", "mappers.c8y.url"])
            .assert()
            .success()
            .stdout("tenant.example.com:443\n");
    }

    #[test]
    fn toml_c8y_mapper_nested_proxy_round_trips() {
        let env = TestEnv::new();
        env.write_mapper_toml("c8y", "[proxy.bind]\nport = 7777\naddress = \"0.0.0.0\"\n");
        env.cmd()
            .args(["get", "mappers.c8y.proxy.bind.port"])
            .assert()
            .success()
            .stdout("7777\n");
        env.cmd()
            .args(["get", "mappers.c8y.proxy.bind.address"])
            .assert()
            .success()
            .stdout("0.0.0.0\n");
    }

    #[test]
    fn toml_custom_mapper_url_is_read() {
        let env = TestEnv::new();
        env.write_mapper_toml("custom", "url = \"custom.example.com\"\n");
        env.cmd()
            .args(["get", "mappers.custom.url"])
            .assert()
            .success()
            .stdout("custom.example.com:443\n");
    }
}

mod cloud_alias_resolution {
    use super::*;

    #[test]
    fn read_c8y_url_resolves_to_mappers_c8y_url() {
        let env = TestEnv::new();
        env.write_mapper_toml("c8y", "url = \"tenant.example.com\"\n");
        env.cmd()
            .args(["get", "c8y.url"])
            .assert()
            .success()
            .stdout("tenant.example.com:443\n");
    }

    #[test]
    fn set_c8y_url_writes_to_mapper_file() {
        let env = TestEnv::new();
        env.write_mapper_toml("c8y", "");
        env.cmd()
            .args(["set", "c8y.url", "tenant.com"])
            .assert()
            .success();

        let mapper_content =
            std::fs::read_to_string(env.config_dir().join("mappers/c8y/mapper.toml")).unwrap();
        assert!(
            mapper_content.contains("tenant.com"),
            "mapper.toml should contain the URL, got: {mapper_content}"
        );

        let root_content = std::fs::read_to_string(env.config_dir().join("tedge.toml")).unwrap();
        assert!(
            !root_content.contains("tenant.com"),
            "tedge.toml should not contain the URL"
        );
    }

    #[test]
    fn set_c8y_nested_key() {
        let env = TestEnv::new();
        env.write_mapper_toml("c8y", "");
        env.cmd()
            .args(["set", "c8y.proxy.bind.port", "9999"])
            .assert()
            .success();
        env.cmd()
            .args(["get", "mappers.c8y.proxy.bind.port"])
            .assert()
            .success()
            .stdout("9999\n");
    }
}

mod profile_support {
    use super::*;

    #[test]
    fn set_and_read_c8y_profiled_url() {
        let env = TestEnv::new().with_profile("staging");
        env.write_mapper_toml("c8y.staging", "");
        env.cmd()
            .args(["set", "c8y.url", "staging.com"])
            .assert()
            .success();
        env.cmd()
            .args(["get", "c8y.url"])
            .assert()
            .success()
            .stdout("staging.com:443\n");
    }

    #[test]
    fn profiled_mapper_not_visible_without_profile_flag() {
        let env = TestEnv::new();
        env.write_mapper_toml("c8y", "");
        env.write_mapper_toml("c8y.staging", "url = \"staging.com\"\n");
        env.cmd()
            .args(["get", "mappers.c8y.url"])
            .assert()
            .failure();
    }
}

mod env_var_overrides {
    use super::*;

    #[test]
    fn mappers_c8y_url_overrides_mapper_value() {
        let env = TestEnv::new().env("TEDGE_MAPPERS_C8Y_URL", "env.example.com");
        env.write_mapper_toml("c8y", "");
        env.cmd()
            .args(["get", "mappers.c8y.url"])
            .assert()
            .success()
            .stdout("env.example.com:443\n");
    }

    #[test]
    fn cloud_c8y_url_overrides_mapper_value() {
        let env = TestEnv::new().env("TEDGE_C8Y_URL", "cloud.example.com");
        env.write_mapper_toml("c8y", "");
        env.cmd()
            .args(["get", "mappers.c8y.url"])
            .assert()
            .success()
            .stdout("cloud.example.com:443\n");
    }

    #[test]
    fn c8y_profiles_new_url_with_profile() {
        let env = TestEnv::new()
            .with_profile("new")
            .env("TEDGE_C8Y_PROFILES_NEW_URL", "new.example.com");
        env.cmd()
            .args(["get", "c8y.url"])
            .assert()
            .success()
            .stdout("new.example.com:443\n");
    }

    #[test]
    fn c8y_proxy_bind_port() {
        let env = TestEnv::new().env("TEDGE_C8Y_PROXY_BIND_PORT", "1234");
        env.write_mapper_toml("c8y", "");
        env.cmd()
            .args(["get", "mappers.c8y.proxy.bind.port"])
            .assert()
            .success()
            .stdout("1234\n");
    }

    #[test]
    fn mappers_c8y_proxy_bind_port() {
        let env = TestEnv::new().env("TEDGE_MAPPERS_C8Y_PROXY_BIND_PORT", "5678");
        env.write_mapper_toml("c8y", "");
        env.cmd()
            .args(["get", "mappers.c8y.proxy.bind.port"])
            .assert()
            .success()
            .stdout("5678\n");
    }

    #[test]
    fn cloud_form_takes_precedence_over_mappers_form() {
        let env = TestEnv::new()
            .env("TEDGE_MAPPERS_C8Y_URL", "mappers-form.example.com")
            .env("TEDGE_C8Y_URL", "cloud-form.example.com");
        env.write_mapper_toml("c8y", "");
        env.cmd()
            .args(["get", "mappers.c8y.url"])
            .assert()
            .success()
            .stdout("cloud-form.example.com:443\n");
    }

    #[test]
    fn mappers_custom_url() {
        let env = TestEnv::new().env("TEDGE_MAPPERS_CUSTOM_URL", "custom.example.com");
        env.write_mapper_toml("custom", "");
        env.cmd()
            .args(["get", "mappers.custom.url"])
            .assert()
            .success()
            .stdout("custom.example.com:443\n");
    }

    #[test]
    fn custom_proxy_bind_port_is_unrecognised() {
        let env = TestEnv::new().env("TEDGE_MAPPERS_CUSTOM_PROXY_BIND_PORT", "9999");
        env.write_mapper_toml("custom", "");
        env.cmd()
            .args(["get", "mappers.custom.proxy.bind.port"])
            .assert()
            .failure();
    }

    #[test]
    fn cloud_form_does_not_apply_to_custom_mapper() {
        let env = TestEnv::new().env("TEDGE_CUSTOM_URL", "should-not-apply.example.com");
        env.write_mapper_toml("custom", "");
        env.cmd()
            .args(["get", "mappers.custom.url"])
            .assert()
            .failure();
    }

    #[test]
    fn tedge_device_key_path() {
        let env = TestEnv::new().env("TEDGE_DEVICE_KEY_PATH", "/custom/key.pem");
        env.cmd()
            .args(["get", "device.key_path"])
            .assert()
            .success()
            .stdout("/custom/key.pem\n");
    }

    #[test]
    fn tedge_mqtt_port() {
        let env = TestEnv::new().env("TEDGE_MQTT_PORT", "9999");
        env.cmd()
            .args(["get", "mqtt.port"])
            .assert()
            .success()
            .stdout("9999\n");
    }

    #[test]
    fn unprofiled_c8y_url_does_not_affect_profiled_mapper() {
        let env = TestEnv::new()
            .with_profile("staging")
            .env("TEDGE_C8Y_URL", "base.example.com");
        env.write_mapper_toml("c8y.staging", "");
        env.cmd().args(["get", "c8y.url"]).assert().failure();
    }

    #[test]
    fn profiled_staging_url_does_not_affect_production() {
        let env = TestEnv::new()
            .with_profile("production")
            .env("TEDGE_C8Y_PROFILES_STAGING_URL", "staging.example.com");
        env.cmd().args(["get", "c8y.url"]).assert().failure();
    }
}

mod cross_config_defaults {
    use super::*;

    #[test]
    fn c8y_inherits_root_cert_path() {
        let env = TestEnv::new();
        env.write_root_toml(
            "[device]\ncert_path = \"/root/cert.pem\"\nkey_path = \"/root/key.pem\"\n",
        );
        env.write_mapper_toml("c8y", "");
        env.cmd()
            .args(["get", "mappers.c8y.device.cert_path"])
            .assert()
            .success()
            .stdout("/root/cert.pem\n");
    }

    #[test]
    fn c8y_explicit_overrides_root() {
        let env = TestEnv::new();
        env.write_root_toml(
            "[device]\ncert_path = \"/root/cert.pem\"\nkey_path = \"/root/key.pem\"\n",
        );
        env.write_mapper_toml("c8y", "[device]\ncert_path = \"/c8y/cert.pem\"\n");
        env.cmd()
            .args(["get", "mappers.c8y.device.cert_path"])
            .assert()
            .success()
            .stdout("/c8y/cert.pem\n");
    }

    #[test]
    fn env_device_cert_path_propagates_to_c8y() {
        let env = TestEnv::new().env("TEDGE_DEVICE_CERT_PATH", "/env/cert.pem");
        env.write_mapper_toml("c8y", "");
        env.cmd()
            .args(["get", "mappers.c8y.device.cert_path"])
            .assert()
            .success()
            .stdout("/env/cert.pem\n");
    }

    #[test]
    fn custom_inherits_root_cert_path() {
        let env = TestEnv::new();
        env.write_root_toml(
            "[device]\ncert_path = \"/root/cert.pem\"\nkey_path = \"/root/key.pem\"\n",
        );
        env.write_mapper_toml("custom", "");
        env.cmd()
            .args(["get", "mappers.custom.device.cert_path"])
            .assert()
            .success()
            .stdout("/root/cert.pem\n");
    }

    #[test]
    fn custom_explicit_overrides_root() {
        let env = TestEnv::new();
        env.write_root_toml(
            "[device]\ncert_path = \"/root/cert.pem\"\nkey_path = \"/root/key.pem\"\n",
        );
        env.write_mapper_toml("custom", "[device]\ncert_path = \"/custom/cert.pem\"\n");
        env.cmd()
            .args(["get", "mappers.custom.device.cert_path"])
            .assert()
            .success()
            .stdout("/custom/cert.pem\n");
    }

    #[test]
    fn env_device_cert_path_propagates_to_custom() {
        let env = TestEnv::new().env("TEDGE_DEVICE_CERT_PATH", "/env/cert.pem");
        env.write_mapper_toml("custom", "");
        env.cmd()
            .args(["get", "mappers.custom.device.cert_path"])
            .assert()
            .success()
            .stdout("/env/cert.pem\n");
    }

    #[test]
    fn c8y_keeps_own_cert_while_custom_inherits_root() {
        let env = TestEnv::new();
        env.write_root_toml(
            "[device]\ncert_path = \"/root/cert.pem\"\nkey_path = \"/root/key.pem\"\n",
        );
        env.write_mapper_toml("c8y", "[device]\ncert_path = \"/c8y/cert.pem\"\n");
        env.write_mapper_toml("custom", "");

        env.cmd()
            .args(["get", "mappers.c8y.device.cert_path"])
            .assert()
            .success()
            .stdout("/c8y/cert.pem\n");
        env.cmd()
            .args(["get", "mappers.custom.device.cert_path"])
            .assert()
            .success()
            .stdout("/root/cert.pem\n");
    }
}

mod list_show_commands {
    use super::*;

    #[test]
    fn list_includes_root_and_mapper_keys() {
        let env = TestEnv::new();
        env.write_mapper_toml("c8y", "");
        let output = env
            .cmd()
            .arg("list")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let stdout = String::from_utf8(output).unwrap();

        assert!(
            stdout.contains("device.id="),
            "should contain root key device.id"
        );
        assert!(
            stdout.contains("mqtt.port="),
            "should contain root key mqtt.port"
        );
        assert!(
            stdout.contains("mappers.c8y.url="),
            "should contain mapper key mappers.c8y.url"
        );
        assert!(
            stdout.contains("mappers.c8y.proxy.bind.port="),
            "should contain mapper key mappers.c8y.proxy.bind.port"
        );
    }

    #[test]
    fn show_displays_all_keys_with_values() {
        let env = TestEnv::new();
        env.write_mapper_toml("c8y", "url = \"tenant.example.com\"\n");
        let output = env
            .cmd()
            .arg("show")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        let stdout = String::from_utf8(output).unwrap();

        assert!(
            stdout.contains("device.type: thin-edge.io"),
            "should show default device.type"
        );
        assert!(
            stdout.contains("mappers.c8y.url: tenant.example.com:443"),
            "should show set mapper URL"
        );
    }
}

mod add_remove {
    use super::*;

    #[test]
    fn add_then_read_templates_set() {
        let env = TestEnv::new();
        env.write_mapper_toml("c8y", "");
        env.cmd()
            .args(["add", "mappers.c8y.smartrest.templates", "t1"])
            .assert()
            .success();
        env.cmd()
            .args(["get", "mappers.c8y.smartrest.templates"])
            .assert()
            .success()
            .stdout("t1\n");
    }

    #[test]
    fn add_twice_accumulates() {
        let env = TestEnv::new();
        env.write_mapper_toml("c8y", "");
        env.cmd()
            .args(["add", "mappers.c8y.smartrest.templates", "t1"])
            .assert()
            .success();
        env.cmd()
            .args(["add", "mappers.c8y.smartrest.templates", "t2"])
            .assert()
            .success();
        env.cmd()
            .args(["get", "mappers.c8y.smartrest.templates"])
            .assert()
            .success()
            .stdout("t1,t2\n");
    }

    #[test]
    fn remove_deletes_matching_value() {
        let env = TestEnv::new();
        env.write_mapper_toml("c8y", "");
        env.cmd()
            .args(["add", "mappers.c8y.smartrest.templates", "a"])
            .assert()
            .success();
        env.cmd()
            .args(["add", "mappers.c8y.smartrest.templates", "b"])
            .assert()
            .success();
        env.cmd()
            .args(["add", "mappers.c8y.smartrest.templates", "c"])
            .assert()
            .success();
        env.cmd()
            .args(["remove", "mappers.c8y.smartrest.templates", "b"])
            .assert()
            .success();
        env.cmd()
            .args(["get", "mappers.c8y.smartrest.templates"])
            .assert()
            .success()
            .stdout("a,c\n");
    }
}

mod env_var_warning_suppression {
    use super::*;

    #[test]
    fn mapper_env_var_does_not_warn_on_root_config() {
        let env = TestEnv::new().env("TEDGE_MAPPERS_TB_DEVICE_KEY_PATH", "/custom/key.pem");
        env.cmd()
            .args(["get", "device.type"])
            .assert()
            .success()
            .stdout("thin-edge.io\n")
            .stderr(predicates::str::contains("Warning").not());
    }

    #[test]
    fn cloud_env_var_does_not_warn_on_root_config() {
        let env = TestEnv::new().env("TEDGE_C8Y_URL", "example.cumulocity.com");
        env.cmd()
            .args(["get", "device.type"])
            .assert()
            .success()
            .stdout("thin-edge.io\n")
            .stderr(predicates::str::contains("Warning").not());
    }

    #[test]
    fn genuinely_unknown_env_var_still_warns() {
        let env = TestEnv::new().env("TEDGE_NONEXISTENT_FIELD", "something");
        env.cmd()
            .args(["get", "device.type"])
            .assert()
            .success()
            .stderr(predicates::str::contains("Unknown configuration field"));
    }
}

mod cloud_type_dispatch {
    use super::*;

    #[test]
    fn c8y_exposes_c8y_specific_keys() {
        let env = TestEnv::new();
        env.write_mapper_toml("mycloud", "cloud_type = \"c8y\"\n");
        env.cmd()
            .args(["get", "mappers.mycloud.smartrest.use_operation_id"])
            .assert()
            .success()
            .stdout("true\n");
    }

    #[test]
    fn c8y_on_custom_named_mapper_allows_set() {
        let env = TestEnv::new();
        env.write_mapper_toml("mycloud", "cloud_type = \"c8y\"\n");
        env.cmd()
            .args(["set", "mappers.mycloud.smartrest.use_operation_id", "false"])
            .assert()
            .success();
        env.cmd()
            .args(["get", "mappers.mycloud.smartrest.use_operation_id"])
            .assert()
            .success()
            .stdout("false\n");
    }

    #[test]
    fn custom_does_not_have_c8y_keys() {
        let env = TestEnv::new();
        env.write_mapper_toml("foo", "cloud_type = \"custom\"\n");
        env.cmd()
            .args(["get", "mappers.foo.smartrest.use_operation_id"])
            .assert()
            .failure();
    }

    #[test]
    fn mapper_without_cloud_type_defaults_to_custom() {
        let env = TestEnv::new();
        env.write_mapper_toml("foo", "");
        env.cmd()
            .args(["get", "mappers.foo.smartrest.use_operation_id"])
            .assert()
            .failure();
    }

    #[test]
    fn builtin_name_c8y_without_explicit_cloud_type_uses_c8y() {
        let env = TestEnv::new();
        env.write_mapper_toml("c8y", "");
        env.cmd()
            .args(["get", "mappers.c8y.smartrest.use_operation_id"])
            .assert()
            .success()
            .stdout("true\n");
    }

    #[test]
    fn c8y_receives_cloud_alias_env_vars() {
        let env = TestEnv::new().env("TEDGE_C8Y_URL", "alias.example.com:8443");
        env.write_mapper_toml("c8y", "cloud_type = \"c8y\"\n");
        env.cmd()
            .args(["get", "mappers.c8y.url"])
            .assert()
            .success()
            .stdout("alias.example.com:8443\n");
    }

    #[test]
    fn visible_in_list_output() {
        let env = TestEnv::new();
        env.write_mapper_toml("mycloud", "cloud_type = \"c8y\"\n");
        env.cmd()
            .args(["list"])
            .assert()
            .success()
            .stdout(predicates::str::contains("mappers.mycloud.cloud_type=c8y"));
    }

    #[test]
    fn non_builtin_name_gets_mapper_specific_env() {
        let env = TestEnv::new().env("TEDGE_MAPPERS_MYCLOUD_SMARTREST_USE_OPERATION_ID", "false");
        env.write_mapper_toml("mycloud", "cloud_type = \"c8y\"\n");
        env.cmd()
            .args(["get", "mappers.mycloud.smartrest.use_operation_id"])
            .assert()
            .success()
            .stdout("false\n");
    }
}

mod error_messages {
    use super::*;

    #[test]
    fn unknown_mapper_mentions_directory_and_known_mappers() {
        let env = TestEnv::new();
        env.write_mapper_toml("c8y", "");
        let output = env
            .cmd()
            .args(["get", "mappers.nosuch.url"])
            .assert()
            .failure();
        let stderr = String::from_utf8_lossy(&output.get_output().stderr);
        assert!(
            stderr.contains("Unknown mapper 'nosuch'"),
            "expected unknown mapper error, got: {stderr}"
        );
        assert!(
            stderr.contains("mappers/nosuch"),
            "expected directory hint, got: {stderr}"
        );
        assert!(
            stderr.contains("c8y"),
            "expected known mappers to include c8y, got: {stderr}"
        );
    }

    #[test]
    fn unset_cloud_key_says_not_set() {
        let env = TestEnv::new();
        env.cmd()
            .args(["get", "c8y.url"])
            .assert()
            .failure()
            .stderr(predicates::str::contains("is not set"));
    }

    #[test]
    fn not_set_error_includes_profile_name() {
        let env = TestEnv::new().with_profile("staging");
        env.cmd()
            .args(["get", "c8y.url"])
            .assert()
            .failure()
            .stderr(
                predicates::str::contains("is not set")
                    .and(predicates::str::contains("profile 'staging'")),
            );
    }
}

mod env_var_suppression {
    use super::*;

    #[test]
    fn tedge_config_dir_does_not_warn() {
        let env = TestEnv::new().env("TEDGE_CONFIG_DIR", "/some/path");
        env.cmd()
            .args(["get", "device.type"])
            .assert()
            .success()
            .stderr(predicates::str::contains("Warning").not());
    }

    #[test]
    fn tedge_cloud_profile_does_not_warn() {
        let env = TestEnv::new().env("TEDGE_CLOUD_PROFILE", "staging");
        env.cmd()
            .args(["get", "device.type"])
            .assert()
            .success()
            .stderr(predicates::str::contains("Warning").not());
    }
}

struct TestEnv {
    dir: TempDir,
    envs: Vec<(String, String)>,
    profile: Option<String>,
}

impl TestEnv {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("tedge.toml"), "").unwrap();
        Self {
            dir,
            envs: Vec::new(),
            profile: None,
        }
    }

    fn with_profile(mut self, profile: &str) -> Self {
        self.profile = Some(profile.to_owned());
        self
    }

    fn env(mut self, key: &str, value: &str) -> Self {
        self.envs.push((key.to_owned(), value.to_owned()));
        self
    }

    fn write_root_toml(&self, content: &str) {
        std::fs::write(self.dir.path().join("tedge.toml"), content).unwrap();
    }

    fn write_mapper_toml(&self, name: &str, content: &str) {
        let mapper_dir = self.dir.path().join("mappers").join(name);
        std::fs::create_dir_all(&mapper_dir).unwrap();
        std::fs::write(mapper_dir.join("mapper.toml"), content).unwrap();
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("facet-config-prototype").unwrap();
        cmd.arg("--config-dir").arg(self.dir.path());
        cmd.env_clear();
        cmd.env("HOME", "/tmp");
        if let Some(profile) = &self.profile {
            cmd.arg("--profile").arg(profile);
        }
        for (k, v) in &self.envs {
            cmd.env(k, v);
        }
        cmd
    }

    fn config_dir(&self) -> &Path {
        self.dir.path()
    }
}
