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

mod env_var_persistence {
    use super::*;

    #[test]
    fn get_applies_env_but_set_and_unset_do_not_persist_it() {
        let env = TestEnv::new().env("TEDGE_MQTT_PORT", "9999");
        env.write_root_toml("[mqtt]\nport = 1883\n");

        env.cmd()
            .args(["get", "mqtt.port"])
            .assert()
            .success()
            .stdout("9999\n");
        env.cmd()
            .args(["set", "device.id", "temporary"])
            .assert()
            .success();
        env.cmd().args(["unset", "device.id"]).assert().success();

        let root: toml::Table = std::fs::read_to_string(env.config_dir().join("tedge.toml"))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(root["mqtt"]["port"].as_integer(), Some(1883));
        assert!(root["device"].get("id").is_none());

        env.cmd()
            .args(["get", "mqtt.port"])
            .assert()
            .success()
            .stdout("9999\n");
    }

    #[test]
    fn empty_env_override_does_not_delete_file_value_during_set() {
        let env = TestEnv::new().env("TEDGE_DEVICE_TYPE", "");
        env.write_root_toml("[device]\ntype = \"from-file\"\n");

        env.cmd()
            .args(["set", "mqtt.port", "8883"])
            .assert()
            .success();

        let root: toml::Table = std::fs::read_to_string(env.config_dir().join("tedge.toml"))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(root["device"]["type"].as_str(), Some("from-file"));
        assert_eq!(root["mqtt"]["port"].as_integer(), Some(8883));
    }

    #[test]
    fn mapper_add_and_remove_use_the_persisted_list() {
        let env = TestEnv::new().env("TEDGE_C8Y_SMARTREST_TEMPLATES", "env-a,env-b");
        env.write_mapper_toml("c8y", "[smartrest]\ntemplates = [\"file-a\", \"file-b\"]\n");

        env.cmd()
            .args(["add", "mappers.c8y.smartrest.templates", "file-c"])
            .assert()
            .success();
        env.cmd()
            .args(["remove", "mappers.c8y.smartrest.templates", "file-a"])
            .assert()
            .success();

        let mapper = env.read_mapper_toml("c8y");
        let templates: Vec<_> = mapper["smartrest"]["templates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        assert_eq!(templates, ["file-b", "file-c"]);

        env.cmd()
            .args(["get", "mappers.c8y.smartrest.templates"])
            .assert()
            .success()
            .stdout("env-a,env-b\n");
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
    fn all_mapper_defaults_can_be_read() {
        let env = TestEnv::new();
        env.write_root_toml("");
        env.write_mapper_toml("tb", "[device]");
        let mgr = mapper_config::config_manager(env.dir.path());
        let entries = mgr.key_entries::<mapper_config::MapperConfigDto>();
        for entry in entries.iter().map(|e| e.key.as_str()) {
            let assert = env
                .cmd()
                .args(["get", &format!("mappers.tb.{entry}")])
                .assert();

            dbg!(&entry, &assert);
            match assert.try_success() {
                Ok(_) => continue,
                Err(assert) => {
                    assert.assert().failure().stderr(format!(
                        "Error: The value for 'mappers.tb.{entry}' is not set.\n"
                    ));
                }
            }
        }
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

mod cloud_type_conversion {
    use super::*;

    #[test]
    fn custom_to_c8y_preserves_shared_values_in_raw_toml() {
        let env = TestEnv::new();
        env.write_mapper_toml(
            "mycloud",
            r#"
            url = "custom.example.com"

            [device]
            cert_path = "/custom/cert.pem"
            key_path = "/custom/key.pem"
            "#,
        );
        env.cmd()
            .args(["set", "mappers.mycloud.cloud_type", "c8y"])
            .assert()
            .success();

        let table = env.read_mapper_toml("mycloud");
        assert_eq!(table["cloud_type"].as_str(), Some("c8y"));
        // Any write through the schema canonicalizes url to host:port form
        assert_eq!(table["url"].as_str(), Some("custom.example.com:443"));
        assert_eq!(
            table["device"]["cert_path"].as_str(),
            Some("/custom/cert.pem")
        );
        assert_eq!(
            table["device"]["key_path"].as_str(),
            Some("/custom/key.pem")
        );
    }

    #[test]
    fn custom_to_c8y_exposes_c8y_schema() {
        let env = TestEnv::new();
        env.write_mapper_toml("mycloud", "url = \"custom.example.com\"\n");
        env.cmd()
            .args(["set", "mappers.mycloud.cloud_type", "c8y"])
            .assert()
            .success();

        env.cmd()
            .args(["get", "mappers.mycloud.smartrest.use_operation_id"])
            .assert()
            .success()
            .stdout("true\n");
        env.cmd()
            .args(["get", "mappers.mycloud.url"])
            .assert()
            .success()
            .stdout("custom.example.com:443\n");
    }

    #[test]
    fn c8y_to_custom_preserves_shared_values_in_raw_toml() {
        let env = TestEnv::new();
        env.write_mapper_toml(
            "mycloud",
            r#"
            cloud_type = "c8y"
            url = "tenant.example.com"

            [device]
            cert_path = "/c8y/cert.pem"
            key_path = "/c8y/key.pem"

            [smartrest]
            templates = ["t1", "t2"]
            "#,
        );
        env.cmd()
            .args(["set", "mappers.mycloud.cloud_type", "custom"])
            .assert()
            .success();

        let table = env.read_mapper_toml("mycloud");
        assert_eq!(table["cloud_type"].as_str(), Some("custom"));
        assert_eq!(table["url"].as_str(), Some("tenant.example.com:443"));
        assert_eq!(table["device"]["cert_path"].as_str(), Some("/c8y/cert.pem"));
        assert_eq!(table["device"]["key_path"].as_str(), Some("/c8y/key.pem"));
    }

    #[test]
    fn c8y_to_custom_deletes_keys_outside_the_new_schema() {
        let env = TestEnv::new();
        env.write_mapper_toml(
            "mycloud",
            r#"
            cloud_type = "c8y"
            url = "tenant.example.com"

            [smartrest]
            templates = ["t1"]
            use_operation_id = false

            [proxy.bind]
            port = 9999

            [availability]
            interval = 60
            "#,
        );
        env.cmd()
            .args(["set", "mappers.mycloud.cloud_type", "custom"])
            .assert()
            .success();

        let table = env.read_mapper_toml("mycloud");
        assert!(
            !table.contains_key("smartrest"),
            "smartrest should be deleted, got: {table}"
        );
        assert!(
            !table.contains_key("proxy"),
            "proxy should be deleted, got: {table}"
        );
        assert!(
            !table.contains_key("availability"),
            "availability should be deleted, got: {table}"
        );
    }

    #[test]
    fn c8y_to_custom_hides_c8y_keys_from_cli() {
        let env = TestEnv::new();
        env.write_mapper_toml(
            "mycloud",
            r#"
            cloud_type = "c8y"

            [smartrest]
            use_operation_id = false
            "#,
        );
        env.cmd()
            .args(["set", "mappers.mycloud.cloud_type", "custom"])
            .assert()
            .success();

        env.cmd()
            .args(["get", "mappers.mycloud.smartrest.use_operation_id"])
            .assert()
            .failure();
    }

    #[test]
    fn builtin_c8y_mapper_converted_to_custom_uses_custom_schema() {
        let env = TestEnv::new();
        env.write_mapper_toml(
            "c8y",
            r#"
            url = "tenant.example.com"

            [smartrest]
            templates = ["t1"]
            "#,
        );
        env.cmd()
            .args(["set", "mappers.c8y.cloud_type", "custom"])
            .assert()
            .success();

        let table = env.read_mapper_toml("c8y");
        assert_eq!(table["cloud_type"].as_str(), Some("custom"));
        assert!(
            !table.contains_key("smartrest"),
            "smartrest should be deleted, got: {table}"
        );
        env.cmd()
            .args(["get", "mappers.c8y.smartrest.use_operation_id"])
            .assert()
            .failure();
        env.cmd()
            .args(["get", "mappers.c8y.url"])
            .assert()
            .success()
            .stdout("tenant.example.com:443\n");
    }

    #[test]
    fn unset_cloud_type_reverts_non_builtin_mapper_to_custom_schema() {
        let env = TestEnv::new();
        env.write_mapper_toml(
            "mycloud",
            r#"
            cloud_type = "c8y"
            url = "tenant.example.com"

            [smartrest]
            templates = ["t1"]
            "#,
        );
        env.cmd()
            .args(["unset", "mappers.mycloud.cloud_type"])
            .assert()
            .success();

        let table = env.read_mapper_toml("mycloud");
        assert!(!table.contains_key("cloud_type"));
        assert!(
            !table.contains_key("smartrest"),
            "smartrest should be deleted, got: {table}"
        );
        assert_eq!(table["url"].as_str(), Some("tenant.example.com:443"));
        env.cmd()
            .args(["get", "mappers.mycloud.cloud_type"])
            .assert()
            .success()
            .stdout("custom\n");
    }

    #[test]
    fn unset_cloud_type_on_builtin_name_keeps_c8y_schema() {
        let env = TestEnv::new();
        env.write_mapper_toml(
            "c8y",
            r#"
            cloud_type = "c8y"

            [smartrest]
            templates = ["t1"]
            "#,
        );
        env.cmd()
            .args(["unset", "mappers.c8y.cloud_type"])
            .assert()
            .success();

        let table = env.read_mapper_toml("c8y");
        assert!(!table.contains_key("cloud_type"));
        assert!(
            table.contains_key("smartrest"),
            "smartrest should survive, got: {table}"
        );
        env.cmd()
            .args(["get", "mappers.c8y.smartrest.templates"])
            .assert()
            .success()
            .stdout("t1\n");
    }

    #[test]
    fn unrecognised_cloud_type_falls_back_to_custom_schema() {
        let env = TestEnv::new();
        env.write_mapper_toml(
            "mycloud",
            r#"
            cloud_type = "c8y"
            url = "tenant.example.com"

            [smartrest]
            templates = ["t1"]
            "#,
        );
        env.cmd()
            .args(["set", "mappers.mycloud.cloud_type", "az"])
            .assert()
            .success();

        let table = env.read_mapper_toml("mycloud");
        assert_eq!(table["cloud_type"].as_str(), Some("az"));
        assert_eq!(table["url"].as_str(), Some("tenant.example.com:443"));
        assert!(
            !table.contains_key("smartrest"),
            "smartrest should be deleted, got: {table}"
        );
    }

    #[test]
    fn converting_away_and_back_loses_c8y_specific_values() {
        let env = TestEnv::new();
        env.write_mapper_toml(
            "mycloud",
            r#"
            cloud_type = "c8y"
            url = "tenant.example.com"

            [smartrest]
            templates = ["t1", "t2"]
            "#,
        );
        env.cmd()
            .args(["set", "mappers.mycloud.cloud_type", "custom"])
            .assert()
            .success();
        env.cmd()
            .args(["set", "mappers.mycloud.cloud_type", "c8y"])
            .assert()
            .success();

        let table = env.read_mapper_toml("mycloud");
        assert_eq!(table["cloud_type"].as_str(), Some("c8y"));
        assert_eq!(table["url"].as_str(), Some("tenant.example.com:443"));
        // Documents the clobber hazard motivating a future guard on cloud_type changes
        assert!(
            !table.contains_key("smartrest"),
            "c8y-specific settings do not survive a round trip, got: {table}"
        );
        env.cmd()
            .args(["get", "mappers.mycloud.smartrest.templates"])
            .assert()
            .success()
            .stdout("\n");
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

mod device_id_from_certificate {
    use super::*;

    const CERT_CN_DEVICE_UNDER_TEST: &str = "-----BEGIN CERTIFICATE-----
MIIBjDCCATOgAwIBAgIUeA8Gov/Qu/raEF5ttAE+NSERsHAwCgYIKoZIzj0EAwIw
HDEaMBgGA1UEAwwRZGV2aWNlLXVuZGVyLXRlc3QwHhcNMjYwNzA3MTQyNDIyWhcN
MzYwNzA0MTQyNDIyWjAcMRowGAYDVQQDDBFkZXZpY2UtdW5kZXItdGVzdDBZMBMG
ByqGSM49AgEGCCqGSM49AwEHA0IABNpkLrT4jun6Lcd5XvXpkd529zKlAuEG9zyJ
hDM6QTCTH38/vvTZ8o3rFhms4CwiQZU8pBq5d8vnDdhYDjAa4omjUzBRMB0GA1Ud
DgQWBBR4BM4tMqXLEaw+K9+Cuu+/ZgP38DAfBgNVHSMEGDAWgBR4BM4tMqXLEaw+
K9+Cuu+/ZgP38DAPBgNVHRMBAf8EBTADAQH/MAoGCCqGSM49BAMCA0cAMEQCIFuT
2xfkDVMUKSrTY/34TUZz0DcXKZ/xh++BjwvH80lcAiA03/aYoiuk5QPaAtzZA+7s
1pkrKODGEA7ma2HoeqqdRA==
-----END CERTIFICATE-----
";

    const CERT_CN_ALTERNATIVE_DEVICE: &str = "-----BEGIN CERTIFICATE-----
MIIBjzCCATWgAwIBAgIUFfvw5yHm3pbxNEnwGJX+y419brEwCgYIKoZIzj0EAwIw
HTEbMBkGA1UEAwwSYWx0ZXJuYXRpdmUtZGV2aWNlMB4XDTI2MDcwNzE0MjQyMloX
DTM2MDcwNDE0MjQyMlowHTEbMBkGA1UEAwwSYWx0ZXJuYXRpdmUtZGV2aWNlMFkw
EwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEbQmsgw72WqAtse6SqirvdX1ZjLwiVAJ0
bRvOiqHnvrp/fzk+mA+n1g3WQEGypbpqeO1lj9FsOpS7ZAvJVqZA56NTMFEwHQYD
VR0OBBYEFKNxvU4CUfLmgtR+lmM4mXr+L0keMB8GA1UdIwQYMBaAFKNxvU4CUfLm
gtR+lmM4mXr+L0keMA8GA1UdEwEB/wQFMAMBAf8wCgYIKoZIzj0EAwIDSAAwRQIh
ALHolJYqVzgoAFCYNjTFXwH45/pBV/6X2Zm32tTLShKLAiAyhaDGhhhZP8fnCJEh
4TI7a2S8iniQoyC5dju8ga4bpA==
-----END CERTIFICATE-----
";

    #[test]
    fn device_id_defaults_to_certificate_common_name() {
        let env = TestEnv::new();
        write_default_certificate(&env, CERT_CN_DEVICE_UNDER_TEST);
        env.cmd()
            .args(["get", "device.id"])
            .assert()
            .success()
            .stdout("device-under-test\n");
    }

    #[test]
    fn set_device_id_overrides_certificate_common_name() {
        let env = TestEnv::new();
        write_default_certificate(&env, CERT_CN_DEVICE_UNDER_TEST);
        env.cmd()
            .args(["set", "device.id", "explicit-id"])
            .assert()
            .success();
        env.cmd()
            .args(["get", "device.id"])
            .assert()
            .success()
            .stdout("explicit-id\n");
    }

    #[test]
    fn device_id_follows_configured_cert_path() {
        let env = TestEnv::new();
        write_default_certificate(&env, CERT_CN_DEVICE_UNDER_TEST);
        let other_cert = env.config_dir().join("other-cert.pem");
        std::fs::write(&other_cert, CERT_CN_ALTERNATIVE_DEVICE).unwrap();
        env.cmd()
            .args(["set", "device.cert_path", other_cert.to_str().unwrap()])
            .assert()
            .success();
        env.cmd()
            .args(["get", "device.id"])
            .assert()
            .success()
            .stdout("alternative-device\n");
    }

    #[test]
    fn device_id_is_unset_when_certificate_is_missing() {
        TestEnv::new()
            .cmd()
            .args(["get", "device.id"])
            .assert()
            .failure();
    }

    #[test]
    fn invalid_certificate_reports_key_source_and_reason() {
        let env = TestEnv::new();
        write_default_certificate(&env, "not a certificate");
        env.cmd()
            .args(["get", "device.id"])
            .assert()
            .failure()
            .stderr(
                predicates::str::contains("Failed to derive a value for 'device.id'")
                    .and(predicates::str::contains("device.cert_path"))
                    .and(predicates::str::contains("not a PEM certificate")),
            );
    }

    #[test]
    fn mapper_device_id_defaults_to_mapper_certificate_common_name() {
        let env = TestEnv::new();
        env.write_mapper_toml("c8y", "");
        let cert = env.config_dir().join("c8y-cert.pem");
        std::fs::write(&cert, CERT_CN_ALTERNATIVE_DEVICE).unwrap();
        env.cmd()
            .args([
                "set",
                "mappers.c8y.device.cert_path",
                cert.to_str().unwrap(),
            ])
            .assert()
            .success();
        env.cmd()
            .args(["get", "mappers.c8y.device.id"])
            .assert()
            .success()
            .stdout("alternative-device\n");
    }

    #[test]
    fn mapper_device_id_falls_back_to_root_certificate() {
        let env = TestEnv::new();
        env.write_mapper_toml("c8y", "");
        write_default_certificate(&env, CERT_CN_DEVICE_UNDER_TEST);
        env.cmd()
            .args(["get", "mappers.c8y.device.id"])
            .assert()
            .success()
            .stdout("device-under-test\n");
    }

    fn write_default_certificate(env: &TestEnv, contents: &str) {
        let certs_dir = env.config_dir().join("device-certs");
        std::fs::create_dir_all(&certs_dir).unwrap();
        std::fs::write(certs_dir.join("tedge-certificate.pem"), contents).unwrap();
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

    fn read_mapper_toml(&self, name: &str) -> toml::Table {
        let path = self
            .dir
            .path()
            .join("mappers")
            .join(name)
            .join("mapper.toml");
        std::fs::read_to_string(&path).unwrap().parse().unwrap()
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
