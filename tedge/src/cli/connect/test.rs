use super::*;

const CORRECT_URL: &str = "http://test.com";
const INCORRECT_URL: &str = "noturl";
const INCORRECT_PATH: &str = "/path";

fn default_bridge_config() -> BridgeConfig {
    BridgeConfig {
        cloud_name: "az/c8y".into(),
        config_file: "cfg".to_string(),
        connection: "edge_to_az/c8y".into(),
        address: "".into(),
        remote_username: "".into(),
        bridge_cafile: "".into(),
        bridge_certfile: "".into(),
        bridge_keyfile: "".into(),
        remote_clientid: "".into(),
        local_clientid: "".into(),
        try_private: false,
        start_type: "automatic".into(),
        cleansession: true,
        bridge_insecure: false,
        notifications: false,
        bridge_attempt_unsubscribe: false,
        topics: vec![],
    }
}

#[test]
fn config_bridge_validate_ok() {
    let ca_file = NamedTempFile::new().unwrap();
    let bridge_cafile = ca_file.path().to_str().unwrap().to_owned();

    let cert_file = NamedTempFile::new().unwrap();
    let bridge_certfile = cert_file.path().to_str().unwrap().to_owned();

    let key_file = NamedTempFile::new().unwrap();
    let bridge_keyfile = key_file.path().to_str().unwrap().to_owned();

    let config = BridgeConfig {
        address: CORRECT_URL.into(),
        bridge_cafile,
        bridge_certfile,
        bridge_keyfile,
        ..default_bridge_config()
    };
    assert!(config.validate().is_ok());
}

#[test]
fn config_bridge_validate_wrong_url() {
    let config = BridgeConfig {
        address: INCORRECT_URL.into(),
        bridge_certfile: INCORRECT_PATH.into(),
        bridge_keyfile: INCORRECT_PATH.into(),
        ..default_bridge_config()
    };

    assert!(config.validate().is_err());
}

#[test]
fn config_bridge_validate_wrong_cert_path() {
    let config = BridgeConfig {
        address: CORRECT_URL.into(),
        bridge_certfile: INCORRECT_PATH.into(),
        bridge_keyfile: INCORRECT_PATH.into(),
        ..default_bridge_config()
    };

    assert!(config.validate().is_err());
}

#[test]
fn config_bridge_validate_wrong_key_path() {
    let cert_file = NamedTempFile::new().unwrap();
    let bridge_certfile = cert_file.path().to_str().unwrap().to_owned();

    let config = BridgeConfig {
        address: CORRECT_URL.into(),
        bridge_certfile,
        bridge_keyfile: INCORRECT_PATH.into(),
        ..default_bridge_config()
    };

    assert!(config.validate().is_err());
}

use std::io::Write;
#[test]
fn bridge_config_c8y_create() {
    let toml_config = r#"
            [device]
            id = "alpha"
            cert_path = "./test-certificate.pem"
            key_path = "./test-private-key.pem"

            [c8y]
            url = "test.test.io:8883"
            root_cert_path = "./test_root.pem"
            connect = "true"
            "#;

    let config_file = temp_file_with_content(toml_config);
    let config = TEdgeConfig::from_custom_config(config_file.path()).unwrap();
    let bridge = C8y::c8y_bridge_config(config).unwrap();

    let expected = BridgeConfig {
        cloud_name: "c8y".into(),
        config_file: "c8y-bridge.conf".into(),
        connection: "edge_to_c8y".into(),
        address: "test.test.io:8883".into(),
        remote_username: "".into(),
        bridge_cafile: "./test_root.pem".into(),
        remote_clientid: "alpha".into(),
        local_clientid: "Cumulocity".into(),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
        try_private: false,
        start_type: "automatic".into(),
        cleansession: true,
        bridge_insecure: false,
        notifications: false,
        bridge_attempt_unsubscribe: false,
        topics: vec![
            // Registration
            r#"s/dcr in 2 c8y/ """#.into(),
            r#"s/ucr out 2 c8y/ """#.into(),
            // Templates
            r#"s/dt in 2 c8y/ """#.into(),
            r#"s/ut/# out 2 c8y/ """#.into(),
            // Static templates
            r#"s/us out 2 c8y/ """#.into(),
            r#"t/us out 2 c8y/ """#.into(),
            r#"q/us out 2 c8y/ """#.into(),
            r#"c/us out 2 c8y/ """#.into(),
            r#"s/ds in 2 c8y/ """#.into(),
            r#"s/os in 2 c8y/ """#.into(),
            // Debug
            r#"s/e in 0 c8y/ """#.into(),
            // SmartRest2
            r#"s/uc/# out 2 c8y/ """#.into(),
            r#"t/uc/# out 2 c8y/ """#.into(),
            r#"q/uc/# out 2 c8y/ """#.into(),
            r#"c/uc/# out 2 c8y/ """#.into(),
            r#"s/dc/# in 2 c8y/ """#.into(),
            r#"s/oc/# in 2 c8y/ """#.into(),
            // c8y JSON
            r#"measurement/measurements/create out 2 c8y/ """#.into(),
            r#"error in 2 c8y/ """#.into(),
        ],
    };

    assert_eq!(bridge, expected);
}

#[test]
fn bridge_config_azure_create() {
    let toml_config = r#"
            [device]
            id = "alpha"
            cert_path = "./test-certificate.pem"
            key_path = "./test-private-key.pem"

            [azure]
            url = "test.test.io:8883"
            root_cert_path = "./test_root.pem"
            connect = "true"
            "#;

    let config_file = temp_file_with_content(toml_config);
    let config = TEdgeConfig::from_custom_config(config_file.path()).unwrap();
    let bridge = Azure::azure_bridge_config(config).unwrap();

    let expected = BridgeConfig {
        cloud_name: "az".into(),
        config_file: "az-bridge.conf".to_string(),
        connection: "edge_to_az".into(),
        address: "test.test.io:8883".into(),
        remote_username: "test.test.io/alpha/?api-version=2018-06-30".into(),
        bridge_cafile: "./test_root.pem".into(),
        remote_clientid: "alpha".into(),
        local_clientid: "Azure".into(),
        bridge_certfile: "./test-certificate.pem".into(),
        bridge_keyfile: "./test-private-key.pem".into(),
        try_private: false,
        start_type: "automatic".into(),
        cleansession: true,
        bridge_insecure: false,
        notifications: false,
        bridge_attempt_unsubscribe: false,
        topics: vec![
            r#"messages/events/ out 1 az/ devices/alpha/"#.into(),
            r##"messages/devicebound/# out 1 az/ devices/alpha/"##.into(),
            r##"twin/res/# in 1 az/ $iothub/"##.into(),
            r#"twin/GET/?$rid=1 out 1 az/ $iothub/"#.into(),
        ],
    };
    assert_eq!(bridge, expected);
}

fn temp_file_with_content(content: &str) -> NamedTempFile {
    let file = NamedTempFile::new().unwrap();
    file.as_file().write_all(content.as_bytes()).unwrap();
    file
}
