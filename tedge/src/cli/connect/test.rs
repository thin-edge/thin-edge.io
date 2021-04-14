use crate::cli::connect::*;
use tedge_config::*;
use tempfile::NamedTempFile;

#[test]
fn test_bridge_config_validate_ok() -> anyhow::Result<()> {
    let ca_file = NamedTempFile::new().unwrap();
    let bridge_ca_path: FilePath = ca_file.path().into();

    let cert_file = NamedTempFile::new().unwrap();
    let bridge_certfile: FilePath = cert_file.path().into();

    let key_file = NamedTempFile::new().unwrap();
    let bridge_keyfile: FilePath = key_file.path().into();

    let correct_url = "http://test.com";

    let config = BridgeConfig {
        address: correct_url.into(),
        bridge_root_cert_path: bridge_ca_path,
        bridge_certfile,
        bridge_keyfile,
        ..default_bridge_config()
    };

    assert!(config.validate().is_ok());

    Ok(())
}

// XXX: This test is flawed.
#[test]
fn test_bridge_config_validate_wrong_url() {
    let incorrect_url = "noturl";
    let non_existent_path = "/path/that/does/not/exist";

    let config = BridgeConfig {
        address: incorrect_url.into(),
        bridge_certfile: non_existent_path.into(),
        bridge_keyfile: non_existent_path.into(),
        ..default_bridge_config()
    };

    assert!(config.validate().is_err());
}

#[test]
fn config_bridge_validate_wrong_cert_path() {
    let correct_url = "http://test.com";
    let non_existent_path = "/path/that/does/not/exist";

    let config = BridgeConfig {
        address: correct_url.into(),
        bridge_certfile: non_existent_path.into(),
        bridge_keyfile: non_existent_path.into(),
        ..default_bridge_config()
    };

    assert!(config.validate().is_err());
}

#[test]
fn config_bridge_validate_wrong_key_path() {
    let cert_file = NamedTempFile::new().unwrap();
    let bridge_certfile: FilePath = cert_file.path().into();
    let correct_url = "http://test.com";
    let non_existent_path = "/path/that/does/not/exist";

    let config = BridgeConfig {
        address: correct_url.into(),
        bridge_certfile,
        bridge_keyfile: non_existent_path.into(),
        ..default_bridge_config()
    };

    assert!(config.validate().is_err());
}

fn default_bridge_config() -> BridgeConfig {
    BridgeConfig {
        common_mosquitto_config: CommonMosquittoConfig::default(),
        cloud_name: "az/c8y".into(),
        config_file: "cfg".to_string(),
        connection: "edge_to_az/c8y".into(),
        address: "".into(),
        remote_username: None,
        bridge_root_cert_path: "".into(),
        bridge_certfile: "".into(),
        bridge_keyfile: "".into(),
        remote_clientid: "".into(),
        local_clientid: "".into(),
        use_mapper: true,
        try_private: false,
        start_type: "automatic".into(),
        clean_session: true,
        notifications: false,
        bridge_attempt_unsubscribe: false,
        topics: vec![],
    }
}
