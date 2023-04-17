// It feels weird to verify the default value using `assert!(config.az_mapper_timestamp())`
// so prevent clippy shouting at us when we do `assert_eq!(config.az_mapper_timestamp(), true)`
// instead
#![allow(clippy::bool_assert_comparison)]
use camino::Utf8PathBuf;
use std::convert::TryFrom;
use std::io::Write;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use tedge_config::ConfigurationUpdate::*;
use tedge_config::*;
use tedge_test_utils::fs::TempTedgeDir;

const DEFAULT_ROOT_CERT_PATH: &str = "/etc/ssl/certs";

#[test]
fn test_parse_config_with_all_values() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
key_path = "/path/to/key"
cert_path = "/path/to/cert"

[c8y]
url = "your-tenant.cumulocity.com"
root_cert_path = "/path/to/c8y/root/cert"
connect = "true"

[az]
url = "MyAzure.azure-devices.net"
root_cert_path = "/path/to/azure/root/cert"
connect = "false"
mapper_timestamp = true

[mqtt]
port = 1234
external_port = 2345
external_bind_address = "0.0.0.0"
external_bind_interface = "wlan0"
external_capath = "ca.pem"
external_certfile = "cert.pem"
external_keyfile = "key.pem"
bind_address = "0.0.0.0"

[tmp]
path = "/some/tmp/path"

[logs]
path = "/some/log/path"

[run]
path = "/some/run/path"

[data]
path = "/some/data/path"

"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;

    let config = TEdgeConfigRepository::new(config_location).load_new()?;

    assert!(config.device_id().is_err());
    assert_eq!(config.device_key_path(), "/path/to/key");
    assert_eq!(config.device_cert_path(), "/path/to/cert");

    assert_eq!(config.c8y_url()?.as_str(), "your-tenant.cumulocity.com");
    assert_eq!(config.c8y_root_cert_path(), "/path/to/c8y/root/cert");

    assert_eq!(config.az_url()?.as_str(), "MyAzure.azure-devices.net");
    assert_eq!(config.az_root_cert_path(), "/path/to/azure/root/cert");
    assert_eq!(config.az_mapper_timestamp(), true);

    assert_eq!(config.mqtt_port(), 1234);

    assert_eq!(config.mqtt_external_port()?, 2345);

    assert_eq!(
        config.mqtt_external_bind_address()?,
        IpAddress::try_from("0.0.0.0").unwrap()
    );

    assert_eq!(config.mqtt_external_bind_interface()?, "wlan0");

    assert_eq!(config.mqtt_external_ca_path()?, "ca.pem");

    assert_eq!(config.mqtt_external_cert_file()?, "cert.pem");

    assert_eq!(config.mqtt_external_key_file()?, "key.pem");

    assert_eq!(
        config.mqtt_bind_address(),
        IpAddress::try_from("0.0.0.0").unwrap()
    );

    assert_eq!(config.service_service_type(), "service");
    assert_eq!(config.tmp_path(), "/some/tmp/path");
    assert_eq!(config.logs_path(), "/some/log/path");
    assert_eq!(config.run_path(), "/some/run/path");
    assert_eq!(config.data_path(), "/some/data/path");

    Ok(())
}

#[test]
fn test_store_config_with_all_values() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
key_path = "/path/to/key"
cert_path = "/path/to/cert"

[c8y]
url = "your-tenant.cumulocity.com"
root_cert_path = "/path/to/c8y/root/cert"

[az]
url = "MyAzure.azure-devices.net"
root_cert_path = "/path/to/azure/root/cert"
mapper_timestamp = false

[mqtt]
port = 1883
bind_address = "0.0.0.0"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config_repo = TEdgeConfigRepository::new(config_location);

    let updated_c8y_url = "other-tenant.cumulocity.com";
    let updated_azure_url = "OtherAzure.azure-devices.net";

    let updated_mqtt_port = 2345;
    let updated_mqtt_external_port = 3456;
    let updated_mqtt_external_bind_address = IpAddress::default();
    let updated_mqtt_external_bind_interface = "eth0";
    let updated_mqtt_external_capath = "/some/path";
    let updated_mqtt_external_certfile = "cert.pem";
    let updated_mqtt_external_keyfile = "key.pem";
    let updated_mqtt_bind_address = IpAddress(std::net::IpAddr::V4(Ipv4Addr::LOCALHOST));
    let updated_service_type = "systemd".to_string();

    let updated_tmp_path = Utf8PathBuf::from("some/tmp/path");
    let updated_log_path = Utf8PathBuf::from("some/log/path");
    let updated_run_path = Utf8PathBuf::from("some/run/path");
    let updated_data_path = Utf8PathBuf::from("some/data/path");

    let config = config_repo.load_new().unwrap();
    assert!(config.device_id().is_err());
    assert_eq!(config.device_key_path(), Utf8PathBuf::from("/path/to/key"));
    assert_eq!(
        config.device_cert_path(),
        Utf8PathBuf::from("/path/to/cert")
    );
    assert_eq!(config.c8y_url()?.as_str(), "your-tenant.cumulocity.com");
    assert_eq!(
        config.c8y_root_cert_path(),
        Utf8PathBuf::from("/path/to/c8y/root/cert")
    );
    assert_eq!(config.az_url()?.as_str(), "MyAzure.azure-devices.net");
    assert_eq!(
        config.az_root_cert_path(),
        Utf8PathBuf::from("/path/to/azure/root/cert")
    );
    assert_eq!(config.az_mapper_timestamp(), false);
    assert_eq!(config.mqtt_port(), 1883);

    assert_eq!(config.service_service_type(), "service");

    config_repo.update(C8yUrl(updated_c8y_url.try_into()?))?;
    config_repo.unset(WritableKey::C8yRootCertPath)?;
    config_repo.update(AzUrl(updated_azure_url.try_into()?))?;
    config_repo.update(MqttPort(updated_mqtt_port))?;
    config_repo.update(MqttExternalPort(updated_mqtt_external_port))?;
    config_repo.update(MqttExternalBindAddress(updated_mqtt_external_bind_address))?;
    config_repo.update(MqttExternalBindInterface(
        updated_mqtt_external_bind_interface.into(),
    ))?;
    config_repo
        .update(MqttExternalCaPath(updated_mqtt_external_capath.into()))
        .unwrap();
    config_repo
        .update(MqttExternalCertFile(updated_mqtt_external_certfile.into()))
        .unwrap();
    config_repo
        .update(MqttExternalKeyFile(updated_mqtt_external_keyfile.into()))
        .unwrap();

    config_repo.update(MqttBindAddress(updated_mqtt_bind_address))?;
    config_repo.update(ServiceServiceType(updated_service_type.clone()))?;
    config_repo.update(TmpPath(updated_tmp_path.clone()))?;
    config_repo.update(LogsPath(updated_log_path.clone()))?;
    config_repo.update(RunPath(updated_run_path.clone()))?;
    config_repo.update(DataPath(updated_data_path.clone()))?;

    {
        let config = config_repo.load_new()?;

        assert!(config.device_id().is_err());
        assert_eq!(config.device_key_path(), "/path/to/key");
        assert_eq!(config.device_cert_path(), "/path/to/cert");

        assert_eq!(config.c8y_url()?.as_str(), updated_c8y_url);
        assert_eq!(config.c8y_root_cert_path(), DEFAULT_ROOT_CERT_PATH);

        assert_eq!(config.az_url()?.as_str(), updated_azure_url);
        assert_eq!(config.az_root_cert_path(), "/path/to/azure/root/cert");
        assert_eq!(config.az_mapper_timestamp(), false);

        assert_eq!(config.mqtt_port(), updated_mqtt_port);
        assert_eq!(config.mqtt_external_port()?, updated_mqtt_external_port);
        assert_eq!(
            config.mqtt_external_bind_address()?,
            updated_mqtt_external_bind_address
        );
        assert_eq!(
            config.mqtt_external_bind_interface()?,
            updated_mqtt_external_bind_interface
        );
        assert_eq!(
            config.mqtt_external_ca_path()?,
            Utf8PathBuf::from(updated_mqtt_external_capath)
        );
        assert_eq!(
            config.mqtt_external_cert_file()?,
            Utf8PathBuf::from(updated_mqtt_external_certfile)
        );
        assert_eq!(
            config.mqtt_external_key_file()?,
            Utf8PathBuf::from(updated_mqtt_external_keyfile)
        );
        assert_eq!(config.mqtt_bind_address(), updated_mqtt_bind_address);
        assert_eq!(config.service_service_type(), updated_service_type);
        assert_eq!(config.tmp_path(), updated_tmp_path);
        assert_eq!(config.logs_path(), updated_log_path);
        assert_eq!(config.run_path(), updated_run_path);
        assert_eq!(config.data_path(), updated_data_path);
    }

    Ok(())
}

#[test]
fn test_parse_config_with_only_device_configuration() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config_dir = config_location.tedge_config_root_path().to_owned();

    let config = TEdgeConfigRepository::new(config_location).load_new()?;

    assert!(config.device_id().is_err());
    assert_eq!(
        config.device_cert_path(),
        format!("{config_dir}/device-certs/tedge-certificate.pem")
    );
    assert_eq!(
        config.device_key_path(),
        format!("{config_dir}/device-certs/tedge-private-key.pem")
    );

    assert!(config.c8y_url().is_err());
    assert_eq!(config.c8y_root_cert_path(), DEFAULT_ROOT_CERT_PATH);

    assert!(config.az_url().is_err());
    assert_eq!(config.az_root_cert_path(), DEFAULT_ROOT_CERT_PATH);
    assert_eq!(config.az_mapper_timestamp(), true);

    assert_eq!(config.mqtt_port(), 1883);
    assert_eq!(
        config.mqtt_bind_address(),
        IpAddress::try_from("127.0.0.1").unwrap()
    );
    Ok(())
}

#[test]
fn test_parse_config_with_only_c8y_configuration() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[c8y]
url = "your-tenant.cumulocity.com"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config_dir = config_location.tedge_config_root_path().to_owned();

    let config = TEdgeConfigRepository::new(config_location).load_new()?;

    assert!(config.device_id().is_err());
    assert_eq!(
        config.device_cert_path(),
        format!("{config_dir}/device-certs/tedge-certificate.pem")
    );
    assert_eq!(
        config.device_key_path(),
        format!("{config_dir}/device-certs/tedge-private-key.pem")
    );

    assert_eq!(config.c8y_url()?.as_str(), "your-tenant.cumulocity.com");
    assert_eq!(config.c8y_root_cert_path(), DEFAULT_ROOT_CERT_PATH);

    assert!(config.az_url().is_err());
    assert_eq!(config.az_root_cert_path(), DEFAULT_ROOT_CERT_PATH);
    assert_eq!(config.az_mapper_timestamp(), true);

    assert_eq!(config.mqtt_port(), 1883);
    assert_eq!(
        config.mqtt_bind_address(),
        IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST))
    );
    Ok(())
}

#[test]
fn test_parse_config_with_only_az_configuration() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[az]
url = "MyAzure.azure-devices.net"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config_dir = config_location.tedge_config_root_path().to_owned();

    let config = TEdgeConfigRepository::new(config_location).load_new()?;

    assert!(config.device_id().is_err());
    assert_eq!(
        config.device_cert_path(),
        format!("{config_dir}/device-certs/tedge-certificate.pem")
    );
    assert_eq!(
        config.device_key_path(),
        format!("{config_dir}/device-certs/tedge-private-key.pem")
    );

    assert!(config.c8y_url().is_err());
    assert_eq!(config.c8y_root_cert_path(), DEFAULT_ROOT_CERT_PATH);

    assert_eq!(config.az_url()?.as_str(), "MyAzure.azure-devices.net");
    assert_eq!(config.az_root_cert_path(), DEFAULT_ROOT_CERT_PATH);
    assert_eq!(config.az_mapper_timestamp(), true);

    assert_eq!(config.mqtt_port(), 1883);
    assert_eq!(
        config.mqtt_bind_address(),
        IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST))
    );
    Ok(())
}

#[test]
fn test_parse_config_with_only_mqtt_configuration() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[mqtt]
port = 2222
bind_address = "1.2.3.4"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config_dir = config_location.tedge_config_root_path().to_owned();

    let config = TEdgeConfigRepository::new(config_location).load_new()?;

    assert!(config.device_id().is_err());
    assert_eq!(
        config.device_cert_path(),
        format!("{config_dir}/device-certs/tedge-certificate.pem")
    );
    assert_eq!(
        config.device_key_path(),
        format!("{config_dir}/device-certs/tedge-private-key.pem")
    );

    assert!(config.c8y_url().is_err());
    assert_eq!(config.c8y_root_cert_path(), DEFAULT_ROOT_CERT_PATH);

    assert!(config.az_url().is_err());
    assert_eq!(config.az_root_cert_path(), DEFAULT_ROOT_CERT_PATH);
    assert_eq!(config.az_mapper_timestamp(), true);

    assert_eq!(config.mqtt_port(), 2222);
    assert_eq!(
        config.mqtt_bind_address(),
        IpAddress::try_from("1.2.3.4").unwrap()
    );
    Ok(())
}

#[test]
fn read_az_keys_from_old_version_config() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[azure]
url = "MyAzure.azure-devices.net"
root_cert_path = "/path/to/azure/root/cert"
mapper_timestamp = true
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;

    let config = TEdgeConfigRepository::new(config_location).load_new()?;

    assert_eq!(config.az_url()?.as_str(), "MyAzure.azure-devices.net");
    assert_eq!(config.az_root_cert_path(), "/path/to/azure/root/cert");
    assert_eq!(config.az_mapper_timestamp(), true);

    Ok(())
}

#[test]
fn set_az_keys_from_old_version_config() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[azure]
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config_repo = TEdgeConfigRepository::new(config_location.clone());
    let updated_azure_url = "OtherAzure.azure-devices.net";

    {
        let config = config_repo.load_new()?;

        assert!(config.az_url().is_err());
        assert_eq!(config.az_root_cert_path(), DEFAULT_ROOT_CERT_PATH);
        assert_eq!(config.az_mapper_timestamp(), true);
    }

    config_repo.update(AzUrl(updated_azure_url.try_into().unwrap()))?;
    config_repo.unset(WritableKey::AzRootCertPath)?;
    config_repo.unset(WritableKey::AzMapperTimestamp)?;

    {
        let config = config_repo.load_new()?;

        assert_eq!(config.az_url()?.as_str(), updated_azure_url);
        assert_eq!(config.az_root_cert_path(), DEFAULT_ROOT_CERT_PATH);
        assert_eq!(config.az_mapper_timestamp(), true);
    }

    let updated_raw_configuration =
        std::fs::read_to_string(config_location.tedge_config_file_path())?;
    assert!(!dbg!(&updated_raw_configuration).contains("[azure]"));
    assert!(updated_raw_configuration.contains("[az]"));

    Ok(())
}

#[test]
fn test_parse_config_empty_file() -> Result<(), TEdgeConfigError> {
    let toml_conf = "";

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config_dir = config_location.tedge_config_root_path().to_owned();

    let config = TEdgeConfigRepository::new(config_location).load_new()?;

    assert!(config.device_id().is_err());

    assert_eq!(
        config.device_cert_path(),
        format!("{config_dir}/device-certs/tedge-certificate.pem",)
    );
    assert_eq!(
        config.device_key_path(),
        format!("{config_dir}/device-certs/tedge-private-key.pem",)
    );

    assert!(config.c8y_url().is_err());
    assert_eq!(config.c8y_root_cert_path(), DEFAULT_ROOT_CERT_PATH);

    assert!(config.az_url().is_err());
    assert_eq!(config.az_root_cert_path(), DEFAULT_ROOT_CERT_PATH);
    assert_eq!(config.az_mapper_timestamp(), true);

    assert_eq!(config.mqtt_port(), 1883);
    assert_eq!(
        config.mqtt_bind_address(),
        IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST))
    );
    assert_eq!(config.firmware_child_update_timeout().as_secs(), 3600);
    assert_eq!(config.service_service_type(), "service");
    assert_eq!(config.tmp_path(), "/tmp");
    assert_eq!(config.logs_path(), "/var/log");
    assert_eq!(config.run_path(), "/run");
    assert_eq!(config.data_path(), "/var/tedge");

    Ok(())
}

#[test]
fn test_parse_config_no_config_file() -> Result<(), TEdgeConfigError> {
    let config_location = TEdgeConfigLocation::from_custom_root("/non/existent/path");
    let config_dir = config_location.tedge_config_root_path().to_owned();

    let config = TEdgeConfigRepository::new(config_location).load_new()?;

    assert!(config.device_id().is_err());
    assert_eq!(
        config.device_cert_path(),
        format!("{config_dir}/device-certs/tedge-certificate.pem",)
    );
    assert_eq!(
        config.device_key_path(),
        format!("{config_dir}/device-certs/tedge-private-key.pem",)
    );

    assert!(config.c8y_url().is_err());
    assert_eq!(config.c8y_root_cert_path(), DEFAULT_ROOT_CERT_PATH);

    assert!(config.az_url().is_err());
    assert_eq!(config.az_root_cert_path(), DEFAULT_ROOT_CERT_PATH);
    assert_eq!(config.az_mapper_timestamp(), true);

    assert_eq!(config.mqtt_port(), 1883);
    assert_eq!(
        config.mqtt_bind_address(),
        IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST))
    );
    Ok(())
}

#[test]
fn test_invalid_mqtt_port() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[mqtt]
port = "1883"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let toml_path = config_location.tedge_config_file_path().to_string();
    let result = TEdgeConfigRepository::new(config_location).load_new();

    let expected_err = format!("invalid type: found string \"1883\", expected u16 for key \"mqtt.port\" in {toml_path} TOML file");

    match result {
        Err(error @ TEdgeConfigError::Figment(_)) => assert_eq!(error.to_string(), expected_err),

        _ => panic!("Expected the parsing to fail with Figment error"),
    }

    Ok(())
}

#[test]
fn test_parse_invalid_toml_file() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
        <abcde>
        "#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let toml_path = config_location.tedge_config_file_path().to_string();
    let result = TEdgeConfigRepository::new(config_location).load_new();

    match result {
        Err(error @ TEdgeConfigError::Figment(_)) => {
            assert_eq!(
                error.to_string(),
                format!(
                    "unexpected character found: `<` at line 2 column 9 in {toml_path} TOML file"
                )
            )
        }

        _ => panic!("Expected the parsing to fail with Figment error"),
    }

    Ok(())
}

#[test]
fn test_crud_config_value() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
key_path = "/path/to/key"
cert_path = "/path/to/cert"

[c8y]
url = "your-tenant.cumulocity.com"
root_cert_path = "/path/to/c8y/root/cert"

[az]
url = "MyAzure.azure-devices.net"
root_cert_path = "/path/to/azure/root/cert"
mapper_timestamp = false

[mqtt]
port = 1024
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;

    let config_repo = TEdgeConfigRepository::new(config_location);
    let mut config = config_repo.load_new()?;

    let original_device_key_path = Utf8PathBuf::from("/path/to/key");
    let original_device_cert_path = Utf8PathBuf::from("/path/to/cert");

    assert!(config.device_id().is_err());
    assert_eq!(config.device_key_path(), original_device_key_path);
    assert_eq!(config.device_cert_path(), original_device_cert_path);

    let original_c8y_url = ConnectUrl::try_from("your-tenant.cumulocity.com")?;
    let original_c8y_root_cert_path = Utf8PathBuf::from("/path/to/c8y/root/cert");
    assert_eq!(config.c8y_url()?, original_c8y_url);
    assert_eq!(config.c8y_root_cert_path(), original_c8y_root_cert_path);

    let original_azure_url = ConnectUrl::try_from("MyAzure.azure-devices.net")?;
    let original_azure_root_cert_path = Utf8PathBuf::from("/path/to/azure/root/cert");
    assert_eq!(config.az_url()?, original_azure_url);
    assert_eq!(config.az_root_cert_path(), original_azure_root_cert_path);
    assert_eq!(config.az_mapper_timestamp(), false);

    assert_eq!(config.mqtt_port(), 1024);

    let updated_c8y_url = ConnectUrl::try_from("other-tenant.cumulocity.com")?;
    config_repo.update(C8yUrl(updated_c8y_url.clone()))?;
    config_repo.unset(WritableKey::C8yRootCertPath)?;

    let updated_azure_url = ConnectUrl::try_from("OtherAzure.azure-devices.net")?;
    config_repo.update(AzUrl(updated_azure_url.clone()))?;
    config_repo.unset(WritableKey::AzRootCertPath)?;
    config_repo.unset(WritableKey::AzMapperTimestamp)?;

    let updated_mqtt_port = 2048;
    config_repo.update(MqttPort(updated_mqtt_port))?;

    config = config_repo.load_new()?;

    assert_eq!(config.device_key_path(), original_device_key_path);
    assert_eq!(config.device_cert_path(), original_device_cert_path);

    assert_eq!(config.c8y_url()?, updated_c8y_url);
    assert_eq!(
        config.c8y_root_cert_path(),
        Utf8PathBuf::from("/etc/ssl/certs")
    );

    assert_eq!(config.az_url()?, updated_azure_url);
    assert_eq!(
        config.az_root_cert_path(),
        Utf8PathBuf::from("/etc/ssl/certs")
    );
    assert_eq!(config.az_mapper_timestamp(), true);

    assert_eq!(config.mqtt_port(), updated_mqtt_port);
    Ok(())
}

#[test]
fn test_any_device_id_provided_by_the_configuration_is_ignored() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
id = "ABCD1234"              # ignored for backward compatibility
cert_path = "/path/to/cert"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config = TEdgeConfigRepository::new(config_location).load_new()?;

    assert!(config.device_id().is_err());
    Ok(())
}

#[test]
fn test_device_id_is_none_when_there_is_no_certificate() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
cert_path = "/path/to/cert"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config = TEdgeConfigRepository::new(config_location).load_new()?;

    assert!(config.device_id().is_err());
    Ok(())
}

#[test]
fn test_device_id_is_err_when_cert_path_is_not_a_certificate() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
cert_path = "/path/to/cert"
"#;

    let (tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config_repo = TEdgeConfigRepository::new(config_location);

    let cert_path = tempdir.path().join("not-a-certificate.pem");
    std::fs::File::create(cert_path.clone()).expect("fail to create a fake certificate");
    config_repo.update(DeviceCertPath(cert_path.try_into().unwrap()))?;

    let config = config_repo.load_new()?;

    match config.device_id() {
        Err(ConfigSettingError::DerivationFailed { key, cause }) => {
            assert_eq!(key, "device.id");
            assert_eq!(cause.to_string(), "PEM file format error");
        }
        Err(e) => panic!("unexpected error: {e}"),
        Ok(_) => panic!("unexpected ok result"),
    }
    Ok(())
}

#[test]
fn test_device_id_is_extracted_from_device_certificate() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
cert_path = "/path/to/cert"
"#;
    let device_id = "device-serial-number";

    let (tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config_repo = TEdgeConfigRepository::new(config_location);

    let cert_path: Utf8PathBuf = tempdir.path().join("certificate.pem").try_into().unwrap();
    create_certificate(cert_path.clone(), device_id).expect("fail to create a certificate");
    config_repo.update(DeviceCertPath(cert_path))?;

    let config = config_repo.load_new()?;
    assert_eq!(config.device_id()?, device_id);

    Ok(())
}

fn create_temp_tedge_config(content: &str) -> std::io::Result<(TempTedgeDir, TEdgeConfigLocation)> {
    let dir = TempTedgeDir::new();
    dir.file("tedge.toml").with_raw_content(content);
    let config_location = TEdgeConfigLocation::from_custom_root(dir.path());
    Ok((dir, config_location))
}

fn create_certificate(
    path: camino::Utf8PathBuf,
    device_id: &str,
) -> Result<(), certificate::CertificateError> {
    let keypair = certificate::KeyCertPair::new_selfsigned_certificate(
        &certificate::NewCertificateConfig::default(),
        device_id,
    )?;
    let pem = keypair.certificate_pem_string()?;
    let mut file = std::fs::File::create(path)?;
    file.write_all(pem.as_bytes())?;
    Ok(())
}
