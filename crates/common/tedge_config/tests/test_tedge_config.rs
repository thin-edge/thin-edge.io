use camino::Utf8PathBuf;
use std::convert::TryFrom;
use std::io::Write;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use tedge_config::*;
use tedge_test_utils::fs::TempTedgeDir;

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
    let config_defaults = dummy_tedge_config_defaults();

    let config =
        TEdgeConfigRepository::new_with_defaults(config_location, config_defaults).load()?;

    assert!(config.query_optional(DeviceIdSetting).is_err());
    assert_eq!(
        config.query(DeviceKeyPathSetting)?,
        Utf8PathBuf::from("/path/to/key")
    );
    assert_eq!(
        config.query(DeviceCertPathSetting)?,
        Utf8PathBuf::from("/path/to/cert")
    );

    assert_eq!(
        config.query(C8yHttpSetting)?.as_str(),
        "your-tenant.cumulocity.com"
    );
    assert_eq!(
        config.query(C8yMqttSetting)?.as_str(),
        "your-tenant.cumulocity.com"
    );
    assert_eq!(
        config.query(C8yRootCertPathSetting)?,
        Utf8PathBuf::from("/path/to/c8y/root/cert")
    );

    assert_eq!(
        config.query(AzureUrlSetting)?.as_str(),
        "MyAzure.azure-devices.net"
    );
    assert_eq!(
        config.query(AzureRootCertPathSetting)?,
        Utf8PathBuf::from("/path/to/azure/root/cert")
    );
    assert_eq!(config.query(AzureMapperTimestamp)?, Flag(true));

    assert_eq!(config.query(MqttPortSetting)?, Port(1234));

    assert_eq!(config.query(MqttExternalPortSetting)?, Port(2345));

    assert_eq!(
        config.query(MqttExternalBindAddressSetting)?,
        IpAddress::try_from("0.0.0.0").unwrap()
    );

    assert_eq!(
        config.query(MqttExternalBindInterfaceSetting)?.as_str(),
        "wlan0"
    );

    assert_eq!(
        config.query(MqttExternalCAPathSetting)?,
        Utf8PathBuf::from("ca.pem")
    );

    assert_eq!(
        config.query(MqttExternalCertfileSetting)?,
        Utf8PathBuf::from("cert.pem")
    );

    assert_eq!(
        config.query(MqttExternalKeyfileSetting)?,
        Utf8PathBuf::from("key.pem")
    );

    assert_eq!(
        config.query(MqttBindAddressSetting)?,
        IpAddress::try_from("0.0.0.0").unwrap()
    );

    assert_eq!(config.query(ServiceTypeSetting)?, "service");
    assert_eq!(
        config.query(TmpPathSetting)?,
        Utf8PathBuf::from("/some/tmp/path")
    );
    assert_eq!(
        config.query(LogPathSetting)?,
        Utf8PathBuf::from("/some/log/path")
    );
    assert_eq!(
        config.query(RunPathSetting)?,
        Utf8PathBuf::from("/some/run/path")
    );
    assert_eq!(
        config.query(DataPathSetting)?,
        Utf8PathBuf::from("/some/data/path")
    );

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
    let config_defaults = TEdgeConfigDefaults {
        default_c8y_root_cert_path: Utf8PathBuf::from("default_c8y_root_cert_path"),
        default_azure_root_cert_path: Utf8PathBuf::from("default_azure_root_cert_path"),
        default_aws_root_cert_path: Utf8PathBuf::from("default_aws_root_cert_path"),
        ..dummy_tedge_config_defaults()
    };

    let config_repo = TEdgeConfigRepository::new_with_defaults(config_location, config_defaults);

    let updated_c8y_url = "other-tenant.cumulocity.com";
    let updated_azure_url = "OtherAzure.azure-devices.net";

    let updated_mqtt_port = Port(2345);
    let updated_mqtt_external_port = Port(3456);
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

    config_repo.update_toml(&|config| {
        assert!(config.query_optional(DeviceIdSetting).is_err());
        assert_eq!(
            config.query(DeviceKeyPathSetting)?,
            Utf8PathBuf::from("/path/to/key")
        );
        assert_eq!(
            config.query(DeviceCertPathSetting)?,
            Utf8PathBuf::from("/path/to/cert")
        );

        assert_eq!(
            config.query(C8yHttpSetting)?.as_str(),
            "your-tenant.cumulocity.com"
        );
        assert_eq!(
            config.query(C8yMqttSetting)?.as_str(),
            "your-tenant.cumulocity.com"
        );
        assert_eq!(
            config.query(C8yRootCertPathSetting)?,
            Utf8PathBuf::from("/path/to/c8y/root/cert")
        );

        assert_eq!(
            config.query(AzureUrlSetting)?.as_str(),
            "MyAzure.azure-devices.net"
        );
        assert_eq!(
            config.query(AzureRootCertPathSetting)?,
            Utf8PathBuf::from("/path/to/azure/root/cert")
        );
        assert_eq!(config.query(AzureMapperTimestamp)?, Flag(false));

        assert_eq!(config.query(MqttPortSetting)?, Port(1883));

        assert_eq!(config.query(ServiceTypeSetting)?, "service");

        #[allow(deprecated)]
        config.update(
            C8yUrlSetting,
            ConnectUrl::try_from(updated_c8y_url).unwrap(),
        )?;
        config.unset(C8yRootCertPathSetting)?;
        config.update(
            AzureUrlSetting,
            ConnectUrl::try_from(updated_azure_url).unwrap(),
        )?;
        config.unset(AzureRootCertPathSetting)?;
        config.unset(AzureMapperTimestamp)?;
        config.update(MqttPortSetting, updated_mqtt_port)?;
        config.update(MqttExternalPortSetting, updated_mqtt_external_port)?;
        config.update(
            MqttExternalBindAddressSetting,
            updated_mqtt_external_bind_address.clone(),
        )?;

        config.update(
            MqttExternalBindInterfaceSetting,
            updated_mqtt_external_bind_interface.to_string(),
        )?;
        config.update(
            MqttExternalCAPathSetting,
            Utf8PathBuf::from(updated_mqtt_external_capath),
        )?;
        config.update(
            MqttExternalCertfileSetting,
            Utf8PathBuf::from(updated_mqtt_external_certfile),
        )?;
        config.update(
            MqttExternalKeyfileSetting,
            Utf8PathBuf::from(updated_mqtt_external_keyfile),
        )?;
        config.update(MqttBindAddressSetting, updated_mqtt_bind_address.clone())?;
        config.update(ServiceTypeSetting, updated_service_type.clone())?;
        config.update(TmpPathSetting, updated_tmp_path.clone())?;
        config.update(LogPathSetting, updated_log_path.clone())?;
        config.update(RunPathSetting, updated_run_path.clone())?;
        config.update(DataPathSetting, updated_data_path.clone())?;

        Ok(())
    })?;

    {
        let config = config_repo.load()?;

        assert!(config.query_optional(DeviceIdSetting).is_err());
        assert_eq!(
            config.query(DeviceKeyPathSetting)?,
            Utf8PathBuf::from("/path/to/key")
        );
        assert_eq!(
            config.query(DeviceCertPathSetting)?,
            Utf8PathBuf::from("/path/to/cert")
        );

        assert_eq!(config.query(C8yHttpSetting)?.as_str(), updated_c8y_url);
        assert_eq!(config.query(C8yMqttSetting)?.as_str(), updated_c8y_url);
        assert_eq!(
            config.query(C8yRootCertPathSetting)?,
            Utf8PathBuf::from("default_c8y_root_cert_path")
        );

        assert_eq!(config.query(AzureUrlSetting)?.as_str(), updated_azure_url);
        assert_eq!(
            config.query(AzureRootCertPathSetting)?,
            Utf8PathBuf::from("default_azure_root_cert_path")
        );
        assert_eq!(config.query(AzureMapperTimestamp)?, Flag(true));

        assert_eq!(config.query(MqttPortSetting)?, updated_mqtt_port);
        assert_eq!(
            config.query(MqttExternalPortSetting)?,
            updated_mqtt_external_port
        );
        assert_eq!(
            config.query(MqttExternalBindAddressSetting)?,
            updated_mqtt_external_bind_address
        );
        assert_eq!(
            config.query(MqttExternalBindInterfaceSetting)?.as_str(),
            updated_mqtt_external_bind_interface
        );
        assert_eq!(
            config.query(MqttExternalCAPathSetting)?,
            Utf8PathBuf::from(updated_mqtt_external_capath)
        );
        assert_eq!(
            config.query(MqttExternalCertfileSetting)?,
            Utf8PathBuf::from(updated_mqtt_external_certfile)
        );
        assert_eq!(
            config.query(MqttExternalKeyfileSetting)?,
            Utf8PathBuf::from(updated_mqtt_external_keyfile)
        );
        assert_eq!(
            config.query(MqttBindAddressSetting)?,
            updated_mqtt_bind_address
        );
        assert_eq!(config.query(ServiceTypeSetting)?, updated_service_type);
        assert_eq!(config.query(TmpPathSetting)?, updated_tmp_path);
        assert_eq!(config.query(LogPathSetting)?, updated_log_path);
        assert_eq!(config.query(RunPathSetting)?, updated_run_path);
        assert_eq!(config.query(DataPathSetting)?, updated_data_path);
    }

    Ok(())
}

#[test]
fn test_parse_config_with_only_device_configuration() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[device]
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config_defaults = TEdgeConfigDefaults {
        default_device_cert_path: Utf8PathBuf::from("/etc/ssl/certs/tedge-certificate.pem"),
        default_device_key_path: Utf8PathBuf::from("/etc/ssl/certs/tedge-private-key.pem"),
        default_c8y_root_cert_path: Utf8PathBuf::from("/etc/ssl/certs"),
        default_azure_root_cert_path: Utf8PathBuf::from("/etc/ssl/certs"),
        ..dummy_tedge_config_defaults()
    };

    let config =
        TEdgeConfigRepository::new_with_defaults(config_location, config_defaults).load()?;

    assert!(config.query_optional(DeviceIdSetting).is_err());
    assert_eq!(
        config.query(DeviceCertPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs/tedge-certificate.pem")
    );
    assert_eq!(
        config.query(DeviceKeyPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs/tedge-private-key.pem")
    );

    assert!(config.query_optional(C8yHttpSetting)?.is_none());
    assert!(config.query_optional(C8yMqttSetting)?.is_none());
    assert_eq!(
        config.query(C8yRootCertPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs")
    );

    assert!(config.query_optional(AzureUrlSetting)?.is_none());
    assert_eq!(
        config.query(AzureRootCertPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs")
    );
    assert_eq!(config.query(AzureMapperTimestamp)?, Flag(true));

    assert_eq!(config.query(MqttPortSetting)?, Port(1883));
    assert_eq!(
        config.query(MqttBindAddressSetting)?,
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

    let config_defaults = TEdgeConfigDefaults {
        default_device_cert_path: Utf8PathBuf::from("/etc/ssl/certs/tedge-certificate.pem"),
        default_device_key_path: Utf8PathBuf::from("/etc/ssl/certs/tedge-private-key.pem"),
        ..dummy_tedge_config_defaults()
    };

    let config =
        TEdgeConfigRepository::new_with_defaults(config_location, config_defaults).load()?;

    assert!(config.query_optional(DeviceIdSetting).is_err());
    assert_eq!(
        config.query(DeviceCertPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs/tedge-certificate.pem")
    );
    assert_eq!(
        config.query(DeviceKeyPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs/tedge-private-key.pem"),
    );

    assert_eq!(
        config.query(C8yHttpSetting)?.as_str(),
        "your-tenant.cumulocity.com"
    );
    assert_eq!(
        config.query(C8yMqttSetting)?.as_str(),
        "your-tenant.cumulocity.com"
    );
    assert_eq!(
        config.query(C8yRootCertPathSetting)?,
        Utf8PathBuf::from("/dev/null")
    );

    assert!(config.query_optional(AzureUrlSetting)?.is_none());
    assert_eq!(
        config.query(AzureRootCertPathSetting)?,
        Utf8PathBuf::from("/dev/null")
    );
    assert_eq!(config.query(AzureMapperTimestamp)?, Flag(true));

    assert_eq!(config.query(MqttPortSetting)?, Port(1883));
    assert_eq!(
        config.query(MqttBindAddressSetting)?,
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

    let config_defaults = TEdgeConfigDefaults {
        default_device_cert_path: Utf8PathBuf::from("/etc/ssl/certs/tedge-certificate.pem"),
        default_device_key_path: Utf8PathBuf::from("/etc/ssl/certs/tedge-private-key.pem"),
        ..dummy_tedge_config_defaults()
    };

    let config =
        TEdgeConfigRepository::new_with_defaults(config_location, config_defaults).load()?;

    assert!(config.query_optional(DeviceIdSetting).is_err());
    assert_eq!(
        config.query(DeviceCertPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs/tedge-certificate.pem")
    );
    assert_eq!(
        config.query(DeviceKeyPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs/tedge-private-key.pem"),
    );

    assert!(config.query_optional(C8yHttpSetting)?.is_none());
    assert!(config.query_optional(C8yMqttSetting)?.is_none());
    assert_eq!(
        config.query(C8yRootCertPathSetting)?,
        Utf8PathBuf::from("/dev/null")
    );

    assert_eq!(
        config.query(AzureUrlSetting)?.as_str(),
        "MyAzure.azure-devices.net"
    );
    assert_eq!(
        config.query(AzureRootCertPathSetting)?,
        Utf8PathBuf::from("/dev/null")
    );
    assert_eq!(config.query(AzureMapperTimestamp)?, Flag(true));

    assert_eq!(config.query(MqttPortSetting)?, Port(1883));
    assert_eq!(
        config.query(MqttBindAddressSetting)?,
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

    let config_defaults = TEdgeConfigDefaults {
        default_device_cert_path: Utf8PathBuf::from("/etc/ssl/certs/tedge-certificate.pem"),
        default_device_key_path: Utf8PathBuf::from("/etc/ssl/certs/tedge-private-key.pem"),
        ..dummy_tedge_config_defaults()
    };

    let config =
        TEdgeConfigRepository::new_with_defaults(config_location, config_defaults).load()?;

    assert!(config.query_optional(DeviceIdSetting).is_err());
    assert_eq!(
        config.query(DeviceCertPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs/tedge-certificate.pem")
    );
    assert_eq!(
        config.query(DeviceKeyPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs/tedge-private-key.pem"),
    );

    assert!(config.query_optional(C8yHttpSetting)?.is_none());
    assert!(config.query_optional(C8yMqttSetting)?.is_none());
    assert_eq!(
        config.query(C8yRootCertPathSetting)?,
        Utf8PathBuf::from("/dev/null")
    );

    assert!(config.query_optional(AzureUrlSetting)?.is_none());
    assert_eq!(
        config.query(AzureRootCertPathSetting)?,
        Utf8PathBuf::from("/dev/null")
    );
    assert_eq!(config.query(AzureMapperTimestamp)?, Flag(true));

    assert_eq!(config.query(MqttPortSetting)?, Port(2222));
    assert_eq!(
        config.query(MqttBindAddressSetting)?,
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
    let config_defaults = dummy_tedge_config_defaults();

    let config =
        TEdgeConfigRepository::new_with_defaults(config_location, config_defaults).load()?;

    assert_eq!(
        config.query(AzureUrlSetting)?.as_str(),
        "MyAzure.azure-devices.net"
    );
    assert_eq!(
        config.query(AzureRootCertPathSetting)?,
        Utf8PathBuf::from("/path/to/azure/root/cert")
    );
    assert_eq!(config.query(AzureMapperTimestamp)?, Flag(true));

    Ok(())
}

#[test]
fn set_az_keys_from_old_version_config() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[azure]
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let config_defaults = TEdgeConfigDefaults {
        default_azure_root_cert_path: Utf8PathBuf::from("default_azure_root_cert_path"),
        ..dummy_tedge_config_defaults()
    };
    let config_repo = TEdgeConfigRepository::new_with_defaults(config_location, config_defaults);
    let updated_azure_url = "OtherAzure.azure-devices.net";

    config_repo.update_toml(&|config| {
        assert!(config.query_optional(AzureUrlSetting)?.is_none());
        assert_eq!(
            config.query(AzureRootCertPathSetting)?,
            Utf8PathBuf::from("default_azure_root_cert_path")
        );
        assert_eq!(config.query(AzureMapperTimestamp)?, Flag(true));

        config.update(
            AzureUrlSetting,
            ConnectUrl::try_from(updated_azure_url).unwrap(),
        )?;
        config.unset(AzureRootCertPathSetting)?;
        config.unset(AzureMapperTimestamp)?;

        Ok(())
    })?;

    {
        let config = config_repo.load()?;

        assert_eq!(config.query(AzureUrlSetting)?.as_str(), updated_azure_url);
        assert_eq!(
            config.query(AzureRootCertPathSetting)?,
            Utf8PathBuf::from("default_azure_root_cert_path")
        );
        assert_eq!(config.query(AzureMapperTimestamp)?, Flag(true));
    }

    Ok(())
}

#[test]
fn test_parse_config_empty_file() -> Result<(), TEdgeConfigError> {
    let toml_conf = "";

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;

    let config_defaults = TEdgeConfigDefaults {
        default_device_cert_path: Utf8PathBuf::from("/etc/ssl/certs/tedge-certificate.pem"),
        default_device_key_path: Utf8PathBuf::from("/etc/ssl/certs/tedge-private-key.pem"),
        default_c8y_root_cert_path: Utf8PathBuf::from("/etc/ssl/certs"),
        default_azure_root_cert_path: Utf8PathBuf::from("/etc/ssl/certs"),
        ..dummy_tedge_config_defaults()
    };

    let config =
        TEdgeConfigRepository::new_with_defaults(config_location, config_defaults).load()?;

    assert!(config.query_optional(DeviceIdSetting).is_err());

    assert_eq!(
        config.query(DeviceCertPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs/tedge-certificate.pem")
    );
    assert_eq!(
        config.query(DeviceKeyPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs/tedge-private-key.pem"),
    );

    assert!(config.query_optional(C8yHttpSetting)?.is_none());
    assert!(config.query_optional(C8yMqttSetting)?.is_none());
    assert_eq!(
        config.query(C8yRootCertPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs")
    );

    assert!(config.query_optional(AzureUrlSetting)?.is_none());
    assert_eq!(
        config.query(AzureRootCertPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs")
    );
    assert_eq!(config.query(AzureMapperTimestamp)?, Flag(true));

    assert_eq!(config.query(MqttPortSetting)?, Port(1883));
    assert_eq!(
        config.query(MqttBindAddressSetting)?,
        IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST))
    );
    assert_eq!(
        config.query(FirmwareChildUpdateTimeoutSetting)?,
        Seconds::from(3600)
    );
    assert_eq!(config.query(ServiceTypeSetting)?, "service".to_string());
    assert_eq!(config.query(TmpPathSetting)?, Utf8PathBuf::from("/tmp"));
    assert_eq!(config.query(LogPathSetting)?, Utf8PathBuf::from("/var/log"));
    assert_eq!(config.query(RunPathSetting)?, Utf8PathBuf::from("/run"));
    assert_eq!(
        config.query(DataPathSetting)?,
        Utf8PathBuf::from("/var/tedge")
    );

    Ok(())
}

#[test]
fn test_parse_config_no_config_file() -> Result<(), TEdgeConfigError> {
    let config_location = TEdgeConfigLocation::from_custom_root("/non/existent/path");
    let config = TEdgeConfigRepository::new(config_location).load()?;

    assert!(config.query_optional(DeviceIdSetting).is_err());
    assert_eq!(
        config.query(DeviceCertPathSetting)?,
        Utf8PathBuf::from("/non/existent/path/device-certs/tedge-certificate.pem")
    );
    assert_eq!(
        config.query(DeviceKeyPathSetting)?,
        Utf8PathBuf::from("/non/existent/path/device-certs/tedge-private-key.pem"),
    );

    assert!(config.query_optional(C8yHttpSetting)?.is_none());
    assert!(config.query_optional(C8yMqttSetting)?.is_none());
    assert_eq!(
        config.query(C8yRootCertPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs")
    );

    assert!(config.query_optional(AzureUrlSetting)?.is_none());
    assert_eq!(
        config.query(AzureRootCertPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs")
    );
    assert_eq!(config.query(AzureMapperTimestamp)?, Flag(true));

    assert_eq!(config.query(MqttPortSetting)?, Port(1883));
    assert_eq!(
        config.query(MqttBindAddressSetting)?,
        IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST))
    );
    Ok(())
}

#[test]
fn test_invalid_mqtt_port() -> Result<(), TEdgeConfigError> {
    let toml_conf = r#"
[mqtt.bind]
port = "1883"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(toml_conf)?;
    let toml_path = config_location.tedge_config_file_path().to_string();
    let result = TEdgeConfigRepository::new(config_location).load();

    let expected_err = format!("invalid type: found string \"1883\", expected a nonzero u16 for key \"mqtt.bind.port\" in {toml_path} TOML file");

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
    let result = TEdgeConfigRepository::new(config_location).load();

    match result {
        Err(error @ TEdgeConfigError::Figment(_)) => {
            assert!(
                error.to_string().contains("invalid key"),
                "error: {} does not contain the expected text: invalid key",
                error
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

    let config_defaults = TEdgeConfigDefaults {
        default_c8y_root_cert_path: Utf8PathBuf::from("/etc/ssl/certs"),
        default_azure_root_cert_path: Utf8PathBuf::from("/etc/ssl/certs"),
        ..dummy_tedge_config_defaults()
    };

    let mut config =
        TEdgeConfigRepository::new_with_defaults(config_location, config_defaults).load()?;

    let original_device_key_path = Utf8PathBuf::from("/path/to/key");
    let original_device_cert_path = Utf8PathBuf::from("/path/to/cert");

    assert!(config.query_optional(DeviceIdSetting).is_err());
    assert_eq!(
        config.query(DeviceKeyPathSetting)?,
        original_device_key_path
    );
    assert_eq!(
        config.query(DeviceCertPathSetting)?,
        original_device_cert_path
    );

    let original_c8y_root_cert_path = Utf8PathBuf::from("/path/to/c8y/root/cert");
    let c8y_http_host = config.query(C8yHttpSetting).unwrap();
    let c8y_mqtt_host = config.query(C8yMqttSetting).unwrap();
    assert_eq!(c8y_http_host.port(), Port(443));
    assert_eq!(c8y_mqtt_host.port(), Port(8883));
    assert_eq!(
        config.query(C8yRootCertPathSetting)?,
        original_c8y_root_cert_path
    );

    let original_azure_url = ConnectUrl::try_from("MyAzure.azure-devices.net")?;
    let original_azure_root_cert_path = Utf8PathBuf::from("/path/to/azure/root/cert");
    assert_eq!(config.query(AzureUrlSetting)?, original_azure_url);
    assert_eq!(
        config.query(AzureRootCertPathSetting)?,
        original_azure_root_cert_path
    );
    assert_eq!(config.query(AzureMapperTimestamp)?, Flag(false));

    assert_eq!(config.query(MqttPortSetting)?, Port(1024));

    let updated_c8y_url = ConnectUrl::try_from("other-tenant.cumulocity.com")?;
    #[allow(deprecated)]
    config.update(C8yUrlSetting, updated_c8y_url)?;

    config
        .update(
            C8yHttpSetting,
            HostPort::<HTTPS_PORT>::try_from("http.other-tenant.cumulocity.com:1234".to_string())
                .unwrap(),
        )
        .unwrap();
    assert_eq!(config.query(C8yHttpSetting).unwrap().port(), Port(1234));

    config
        .update(
            C8yMqttSetting,
            HostPort::<MQTT_TLS_PORT>::try_from(
                "http.other-tenant.cumulocity.com:2137".to_string(),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(config.query(C8yMqttSetting).unwrap().port(), Port(2137));

    config.unset(C8yRootCertPathSetting)?;

    let updated_azure_url = ConnectUrl::try_from("OtherAzure.azure-devices.net")?;
    config.update(AzureUrlSetting, updated_azure_url.clone())?;
    config.unset(AzureRootCertPathSetting)?;
    config.unset(AzureMapperTimestamp)?;

    let updated_mqtt_port = Port(2048);
    config.update(MqttPortSetting, updated_mqtt_port)?;

    assert_eq!(
        config.query(DeviceKeyPathSetting)?,
        original_device_key_path
    );
    assert_eq!(
        config.query(DeviceCertPathSetting)?,
        original_device_cert_path
    );

    assert_eq!(
        config.query(C8yRootCertPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs")
    );

    assert_eq!(config.query(AzureUrlSetting)?, updated_azure_url);
    assert_eq!(
        config.query(AzureRootCertPathSetting)?,
        Utf8PathBuf::from("/etc/ssl/certs")
    );
    assert_eq!(config.query(AzureMapperTimestamp)?, Flag(true));

    assert_eq!(config.query(MqttPortSetting)?, updated_mqtt_port);
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

    assert!(config.query_optional(DeviceIdSetting).is_err());
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

    assert!(config.query_optional(DeviceIdSetting).is_err());
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
    config.update(DeviceCertPathSetting, cert_path.try_into().unwrap())?;

    match config.query(DeviceIdSetting) {
        Err(ConfigSettingError::DerivationFailed { key, cause }) => {
            assert_eq!(key, "device.id");
            assert_eq!(cause, "PEM file format error");
        }
        Err(_) => panic!("unexpected error"),
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
    let mut config =
        TEdgeConfigRepository::new_with_defaults(config_location, dummy_tedge_config_defaults())
            .load()?;

    let cert_path: Utf8PathBuf = tempdir.path().join("certificate.pem").try_into().unwrap();
    create_certificate(cert_path.clone(), device_id).expect("fail to create a certificate");
    config.update(DeviceCertPathSetting, cert_path)?;

    assert_eq!(config.query(DeviceIdSetting)?, device_id);

    Ok(())
}

#[test]
fn http_and_mqtt_hosts_serialize_and_deserialize_correctly() {
    let no_ports = r#"
[c8y]
url = "tenant.cumulocity.com"
http = "http.tenant.cumulocity.com"
mqtt = "mqtt.tenant.cumulocity.com"
"#;

    let (_tempdir, config_location) = create_temp_tedge_config(no_ports).unwrap();
    let repository =
        TEdgeConfigRepository::new_with_defaults(config_location, dummy_tedge_config_defaults());
    let config = repository.load().unwrap();

    assert_eq!(config.query(C8yHttpSetting).unwrap().port().0, 443);
    assert_eq!(config.query(C8yMqttSetting).unwrap().port().0, 8883);

    // test updating C8yHttpSetting
    repository
        .update_toml(&|config| {
            let new_c8y_http =
                HostPort::<HTTPS_PORT>::try_from("custom.domain.com:8080".to_string()).unwrap();
            config.update(C8yHttpSetting, new_c8y_http)
        })
        .unwrap();

    let toml_content =
        std::fs::read_to_string(&repository.get_config_location().tedge_config_file_path).unwrap();
    assert!(toml_content.contains("http = \"custom.domain.com:8080\""));
    assert!(toml_content.contains("mqtt = \"mqtt.tenant.cumulocity.com\""));

    // test updating C8yMqttSetting
    repository
        .update_toml(&|config| {
            let new_c8y_mqtt =
                HostPort::<MQTT_TLS_PORT>::try_from("custom.domain.com:1883".to_string()).unwrap();
            config.update(C8yMqttSetting, new_c8y_mqtt)
        })
        .unwrap();

    let toml_content =
        std::fs::read_to_string(&repository.get_config_location().tedge_config_file_path).unwrap();
    assert!(toml_content.contains("http = \"custom.domain.com:8080\""));
    assert!(toml_content.contains("mqtt = \"custom.domain.com:1883\""));
}

fn create_temp_tedge_config(content: &str) -> std::io::Result<(TempTedgeDir, TEdgeConfigLocation)> {
    let dir = TempTedgeDir::new();
    dir.file("tedge.toml").with_raw_content(content);
    let config_location = TEdgeConfigLocation::from_custom_root(dir.path());
    Ok((dir, config_location))
}

fn dummy_tedge_config_defaults() -> TEdgeConfigDefaults {
    TEdgeConfigDefaults {
        default_device_cert_path: Utf8PathBuf::from("/dev/null"),
        default_device_key_path: Utf8PathBuf::from("/dev/null"),
        default_c8y_root_cert_path: Utf8PathBuf::from("/dev/null"),
        default_azure_root_cert_path: Utf8PathBuf::from("/dev/null"),
        default_aws_root_cert_path: Utf8PathBuf::from("/dev/null"),
        default_mapper_timestamp: Flag(true),
        default_mqtt_port: Port(1883),
        default_http_port: Port(8000),
        default_tmp_path: Utf8PathBuf::from("/tmp"),
        default_logs_path: Utf8PathBuf::from("/var/log"),
        default_run_path: Utf8PathBuf::from("/run"),
        default_data_path: Utf8PathBuf::from("/var/tedge"),
        default_device_type: String::from("test"),
        default_mqtt_client_host: "localhost".to_string(),
        default_mqtt_bind_address: IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        default_http_bind_address: IpAddress(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        default_c8y_smartrest_templates: TemplatesSet::default(),
        default_firmware_child_update_timeout: Seconds::from(3600),
        default_service_type: String::from("service"),
        default_lock_files: Flag(true),
    }
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
