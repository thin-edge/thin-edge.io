use super::*;
use crate::tedge_toml::tedge_config::TEdgeConfigReaderAws;
use crate::tedge_toml::tedge_config::TEdgeConfigReaderAz;
use crate::tedge_toml::tedge_config::TEdgeConfigReaderC8y;
use crate::tedge_toml::ReadableKey;
use crate::TEdgeConfig;

/// Trait for creating cloud-specific mapper configuration from tedge.toml cloud sections
///
/// This trait enables backward compatibility by loading the new `MapperConfig<T>` format
/// from the legacy c8y/az/aws configuration sections in tedge.toml.
pub trait FromCloudConfig: Sized {
    /// The corresponding TEdgeConfigReader type for this cloud
    type CloudConfigReader: CloudConfigAccessor + Send + Sync + 'static;

    /// Returns the cloud type for this configuration
    fn load_cloud_mapper_config(
        profile: Option<&str>,
        tedge_config: &TEdgeConfig,
    ) -> Result<MapperConfig<Self>, MapperConfigError>
    where
        Self: SpecialisedCloudConfig;

    /// Create from the cloud-specific configuration reader
    fn from_cloud_config(config: &Self::CloudConfigReader, profile: Option<&str>) -> Self;
}

/// Load a cloud mapper configuration from tedge.toml legacy cloud sections
///
/// This function provides backward compatibility by converting the old c8y/az/aws
/// configuration format into the new generic `MapperConfig<T>` structure.
pub fn load_cloud_mapper_config<T>(
    profile: Option<&str>,
    tedge_config: &TEdgeConfig,
) -> Result<MapperConfig<T>, MapperConfigError>
where
    T: SpecialisedCloudConfig,
{
    T::load_cloud_mapper_config(profile, tedge_config)
}

impl FromCloudConfig for C8yMapperSpecificConfig {
    type CloudConfigReader = TEdgeConfigReaderC8y;

    fn load_cloud_mapper_config(
        profile: Option<&str>,
        tedge_config: &TEdgeConfig,
    ) -> Result<MapperConfig<Self>, MapperConfigError> {
        let c8y_config = tedge_config.c8y.try_get(profile).map_err(|_| {
            MapperConfigError::ConfigRead(format!("C8y profile '{}' not found", profile.unwrap()))
        })?;

        build_mapper_config(c8y_config.clone(), profile)
    }

    fn from_cloud_config(c8y: &Self::CloudConfigReader, profile: Option<&str>) -> Self {
        C8yMapperSpecificConfig {
            auth_method: c8y.auth_method,
            credentials_path: c8y.credentials_path.clone(),
            smartrest: SmartrestConfig {
                templates: c8y.smartrest.templates.clone(),
                use_operation_id: c8y.smartrest.use_operation_id,
                child_device: SmartrestChildDeviceConfig {
                    create_with_device_marker: c8y.smartrest.child_device.create_with_device_marker,
                },
            },
            smartrest1: Smartrest1Config {
                templates: c8y.smartrest1.templates.clone(),
            },
            http: c8y.http.clone(),
            mqtt: c8y.mqtt.clone(),
            proxy: ProxyConfig {
                bind: ProxyBindConfig {
                    address: c8y.proxy.bind.address,
                    port: Keyed {
                        value: c8y.proxy.bind.port,
                        key: ReadableKey::C8yProxyBindPort(profile.map(<_>::to_owned)),
                    },
                },
                client: ProxyClientConfig {
                    host: c8y.proxy.client.host.clone(),
                    port: c8y.proxy.client.port,
                },
                cert_path: c8y.proxy.cert_path.clone(),
                key_path: c8y.proxy.key_path.clone(),
                ca_path: c8y.proxy.ca_path.clone(),
            },
            entity_store: EntityStoreConfig {
                auto_register: c8y.entity_store.auto_register,
                clean_start: c8y.entity_store.clean_start,
            },
            software_management: SoftwareManagementConfig {
                api: c8y.software_management.api,
                with_types: c8y.software_management.with_types,
            },
            operations: OperationsConfig {
                auto_log_upload: c8y.operations.auto_log_upload,
            },
            availability: AvailabilityConfig {
                enable: c8y.availability.enable,
                interval: c8y.availability.interval.clone(),
            },
            enable: EnableConfig {
                log_upload: c8y.enable.log_upload,
                config_snapshot: c8y.enable.config_snapshot,
                config_update: c8y.enable.config_update,
                firmware_update: c8y.enable.firmware_update,
                device_profile: c8y.enable.device_profile,
            },
            mqtt_service: MqttServiceConfig {
                enabled: c8y.mqtt_service.enabled,
                topics: c8y.mqtt_service.topics.clone(),
            },
            bridge: C8yBridgeConfig {
                include: C8yBridgeIncludeConfig {
                    local_cleansession: c8y.bridge.include.local_cleansession,
                },
            },
        }
    }
}

impl FromCloudConfig for AzMapperSpecificConfig {
    type CloudConfigReader = TEdgeConfigReaderAz;

    fn load_cloud_mapper_config(
        profile: Option<&str>,
        tedge_config: &TEdgeConfig,
    ) -> Result<MapperConfig<Self>, MapperConfigError> {
        let az_config = tedge_config.az.try_get(profile).map_err(|_| {
            MapperConfigError::ConfigRead(format!("Azure profile '{}' not found", profile.unwrap()))
        })?;

        build_mapper_config(az_config.clone(), profile)
    }

    fn from_cloud_config(az: &Self::CloudConfigReader, _profile: Option<&str>) -> Self {
        AzMapperSpecificConfig {
            mapper: AzCloudMapperConfig {
                timestamp: az.mapper.timestamp,
                timestamp_format: az.mapper.timestamp_format,
            },
        }
    }
}

impl FromCloudConfig for AwsMapperSpecificConfig {
    type CloudConfigReader = TEdgeConfigReaderAws;

    fn load_cloud_mapper_config(
        profile: Option<&str>,
        tedge_config: &TEdgeConfig,
    ) -> Result<MapperConfig<Self>, MapperConfigError> {
        let aws_config = tedge_config.aws.try_get(profile).map_err(|_| {
            MapperConfigError::ConfigRead(format!("AWS profile '{}' not found", profile.unwrap()))
        })?;

        build_mapper_config(aws_config.clone(), profile)
    }

    fn from_cloud_config(aws: &Self::CloudConfigReader, _profile: Option<&str>) -> Self {
        AwsMapperSpecificConfig {
            mapper: AwsCloudMapperConfig {
                timestamp: aws.mapper.timestamp,
                timestamp_format: aws.mapper.timestamp_format,
            },
        }
    }
}

/// Generic helper to build MapperConfig from any cloud config reader
pub fn build_mapper_config<T>(
    cloud_config: T::CloudConfigReader,
    profile: Option<&str>,
) -> Result<MapperConfig<T>, MapperConfigError>
where
    T: SpecialisedCloudConfig,
{
    let url = cloud_config.url().clone();

    let device = DeviceConfig {
        id: to_optional_config(
            cloud_config.device_id().ok().map(|s| s.to_string()),
            &cloud_config.device_id_key(profile),
        ),
        key_path: cloud_config.device_key_path().to_owned(),
        cert_path: cloud_config.device_cert_path().to_owned(),
        csr_path: cloud_config.device_csr_path().to_owned(),
        key_uri: cloud_config.device_key_uri(),
        key_pin: cloud_config.device_key_pin(),
    };

    let bridge = BridgeConfig {
        topic_prefix: cloud_config.bridge_topic_prefix(profile),
        keepalive_interval: cloud_config.bridge_keepalive_interval().clone(),
    };

    let topics = cloud_config.topics().clone();

    let root_cert_path = cloud_config.root_cert_path(profile);

    let max_payload_size = cloud_config.max_payload_size();

    let cloud_specific = T::from_cloud_config(&cloud_config, profile);

    Ok(MapperConfig {
        url,
        root_cert_path,
        device,
        topics,
        bridge,
        mapper: CommonMapperConfig {
            mqtt: MqttConfig { max_payload_size },
        },
        cloud_specific,
    })
}

/// Trait to abstract over different cloud config readers
///
/// This trait provides a uniform interface for accessing common fields
/// from different cloud configuration readers (C8y, Az, Aws).
pub trait CloudConfigAccessor {
    fn url(&self) -> &OptionalConfig<ConnectUrl>;
    fn device_id(&self) -> Result<String, ReadError>;
    fn device_id_key(&self, profile: Option<&str>) -> ReadableKey;
    fn device_key_path(&self) -> &AbsolutePath;
    fn device_cert_path(&self) -> &AbsolutePath;
    fn device_csr_path(&self) -> &AbsolutePath;
    fn device_key_uri(&self) -> Option<Arc<str>>;
    fn device_key_pin(&self) -> Option<Arc<str>>;
    fn bridge_topic_prefix(&self, profile: Option<&str>) -> Keyed<TopicPrefix>;
    fn bridge_keepalive_interval(&self) -> &SecondsOrHumanTime;
    fn topics(&self) -> &TemplatesSet;
    fn root_cert_path(&self, profile: Option<&str>) -> Keyed<AbsolutePath>;
    fn max_payload_size(&self) -> MqttPayloadLimit;
}

impl CloudConfigAccessor for TEdgeConfigReaderC8y {
    fn url(&self) -> &OptionalConfig<ConnectUrl> {
        &self.url
    }

    fn device_id_key(&self, profile: Option<&str>) -> ReadableKey {
        ReadableKey::C8yDeviceId(profile.map(<_>::to_owned))
    }

    fn device_id(&self) -> Result<String, ReadError> {
        Ok(self.device.id()?.clone())
    }

    fn device_key_path(&self) -> &AbsolutePath {
        &self.device.key_path
    }

    fn device_cert_path(&self) -> &AbsolutePath {
        &self.device.cert_path
    }

    fn device_csr_path(&self) -> &AbsolutePath {
        &self.device.csr_path
    }

    fn device_key_uri(&self) -> Option<Arc<str>> {
        self.device.key_uri.or_none().cloned()
    }

    fn device_key_pin(&self) -> Option<Arc<str>> {
        self.device.key_pin.or_none().cloned()
    }

    fn bridge_topic_prefix(&self, profile: Option<&str>) -> Keyed<TopicPrefix> {
        Keyed::new(
            self.bridge.topic_prefix.clone(),
            ReadableKey::C8yBridgeTopicPrefix(profile.map(<_>::to_owned)),
        )
    }

    fn bridge_keepalive_interval(&self) -> &SecondsOrHumanTime {
        &self.bridge.keepalive_interval
    }

    fn topics(&self) -> &TemplatesSet {
        &self.topics
    }

    fn root_cert_path(&self, profile: Option<&str>) -> Keyed<AbsolutePath> {
        Keyed::new(
            self.root_cert_path.clone(),
            ReadableKey::C8yRootCertPath(profile.map(<_>::to_owned)),
        )
    }

    fn max_payload_size(&self) -> MqttPayloadLimit {
        self.mapper.mqtt.max_payload_size
    }
}

impl CloudConfigAccessor for TEdgeConfigReaderAz {
    fn url(&self) -> &OptionalConfig<ConnectUrl> {
        &self.url
    }

    fn device_id_key(&self, profile: Option<&str>) -> ReadableKey {
        ReadableKey::AzDeviceId(profile.map(<_>::to_owned))
    }

    fn device_id(&self) -> Result<String, ReadError> {
        Ok(self.device.id()?.clone())
    }

    fn device_key_path(&self) -> &AbsolutePath {
        &self.device.key_path
    }

    fn device_cert_path(&self) -> &AbsolutePath {
        &self.device.cert_path
    }

    fn device_csr_path(&self) -> &AbsolutePath {
        &self.device.csr_path
    }

    fn device_key_uri(&self) -> Option<Arc<str>> {
        self.device.key_uri.or_none().cloned()
    }

    fn device_key_pin(&self) -> Option<Arc<str>> {
        self.device.key_pin.or_none().cloned()
    }

    fn bridge_topic_prefix(&self, profile: Option<&str>) -> Keyed<TopicPrefix> {
        Keyed::new(
            self.bridge.topic_prefix.clone(),
            ReadableKey::AzBridgeTopicPrefix(profile.map(<_>::to_owned)),
        )
    }

    fn bridge_keepalive_interval(&self) -> &SecondsOrHumanTime {
        &self.bridge.keepalive_interval
    }

    fn topics(&self) -> &TemplatesSet {
        &self.topics
    }

    fn root_cert_path(&self, profile: Option<&str>) -> Keyed<AbsolutePath> {
        Keyed::new(
            self.root_cert_path.clone(),
            ReadableKey::AzRootCertPath(profile.map(<_>::to_owned)),
        )
    }

    fn max_payload_size(&self) -> MqttPayloadLimit {
        self.mapper.mqtt.max_payload_size
    }
}

impl CloudConfigAccessor for TEdgeConfigReaderAws {
    fn url(&self) -> &OptionalConfig<ConnectUrl> {
        &self.url
    }

    fn device_id_key(&self, profile: Option<&str>) -> ReadableKey {
        ReadableKey::AwsDeviceId(profile.map(<_>::to_owned))
    }

    fn device_id(&self) -> Result<String, ReadError> {
        Ok(self.device.id()?.clone())
    }

    fn device_key_path(&self) -> &AbsolutePath {
        &self.device.key_path
    }

    fn device_cert_path(&self) -> &AbsolutePath {
        &self.device.cert_path
    }

    fn device_csr_path(&self) -> &AbsolutePath {
        &self.device.csr_path
    }

    fn device_key_uri(&self) -> Option<Arc<str>> {
        self.device.key_uri.or_none().cloned()
    }

    fn device_key_pin(&self) -> Option<Arc<str>> {
        self.device.key_pin.or_none().cloned()
    }

    fn bridge_topic_prefix(&self, profile: Option<&str>) -> Keyed<TopicPrefix> {
        Keyed::new(
            self.bridge.topic_prefix.clone(),
            ReadableKey::AwsBridgeTopicPrefix(profile.map(<_>::to_owned)),
        )
    }

    fn bridge_keepalive_interval(&self) -> &SecondsOrHumanTime {
        &self.bridge.keepalive_interval
    }

    fn topics(&self) -> &TemplatesSet {
        &self.topics
    }

    fn root_cert_path(&self, profile: Option<&str>) -> Keyed<AbsolutePath> {
        Keyed::new(
            self.root_cert_path.clone(),
            ReadableKey::AwsRootCertPath(profile.map(<_>::to_owned)),
        )
    }

    fn max_payload_size(&self) -> MqttPayloadLimit {
        self.mapper.mqtt.max_payload_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TEdgeConfigLocation;

    #[test]
    fn test_load_c8y_config_default_profile() {
        let tedge_toml = r#"
            [c8y]
            url = "tenant.cumulocity.com"
        "#;

        let tedge_config = TEdgeConfig::from_dto(
            toml::from_str(tedge_toml).unwrap(),
            TEdgeConfigLocation::from_custom_root("/tmp/tedge"),
        );

        let config: C8yMapperConfig = load_cloud_mapper_config(None, &tedge_config).unwrap();

        assert_eq!(config.cloud_specific.auth_method, AuthMethod::Certificate);
        assert!(config.cloud_specific.entity_store.auto_register);
        assert!(config.cloud_specific.entity_store.clean_start);

        assert_eq!(
            config.http().or_none().unwrap().to_string(),
            "tenant.cumulocity.com:443"
        );
        assert_eq!(
            config.mqtt().or_none().unwrap().to_string(),
            "tenant.cumulocity.com:8883"
        );
    }

    #[test]
    fn test_load_c8y_config_named_profile() {
        let tedge_toml = r#"
            [c8y.profiles.test-tenant]
            url = "test.cumulocity.com"
        "#;

        let tedge_config = TEdgeConfig::from_dto(
            toml::from_str(tedge_toml).unwrap(),
            TEdgeConfigLocation::from_custom_root("/tmp/tedge"),
        );

        let config: C8yMapperConfig =
            load_cloud_mapper_config(Some("test-tenant"), &tedge_config).unwrap();

        assert_eq!(
            config.http().or_none().unwrap().host().to_string(),
            "test.cumulocity.com"
        );
    }

    #[test]
    fn test_load_az_config() {
        let tedge_toml = r#"
            [az]
            url = "mydevice.azure-devices.net"
        "#;

        let tedge_config = TEdgeConfig::from_dto(
            toml::from_str(tedge_toml).unwrap(),
            TEdgeConfigLocation::from_custom_root("/tmp/tedge"),
        );

        let config: AzMapperConfig = load_cloud_mapper_config(None, &tedge_config).unwrap();

        assert_eq!(
            config.url().or_none().unwrap().as_str(),
            "mydevice.azure-devices.net"
        );
        assert!(config.cloud_specific.mapper.timestamp);
        assert_eq!(
            config.cloud_specific.mapper.timestamp_format,
            TimeFormat::Unix
        );
    }

    #[test]
    fn test_load_aws_config() {
        let tedge_toml = r#"
            [aws]
            url = "mydevice.amazonaws.com"
        "#;

        let tedge_config = TEdgeConfig::from_dto(
            toml::from_str(tedge_toml).unwrap(),
            TEdgeConfigLocation::from_custom_root("/tmp/tedge"),
        );

        let config: AwsMapperConfig = load_cloud_mapper_config(None, &tedge_config).unwrap();

        assert_eq!(
            config.url().or_none().unwrap().as_str(),
            "mydevice.amazonaws.com"
        );
        assert!(config.cloud_specific.mapper.timestamp);
        assert_eq!(
            config.cloud_specific.mapper.timestamp_format,
            TimeFormat::Unix
        );
    }

    #[test]
    fn test_missing_url_preserves_key_name() {
        let tedge_toml = r#"
            [c8y]
            # URL intentionally not set
        "#;

        let tedge_config = TEdgeConfig::from_dto(
            toml::from_str(tedge_toml).unwrap(),
            TEdgeConfigLocation::from_custom_root("/tmp/tedge"),
        );

        let result: C8yMapperConfig = load_cloud_mapper_config(None, &tedge_config).unwrap();

        assert_eq!(result.url.key(), "c8y.url");
    }

    #[test]
    fn test_profile_not_found_returns_error() {
        let tedge_toml = r#"
            [c8y]
            url = "tenant.cumulocity.com"
        "#;

        let tedge_config = TEdgeConfig::from_dto(
            toml::from_str(tedge_toml).unwrap(),
            TEdgeConfigLocation::from_custom_root("/tmp/tedge"),
        );

        let result: Result<C8yMapperConfig, _> =
            load_cloud_mapper_config(Some("nonexistent"), &tedge_config);

        assert!(result.is_err());
    }

    #[test]
    fn test_c8y_specific_fields_mapped_correctly() {
        let tedge_toml = r#"
            [c8y]
            url = "tenant.cumulocity.com"
            auth_method = "basic"

            [c8y.smartrest]
            use_operation_id = false

            [c8y.entity_store]
            auto_register = false

            [c8y.enable]
            log_upload = false
        "#;

        let tedge_config = TEdgeConfig::from_dto(
            toml::from_str(tedge_toml).unwrap(),
            TEdgeConfigLocation::from_custom_root("/tmp/tedge"),
        );

        let config: C8yMapperConfig = load_cloud_mapper_config(None, &tedge_config).unwrap();

        assert_eq!(config.cloud_specific.auth_method, AuthMethod::Basic);
        assert!(!config.cloud_specific.smartrest.use_operation_id);
        assert!(!config.cloud_specific.entity_store.auto_register);
        assert!(!config.cloud_specific.enable.log_upload);
    }

    #[test]
    fn test_c8y_proxy_port_inheritance() {
        let tedge_toml = r#"
            [c8y]
            url = "tenant.cumulocity.com"

            [c8y.proxy.bind]
            port = 9001
        "#;

        let tedge_config = TEdgeConfig::from_dto(
            toml::from_str(tedge_toml).unwrap(),
            TEdgeConfigLocation::from_custom_root("/tmp/tedge"),
        );

        let config: C8yMapperConfig = load_cloud_mapper_config(None, &tedge_config).unwrap();

        assert_eq!(config.cloud_specific.proxy.bind.port, 9001);
        assert_eq!(config.cloud_specific.proxy.client.port, 9001);
    }

    #[test]
    fn test_device_fields_populated_from_tedge_config() {
        let tedge_toml = r#"
            [device]
            id = "my-device-123"

            [c8y.profiles.new]
            url = "tenant.cumulocity.com"
        "#;

        let tedge_config = TEdgeConfig::from_dto(
            toml::from_str(tedge_toml).unwrap(),
            TEdgeConfigLocation::from_custom_root("/tmp/tedge"),
        );

        let config: C8yMapperConfig = load_cloud_mapper_config(Some("new"), &tedge_config).unwrap();

        // Device ID should come from tedge_config
        assert_eq!(config.device.id.or_none().unwrap(), "my-device-123");
        // The key should be set to the specific device_id key for the cloud
        // This is for the error message the mapper produces when trying to
        // connect the same device id to the same url
        assert_eq!(config.device.id.key(), "c8y.profiles.new.device.id");

        // Other device fields should have defaults from tedge_config
        assert!(config.device.key_path.as_str().contains("tedge"));
        assert!(config.device.cert_path.as_str().contains("tedge"));
    }

    #[test]
    fn empty_c8y_proxy_cert_path_preserves_original_key() {
        let tedge_toml = r#"
            [device]
            id = "my-device-123"

            [c8y]
            url = "tenant.cumulocity.com"
        "#;

        let tedge_config = TEdgeConfig::from_dto(
            toml::from_str(tedge_toml).unwrap(),
            TEdgeConfigLocation::from_custom_root("/tmp/tedge"),
        );

        let config: C8yMapperConfig = load_cloud_mapper_config(None, &tedge_config).unwrap();

        assert_eq!(
            config.cloud_specific.proxy.cert_path.key(),
            "c8y.proxy.cert_path"
        )
    }
}
