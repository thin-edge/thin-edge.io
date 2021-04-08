use assert_matches::assert_matches;
use std::convert::TryFrom;
use std::io::Write;
use std::path::PathBuf;
use tedge_config::*;
use tempfile::TempDir;

#[test]
fn test_parse_config_with_all_values() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
id = "ABCD1234"
key_path = "/path/to/key"
cert_path = "/path/to/cert"

[c8y]
url = "your-tenant.cumulocity.com"
root_cert_path = "/path/to/c8y/root/cert"
connect = "true"

[azure]
url = "MyAzure.azure-devices.net"
root_cert_path = "/path/to/azure/root/cert"
connect = "false"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config = TEdgeConfigRepository::new(config_location).load()?;

    assert_eq!(config.query(DeviceIdSetting)?, "ABCD1234");
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

    Ok(())
}

#[test]
fn test_store_config() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
id = "ABCD1234"
key_path = "/path/to/key"
cert_path = "/path/to/cert"

[c8y]
url = "your-tenant.cumulocity.com"
root_cert_path = "/path/to/c8y/root/cert"

[azure]
url = "MyAzure.azure-devices.net"
root_cert_path = "/path/to/azure/root/cert"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config_repo = TEdgeConfigRepository::new(TEdgeConfigLocation {
        default_c8y_root_cert_path: FilePath::from("default_c8y_root_cert_path"),
        default_azure_root_cert_path: FilePath::from("default_azure_root_cert_path"),
        ..config_location
    });

    let updated_c8y_url = "other-tenant.cumulocity.com";
    let updated_azure_url = "OtherAzure.azure-devices.net";

    {
        let mut config = config_repo.load()?;
        assert_eq!(config.query(DeviceIdSetting)?, "ABCD1234");
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
        config.unset(AzureRootCertPathSetting)?;
        config_repo.store(config)?;
    }

    {
        let config = config_repo.load()?;

        assert_eq!(config.query(DeviceIdSetting)?, "ABCD1234");
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
    }

    Ok(())
}
#[test]
fn test_parse_config_missing_c8y_configuration() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
id = "ABCD1234"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config = TEdgeConfigRepository::new(TEdgeConfigLocation {
        default_device_cert_path: FilePath::from("/etc/ssl/certs/tedge-certificate.pem"),
        default_device_key_path: FilePath::from("/etc/ssl/certs/tedge-private-key.pem"),
        default_c8y_root_cert_path: FilePath::from("/etc/ssl/certs"),
        ..config_location
    })
    .load()?;

    assert_eq!(config.query(DeviceIdSetting)?, "ABCD1234");
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
id = "ABCD1234"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config = TEdgeConfigRepository::new(TEdgeConfigLocation {
        default_device_cert_path: FilePath::from("/etc/ssl/certs/tedge-certificate.pem"),
        default_device_key_path: FilePath::from("/etc/ssl/certs/tedge-private-key.pem"),
        default_c8y_root_cert_path: FilePath::from("/etc/ssl/certs"),
        ..config_location
    })
    .load()?;

    assert_eq!(config.query(DeviceIdSetting)?, "ABCD1234");
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
    let config = TEdgeConfigRepository::new(TEdgeConfigLocation {
        default_device_cert_path: FilePath::from("/etc/ssl/certs/tedge-certificate.pem"),
        default_device_key_path: FilePath::from("/etc/ssl/certs/tedge-private-key.pem"),
        ..config_location
    })
    .load()?;

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
    let config = TEdgeConfigRepository::new(TEdgeConfigLocation {
        default_device_cert_path: FilePath::from("/etc/ssl/certs/tedge-certificate.pem"),
        default_device_key_path: FilePath::from("/etc/ssl/certs/tedge-private-key.pem"),
        default_c8y_root_cert_path: FilePath::from("/etc/ssl/certs"),
        default_azure_root_cert_path: FilePath::from("/etc/ssl/certs"),
        ..config_location
    })
    .load()?;

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
    Ok(())
}

#[test]
fn test_parse_config_no_config_file() -> Result<(), TEdgeConfigError> {
    let config_location = TEdgeConfigLocation {
        tedge_config_path: PathBuf::from("/non/existent/path"),
        ..dummy_tedge_config_location()
    };

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
id = "ABCD1234"
key_path = "/path/to/key"
cert_path = "/path/to/cert"

[c8y]
url = "your-tenant.cumulocity.com"
root_cert_path = "/path/to/c8y/root/cert"

[azure]
url = "MyAzure.azure-devices.net"
root_cert_path = "/path/to/azure/root/cert"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let mut config = TEdgeConfigRepository::new(TEdgeConfigLocation {
        default_c8y_root_cert_path: FilePath::from("/etc/ssl/certs"),
        ..config_location
    })
    .load()?;

    let original_device_id = "ABCD1234".to_string();
    let original_device_key_path = FilePath::from("/path/to/key");
    let original_device_cert_path = FilePath::from("/path/to/cert");

    assert_eq!(config.query(DeviceIdSetting)?, original_device_id);
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
    Ok(())
}

#[test]
fn test_crud_config_value_azure() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
id = "ABCD1234"
key_path = "/path/to/key"
cert_path = "/path/to/cert"

[c8y]
url = "your-tenant.cumulocity.com"
root_cert_path = "/path/to/c8y/root/cert"

[azure]
url = "MyAzure.azure-devices.net"
root_cert_path = "/path/to/azure/root/cert"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let mut config = TEdgeConfigRepository::new(TEdgeConfigLocation {
        default_azure_root_cert_path: FilePath::from("/etc/ssl/certs"),
        ..config_location
    })
    .load()?;

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

fn create_temp_tedge_config(content: &str) -> std::io::Result<(TempDir, TEdgeConfigLocation)> {
    let dir = TempDir::new()?;
    let file_path = dir.path().join("tedge.toml");
    let mut file = std::fs::File::create(&file_path)?;
    file.write_all(content.as_bytes())?;
    let config_location = TEdgeConfigLocation {
        tedge_config_path: file_path,
        ..dummy_tedge_config_location()
    };
    Ok((dir, config_location))
}

fn dummy_tedge_config_location() -> TEdgeConfigLocation {
    TEdgeConfigLocation {
        tedge_config_path: PathBuf::from("/dev/null"),
        default_device_cert_path: FilePath::from("/dev/null"),
        default_device_key_path: FilePath::from("/dev/null"),
        default_c8y_root_cert_path: FilePath::from("/dev/null"),
        default_azure_root_cert_path: FilePath::from("/dev/null"),
    }
}
