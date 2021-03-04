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
