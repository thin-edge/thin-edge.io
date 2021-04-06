use assert_matches::assert_matches;
use std::convert::TryFrom;
use std::io::Write;
use std::path::Path;
use tedge_config::*;
use tempfile::NamedTempFile;

#[test]
fn test_parse_config_with_all_values() {
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

    let config_file = temp_file_with_content(toml_conf);
    let config = TEdgeConfigRepository::new("/home/user/.tedge".into())
        .from_custom_config(config_file.path())
        .unwrap();

    assert_eq!(config.query(DeviceIdSetting).unwrap(), "ABCD1234");
    assert_eq!(config.query(DeviceKeyPathSetting).unwrap(), "/path/to/key");
    assert_eq!(
        config.query(DeviceCertPathSetting).unwrap(),
        "/path/to/cert"
    );

    assert_eq!(
        config.query(C8yUrlSetting).unwrap().as_str(),
        "your-tenant.cumulocity.com"
    );
    assert_eq!(
        config.query(C8yRootCertPathSetting).unwrap(),
        "/path/to/c8y/root/cert"
    );

    assert_eq!(
        config.query(AzureUrlSetting).unwrap().as_str(),
        "MyAzure.azure-devices.net"
    );
    assert_eq!(
        config.query(AzureRootCertPathSetting).unwrap(),
        "/path/to/azure/root/cert"
    );
}

#[test]
fn test_write_to_custom_config() {
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

    // Using a TempPath let's close the file (this is required on Windows for that test to work).
    let config_file_path = temp_file_with_content(toml_conf).into_temp_path();

    let updated_device_id = "XYZ1234";
    let updated_c8y_url = "other-tenant.cumulocity.com";
    let updated_azure_url = "OtherAzure.azure-devices.net";

    {
        let mut config = TEdgeConfigRepository::new("/home/tedge/.tedge".into())
            .from_custom_config(config_file_path.as_ref())
            .unwrap();
        assert_eq!(config.query(DeviceIdSetting).unwrap(), "ABCD1234");
        assert_eq!(config.query(DeviceKeyPathSetting).unwrap(), "/path/to/key");
        assert_eq!(
            config.query(DeviceCertPathSetting).unwrap(),
            "/path/to/cert"
        );

        assert_eq!(
            config.query(C8yUrlSetting).unwrap().as_str(),
            "your-tenant.cumulocity.com"
        );
        assert_eq!(
            config.query(C8yRootCertPathSetting).unwrap(),
            "/path/to/c8y/root/cert"
        );

        assert_eq!(
            config.query(AzureUrlSetting).unwrap().as_str(),
            "MyAzure.azure-devices.net"
        );
        assert_eq!(
            config.query(AzureRootCertPathSetting).unwrap(),
            "/path/to/azure/root/cert"
        );

        config
            .update(DeviceIdSetting, updated_device_id.to_string())
            .unwrap();
        config
            .update(
                C8yUrlSetting,
                ConnectUrl::try_from(updated_c8y_url.to_string()).unwrap(),
            )
            .unwrap();
        config.unset(C8yRootCertPathSetting).unwrap();
        config
            .update(
                AzureUrlSetting,
                ConnectUrl::try_from(updated_azure_url.to_string()).unwrap(),
            )
            .unwrap();
        config.unset(AzureRootCertPathSetting).unwrap();
        config.persist().unwrap();
    }

    {
        let config = TEdgeConfigRepository::new("/home/tedge/.tedge".into())
            .from_custom_config(config_file_path.as_ref())
            .unwrap();

        assert_eq!(config.query(DeviceIdSetting).unwrap(), updated_device_id);
        assert_eq!(config.query(DeviceKeyPathSetting).unwrap(), "/path/to/key");
        assert_eq!(
            config.query(DeviceCertPathSetting).unwrap(),
            "/path/to/cert"
        );

        assert_eq!(
            config.query(C8yUrlSetting).unwrap().as_str(),
            updated_c8y_url
        );
        assert!(config
            .query_optional(C8yRootCertPathSetting)
            .unwrap()
            .is_none());

        assert_eq!(
            config.query(AzureUrlSetting).unwrap().as_str(),
            updated_azure_url
        );
        assert!(config
            .query_optional(AzureRootCertPathSetting)
            .unwrap()
            .is_none());
    }
}

#[test]
fn test_parse_config_missing_c8y_configuration() {
    let toml_conf = r#"
[device]
id = "ABCD1234"
"#;

    let config_file = temp_file_with_content(toml_conf);
    let config = TEdgeConfigRepository::new("/home/tedge/.tedge".into())
        .from_custom_config(config_file.path())
        .unwrap();

    assert_eq!(config.query(DeviceIdSetting).unwrap(), "ABCD1234");
    assert_eq!(
        config.query(DeviceCertPathSetting).unwrap(),
        "/home/tedge/.tedge/tedge-certificate.pem"
    );
    assert_eq!(
        config.query(DeviceKeyPathSetting).unwrap(),
        "/home/tedge/.tedge/tedge-private-key.pem"
    );

    assert!(config.query_optional(C8yUrlSetting).unwrap().is_none());
    assert!(config
        .query_optional(C8yRootCertPathSetting)
        .unwrap()
        .is_none());
}

#[test]
fn test_parse_config_missing_azure_configuration() {
    let toml_conf = r#"
[device]
id = "ABCD1234"
"#;

    let config_file = temp_file_with_content(toml_conf);
    let config = TEdgeConfigRepository::new("/home/tedge/.tedge".into())
        .from_custom_config(config_file.path())
        .unwrap();

    assert_eq!(config.query(DeviceIdSetting).unwrap(), "ABCD1234");
    assert_eq!(
        config.query(DeviceCertPathSetting).unwrap(),
        "/home/tedge/.tedge/tedge-certificate.pem"
    );
    assert_eq!(
        config.query(DeviceKeyPathSetting).unwrap(),
        "/home/tedge/.tedge/tedge-private-key.pem"
    );

    assert!(config.query_optional(AzureUrlSetting).unwrap().is_none());
    assert!(config
        .query_optional(AzureRootCertPathSetting)
        .unwrap()
        .is_none());
}

#[test]
fn test_parse_config_missing_device_configuration() {
    let toml_conf = r#"
[c8y]
url = "your-tenant.cumulocity.com"
"#;

    let config_file = temp_file_with_content(toml_conf);
    let config = TEdgeConfigRepository::new("/home/tedge/.tedge".into())
        .from_custom_config(config_file.path())
        .unwrap();

    assert_eq!(
        config.query(C8yUrlSetting).unwrap().as_str(),
        "your-tenant.cumulocity.com"
    );

    assert!(config.query_optional(DeviceIdSetting).unwrap().is_none());
    assert_eq!(
        config.query(DeviceCertPathSetting).unwrap(),
        "/home/tedge/.tedge/tedge-certificate.pem"
    );
    assert_eq!(
        config.query(DeviceKeyPathSetting).unwrap(),
        "/home/tedge/.tedge/tedge-private-key.pem"
    );
}

#[test]
fn test_parse_config_empty_file() {
    let config_file = NamedTempFile::new().unwrap();
    let config = TEdgeConfigRepository::new("/home/tedge/.tedge".into())
        .from_custom_config(config_file.path())
        .unwrap();

    assert!(config.query_optional(DeviceIdSetting).unwrap().is_none());
    assert_eq!(
        config.query(DeviceCertPathSetting).unwrap(),
        "/home/tedge/.tedge/tedge-certificate.pem"
    );
    assert_eq!(
        config.query(DeviceKeyPathSetting).unwrap(),
        "/home/tedge/.tedge/tedge-private-key.pem"
    );

    assert!(config.query_optional(C8yUrlSetting).unwrap().is_none());
    assert!(config
        .query_optional(C8yRootCertPathSetting)
        .unwrap()
        .is_none());
    assert!(config.query_optional(AzureUrlSetting).unwrap().is_none());
    assert!(config
        .query_optional(AzureRootCertPathSetting)
        .unwrap()
        .is_none());
}

#[test]
fn test_parse_config_no_config_file() {
    let config = TEdgeConfigRepository::new("/home/tedge/.tedge".into())
        .from_custom_config(Path::new("/non/existent/path"))
        .unwrap();

    assert!(config.query_optional(DeviceIdSetting).unwrap().is_none());
    assert!(config.query_optional(C8yUrlSetting).unwrap().is_none());
}

#[test]
fn test_parse_unsupported_keys() {
    let toml_conf = r#"
hey="tedge"
[c8y]
hello="tedge"
"#;

    let config_file = temp_file_with_content(toml_conf);
    let result = TEdgeConfigRepository::new("/home/tedge/.tedge".into())
        .from_custom_config(config_file.path());
    assert_matches!(
        result.unwrap_err(),
        TEdgeConfigError::TOMLParseError(_),
        "Expected the parsing to fail with TOMLParseError"
    );
}

#[test]
fn test_parse_invalid_toml_file() {
    let toml_conf = r#"
        <abcde>
        "#;

    let config_file = temp_file_with_content(toml_conf);
    let result = TEdgeConfigRepository::new("/home/tedge/.tedge".into())
        .from_custom_config(config_file.path());
    assert_matches!(
        result.unwrap_err(),
        TEdgeConfigError::TOMLParseError(_),
        "Expected the parsing to fail with TOMLParseError"
    );
}

#[test]
fn test_crud_config_value() {
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

    let config_file = temp_file_with_content(toml_conf);
    let mut config = TEdgeConfigRepository::new("/home/tedge/.tedge".into())
        .from_custom_config(config_file.path())
        .unwrap();

    let original_device_id = "ABCD1234".to_string();
    let original_device_key_path = "/path/to/key".to_string();
    let original_device_cert_path = "/path/to/cert".to_string();
    assert_eq!(config.query(DeviceIdSetting).unwrap(), original_device_id);
    assert_eq!(
        config.query(DeviceKeyPathSetting).unwrap(),
        original_device_key_path
    );
    assert_eq!(
        config.query(DeviceCertPathSetting).unwrap(),
        original_device_cert_path
    );

    let original_c8y_url = "your-tenant.cumulocity.com".to_string();
    let original_c8y_root_cert_path = "/path/to/c8y/root/cert".to_string();
    assert_eq!(
        config.query_string(C8yUrlSetting).unwrap(),
        original_c8y_url
    );
    assert_eq!(
        config.query_string(C8yRootCertPathSetting).unwrap(),
        original_c8y_root_cert_path
    );

    // let updated_device_id = "XYZ1234".to_string();
    let updated_c8y_url = ConnectUrl::try_from("other-tenant.cumulocity.com".to_string()).unwrap();

    // DeviceIdSetting.set_string(&mut config, updated_device_id.clone()).unwrap();
    config
        .update(C8yUrlSetting, updated_c8y_url.clone())
        .unwrap();

    config.unset(C8yRootCertPathSetting).unwrap();

    /*
    assert_eq!(
        config.get_config_value(DEVICE_ID).unwrap().unwrap(),
        updated_device_id
    );
    */
    assert_eq!(
        config.query(DeviceKeyPathSetting).unwrap(),
        original_device_key_path
    );
    assert_eq!(
        config.query(DeviceCertPathSetting).unwrap(),
        original_device_cert_path
    );

    assert_eq!(config.query(C8yUrlSetting).unwrap(), updated_c8y_url);
    assert!(config.query(C8yRootCertPathSetting).is_err());
}

#[test]
fn test_crud_config_value_azure() {
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

    let config_file = temp_file_with_content(toml_conf);
    let mut config = TEdgeConfigRepository::new("/home/tedge/.tedge".into())
        .from_custom_config(config_file.path())
        .unwrap();

    let original_azure_url = "MyAzure.azure-devices.net".to_string();
    let original_azure_root_cert_path = "/path/to/azure/root/cert".to_string();

    // read
    assert_eq!(
        config.query_string(AzureUrlSetting).unwrap(),
        original_azure_url
    );
    assert_eq!(
        config.query_string(AzureRootCertPathSetting).unwrap(),
        original_azure_root_cert_path
    );

    // set
    let updated_azure_url =
        ConnectUrl::try_from("OtherAzure.azure-devices.net".to_string()).unwrap();
    config
        .update(AzureUrlSetting, updated_azure_url.clone())
        .unwrap();

    assert_eq!(config.query(AzureUrlSetting).unwrap(), updated_azure_url);

    // unset
    config.unset(AzureRootCertPathSetting).unwrap();
    assert!(config.query_string(AzureRootCertPathSetting).is_err());
}

fn temp_file_with_content(content: &str) -> NamedTempFile {
    let file = NamedTempFile::new().unwrap();
    file.as_file().write_all(content.as_bytes()).unwrap();
    file
}
