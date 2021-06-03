use assert_matches::assert_matches;
use std::convert::TryFrom;
use std::io::Write;
use tedge_config::*;
use tempfile::TempDir;

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

[mqtt]
port = 1234
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config_defaults = dummy_tedge_config_defaults();

    let config =
        TEdgeConfigRepository::new_with_defaults(config_location, config_defaults).load()?;

    assert!(config.query_optional(DeviceIdSetting)?.is_none());
    assert_eq!(
        config.query(DeviceKeyPathSetting)?,
        FilePath::from("/path/to/key")
    );
    assert_eq!(
        config.query(DeviceCertPathSetting)?,
        FilePath::from("/path/to/cert")
    );

    assert_eq!(
        config.query(C8yUrlSetting)?.as_str(),
        "your-tenant.cumulocity.com"
    );
    assert_eq!(
        config.query(C8yRootCertPathSetting)?,
        FilePath::from("/path/to/c8y/root/cert")
    );

    assert_eq!(
        config.query(AzureUrlSetting)?.as_str(),
        "MyAzure.azure-devices.net"
    );
    assert_eq!(
        config.query(AzureRootCertPathSetting)?,
        FilePath::from("/path/to/azure/root/cert")
    );

    assert_eq!(config.query(MqttPortSetting)?, Port(1234));

    Ok(())
}

#[test]
fn test_store_config() -> Result<(), TEdgeConfigError> {
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
mapper_timestamp = true

[mqtt]
port = 1883
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config_defaults = TEdgeConfigDefaults {
        default_c8y_root_cert_path: FilePath::from("default_c8y_root_cert_path"),
        default_azure_root_cert_path: FilePath::from("default_azure_root_cert_path"),
        ..dummy_tedge_config_defaults()
    };

    let config_repo = TEdgeConfigRepository::new_with_defaults(config_location, config_defaults);

    let updated_c8y_url = "other-tenant.cumulocity.com";
    let updated_azure_url = "OtherAzure.azure-devices.net";
    let updated_mqtt_port = Port(2345);

    {
        let mut config = config_repo.load()?;
        assert!(config.query_optional(DeviceIdSetting)?.is_none());
        assert_eq!(
            config.query(DeviceKeyPathSetting)?,
            FilePath::from("/path/to/key")
        );
        assert_eq!(
            config.query(DeviceCertPathSetting)?,
            FilePath::from("/path/to/cert")
        );

        assert_eq!(
            config.query(C8yUrlSetting)?.as_str(),
            "your-tenant.cumulocity.com"
        );
        assert_eq!(
            config.query(C8yRootCertPathSetting)?,
            FilePath::from("/path/to/c8y/root/cert")
        );

        assert_eq!(
            config.query(AzureUrlSetting)?.as_str(),
            "MyAzure.azure-devices.net"
        );
        assert_eq!(
            config.query(AzureRootCertPathSetting)?,
            FilePath::from("/path/to/azure/root/cert")
        );

        config.update(C8yUrlSetting, ConnectUrl::try_from(updated_c8y_url)?)?;
        config.unset(C8yRootCertPathSetting)?;
        config.update(AzureUrlSetting, ConnectUrl::try_from(updated_azure_url)?)?;
        config.update(MqttPortSetting, updated_mqtt_port)?;
        config.unset(AzureRootCertPathSetting)?;
        config_repo.store(config)?;
    }

    {
        let config = config_repo.load()?;

        assert!(config.query_optional(DeviceIdSetting)?.is_none());
        assert_eq!(
            config.query(DeviceKeyPathSetting)?,
            FilePath::from("/path/to/key")
        );
        assert_eq!(
            config.query(DeviceCertPathSetting)?,
            FilePath::from("/path/to/cert")
        );

        assert_eq!(config.query(C8yUrlSetting)?.as_str(), updated_c8y_url);
        assert_eq!(
            config.query(C8yRootCertPathSetting)?,
            FilePath::from("default_c8y_root_cert_path")
        );

        assert_eq!(config.query(AzureUrlSetting)?.as_str(), updated_azure_url);
        assert_eq!(
            config.query(AzureRootCertPathSetting)?,
            FilePath::from("default_azure_root_cert_path")
        );

        assert_eq!(config.query(MqttPortSetting)?, updated_mqtt_port);
    }

    Ok(())
}

#[test]
fn test_parse_config_missing_c8y_configuration() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config_defaults = TEdgeConfigDefaults {
        default_device_cert_path: FilePath::from("/etc/ssl/certs/tedge-certificate.pem"),
        default_device_key_path: FilePath::from("/etc/ssl/certs/tedge-private-key.pem"),
        default_c8y_root_cert_path: FilePath::from("/etc/ssl/certs"),
        ..dummy_tedge_config_defaults()
    };

    let config =
        TEdgeConfigRepository::new_with_defaults(config_location, config_defaults).load()?;

    assert!(config.query_optional(DeviceIdSetting)?.is_none());
    assert_eq!(
        config.query(DeviceCertPathSetting)?,
        FilePath::from("/etc/ssl/certs/tedge-certificate.pem")
    );
    assert_eq!(
        config.query(DeviceKeyPathSetting)?,
        FilePath::from("/etc/ssl/certs/tedge-private-key.pem")
    );

    assert!(config.query_optional(C8yUrlSetting)?.is_none());
    assert_eq!(
        config.query(C8yRootCertPathSetting)?,
        FilePath::from("/etc/ssl/certs")
    );
    Ok(())
}

#[test]
fn test_parse_config_missing_azure_configuration() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config_defaults = TEdgeConfigDefaults {
        default_device_cert_path: FilePath::from("/etc/ssl/certs/tedge-certificate.pem"),
        default_device_key_path: FilePath::from("/etc/ssl/certs/tedge-private-key.pem"),
        default_c8y_root_cert_path: FilePath::from("/etc/ssl/certs"),
        ..dummy_tedge_config_defaults()
    };

    let config =
        TEdgeConfigRepository::new_with_defaults(config_location, config_defaults).load()?;

    assert!(config.query_optional(DeviceIdSetting)?.is_none());
    assert_eq!(
        config.query(DeviceCertPathSetting)?,
        FilePath::from("/etc/ssl/certs/tedge-certificate.pem")
    );
    assert_eq!(
        config.query(DeviceKeyPathSetting)?,
        FilePath::from("/etc/ssl/certs/tedge-private-key.pem")
    );

    assert_matches!(config.query_optional(AzureUrlSetting), Ok(None));
    assert_eq!(
        config.query(C8yRootCertPathSetting)?,
        FilePath::from("/etc/ssl/certs")
    );
    Ok(())
}

#[test]
fn test_parse_config_missing_device_configuration() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[c8y]
url = "your-tenant.cumulocity.com"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;

    let config_defaults = TEdgeConfigDefaults {
        default_device_cert_path: FilePath::from("/etc/ssl/certs/tedge-certificate.pem"),
        default_device_key_path: FilePath::from("/etc/ssl/certs/tedge-private-key.pem"),
        ..dummy_tedge_config_defaults()
    };

    let config =
        TEdgeConfigRepository::new_with_defaults(config_location, config_defaults).load()?;

    assert_eq!(
        config.query(C8yUrlSetting)?.as_str(),
        "your-tenant.cumulocity.com"
    );

    assert!(config.query_optional(DeviceIdSetting)?.is_none());
    assert_eq!(
        config.query(DeviceCertPathSetting)?,
        FilePath::from("/etc/ssl/certs/tedge-certificate.pem")
    );
    assert_eq!(
        config.query(DeviceKeyPathSetting)?,
        FilePath::from("/etc/ssl/certs/tedge-private-key.pem"),
    );
    Ok(())
}

#[test]
fn test_parse_config_empty_file() -> Result<(), TEdgeConfigError> {
    let toml_conf = "";

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;

    let config_defaults = TEdgeConfigDefaults {
        default_device_cert_path: FilePath::from("/etc/ssl/certs/tedge-certificate.pem"),
        default_device_key_path: FilePath::from("/etc/ssl/certs/tedge-private-key.pem"),
        default_c8y_root_cert_path: FilePath::from("/etc/ssl/certs"),
        default_azure_root_cert_path: FilePath::from("/etc/ssl/certs"),
        ..dummy_tedge_config_defaults()
    };

    let config =
        TEdgeConfigRepository::new_with_defaults(config_location, config_defaults).load()?;

    assert!(config.query_optional(DeviceIdSetting)?.is_none());

    assert_eq!(
        config.query(DeviceCertPathSetting)?,
        FilePath::from("/etc/ssl/certs/tedge-certificate.pem")
    );
    assert_eq!(
        config.query(DeviceKeyPathSetting)?,
        FilePath::from("/etc/ssl/certs/tedge-private-key.pem"),
    );

    assert!(config.query_optional(C8yUrlSetting)?.is_none());
    assert_eq!(
        config.query(C8yRootCertPathSetting)?,
        FilePath::from("/etc/ssl/certs")
    );

    assert_matches!(config.query_optional(AzureUrlSetting), Ok(None));
    assert_eq!(
        config.query(AzureRootCertPathSetting)?,
        FilePath::from("/etc/ssl/certs")
    );

    assert_eq!(config.query(MqttPortSetting)?, Port(1883));
    Ok(())
}

#[test]
fn test_parse_config_no_config_file() -> Result<(), TEdgeConfigError> {
    let config_location = TEdgeConfigLocation::from_custom_root("/non/existent/path");
    let config = TEdgeConfigRepository::new(config_location).load()?;

    assert!(config.query_optional(DeviceIdSetting)?.is_none());
    assert!(config.query_optional(C8yUrlSetting)?.is_none());
    Ok(())
}

#[test]
fn test_parse_unsupported_keys() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
hey="tedge"
[c8y]
hello="tedge"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let result = TEdgeConfigRepository::new(config_location).load();

    assert_matches!(
        result,
        Err(TEdgeConfigError::TOMLParseError(_)),
        "Expected the parsing to fail with TOMLParseError"
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
    let result = TEdgeConfigRepository::new(config_location).load();

    let expected_err =
        "invalid type: string \"1883\", expected u16 for key `mqtt.port` at line 3 column 8";

    match result {
        Err(TEdgeConfigError::TOMLParseError(err)) => assert_eq!(err.to_string(), expected_err),

        _ => assert!(false, "Expected the parsing to fail with TOMLParseError"),
    }

    Ok(())
}

#[test]
fn test_parse_invalid_toml_file() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
        <abcde>
        "#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let result = TEdgeConfigRepository::new(config_location).load();

    assert_matches!(
        result,
        Err(TEdgeConfigError::TOMLParseError(_)),
        "Expected the parsing to fail with TOMLParseError"
    );
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

[mqtt]
port = 1024
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;

    let config_defaults = TEdgeConfigDefaults {
        default_c8y_root_cert_path: FilePath::from("/etc/ssl/certs"),
        ..dummy_tedge_config_defaults()
    };

    let mut config =
        TEdgeConfigRepository::new_with_defaults(config_location, config_defaults).load()?;

    let original_device_key_path = FilePath::from("/path/to/key");
    let original_device_cert_path = FilePath::from("/path/to/cert");

    assert!(config.query_optional(DeviceIdSetting)?.is_none());
    assert_eq!(
        config.query(DeviceKeyPathSetting)?,
        original_device_key_path
    );
    assert_eq!(
        config.query(DeviceCertPathSetting)?,
        original_device_cert_path
    );

    let original_c8y_url = ConnectUrl::try_from("your-tenant.cumulocity.com")?;
    let original_c8y_root_cert_path = FilePath::from("/path/to/c8y/root/cert");
    assert_eq!(config.query(C8yUrlSetting)?, original_c8y_url);
    assert_eq!(
        config.query(C8yRootCertPathSetting)?,
        original_c8y_root_cert_path
    );

    let updated_c8y_url = ConnectUrl::try_from("other-tenant.cumulocity.com")?;

    config.update(C8yUrlSetting, updated_c8y_url.clone())?;

    let updated_mqtt_port = Port(2048);
    config.update(MqttPortSetting, updated_mqtt_port.clone())?;

    config.unset(C8yRootCertPathSetting)?;

    assert_eq!(
        config.query(DeviceKeyPathSetting)?,
        original_device_key_path
    );
    assert_eq!(
        config.query(DeviceCertPathSetting)?,
        original_device_cert_path
    );

    assert_eq!(config.query(C8yUrlSetting)?, updated_c8y_url);
    assert_eq!(
        config.query(C8yRootCertPathSetting)?,
        FilePath::from("/etc/ssl/certs")
    );

    assert_eq!(config.query(MqttPortSetting)?, updated_mqtt_port);
    Ok(())
}

#[test]
fn test_crud_config_value_azure() -> Result<(), TEdgeConfigError> {
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
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;

    let config_defaults = TEdgeConfigDefaults {
        default_azure_root_cert_path: FilePath::from("/etc/ssl/certs"),
        ..dummy_tedge_config_defaults()
    };

    let mut config =
        TEdgeConfigRepository::new_with_defaults(config_location, config_defaults).load()?;

    let original_azure_url = ConnectUrl::try_from("MyAzure.azure-devices.net")?;
    let original_azure_root_cert_path = FilePath::from("/path/to/azure/root/cert");

    // read
    assert_eq!(config.query(AzureUrlSetting)?, original_azure_url);
    assert_eq!(
        config.query(AzureRootCertPathSetting)?,
        original_azure_root_cert_path
    );

    // set
    let updated_azure_url = ConnectUrl::try_from("OtherAzure.azure-devices.net")?;
    config.update(AzureUrlSetting, updated_azure_url.clone())?;

    assert_eq!(config.query(AzureUrlSetting)?, updated_azure_url);

    // unset
    config.unset(AzureRootCertPathSetting)?;

    assert_eq!(
        config.query(AzureRootCertPathSetting)?,
        FilePath::from("/etc/ssl/certs")
    );
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
    let config =
        TEdgeConfigRepository::new_with_defaults(config_location, dummy_tedge_config_defaults())
            .load()?;

    assert!(config.query_optional(DeviceIdSetting)?.is_none());
    Ok(())
}

#[test]
fn test_device_id_is_none_when_there_is_no_certificate() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
cert_path = "/path/to/cert"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config =
        TEdgeConfigRepository::new_with_defaults(config_location, dummy_tedge_config_defaults())
            .load()?;

    assert!(config.query_optional(DeviceIdSetting)?.is_none());
    Ok(())
}

#[test]
fn test_device_id_is_err_when_cert_path_is_not_a_certificate() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
cert_path = "/path/to/cert"
"#;

    let (tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let mut config =
        TEdgeConfigRepository::new_with_defaults(config_location, dummy_tedge_config_defaults())
            .load()?;

    let cert_path = tempdir.path().join("not-a-certificate.pem");
    std::fs::File::create(cert_path.clone()).expect("fail to create a fake certificate");
    config.update(DeviceCertPathSetting, cert_path.into())?;

    match config.query(DeviceIdSetting) {
        Err(ConfigSettingError::DerivationFailed { key, cause }) => {
            assert_eq!(key, "device.id");
            assert_eq!(cause, "PEM file format error");
        }
        Err(_) => assert!(false, "unexpected error"),
        Ok(_) => assert!(false, "unexpected ok result"),
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
    let mut config =
        TEdgeConfigRepository::new_with_defaults(config_location, dummy_tedge_config_defaults())
            .load()?;

    let cert_path = tempdir.path().join("certificate.pem");
    create_certificate(cert_path.clone(), device_id).expect("fail to create a certificate");
    config.update(DeviceCertPathSetting, cert_path.into())?;

    assert_eq!(config.query(DeviceIdSetting)?, device_id);

    Ok(())
}

fn create_temp_tedge_config(content: &str) -> std::io::Result<(TempDir, TEdgeConfigLocation)> {
    let dir = TempDir::new()?;
    let config_location = TEdgeConfigLocation::from_custom_root(dir.path());
    let mut file = std::fs::File::create(config_location.tedge_config_file_path())?;
    file.write_all(content.as_bytes())?;
    Ok((dir, config_location))
}

fn dummy_tedge_config_defaults() -> TEdgeConfigDefaults {
    TEdgeConfigDefaults {
        default_device_cert_path: FilePath::from("/dev/null"),
        default_device_key_path: FilePath::from("/dev/null"),
        default_c8y_root_cert_path: FilePath::from("/dev/null"),
        default_azure_root_cert_path: FilePath::from("/dev/null"),
        default_mqtt_port: Port(1883),
    }
}

fn create_certificate(
    path: std::path::PathBuf,
    device_id: &str,
) -> Result<(), certificate::CertificateError> {
    let keypair = certificate::KeyCertPair::new_selfsigned_certificate(
        &certificate::NewCertificateConfig::default(),
        device_id,
    )?;
    let pem = keypair.certificate_pem_string()?;
    let mut file = std::fs::File::create(path.clone())?;
    file.write_all(pem.as_bytes())?;
    Ok(())
}
