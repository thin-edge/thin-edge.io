pub mod compat;

use crate::models::CloudType;
use crate::tedge_toml::tedge_config::cert_error_into_config_error;
use crate::tedge_toml::MapperConfigLocation;
use crate::tedge_toml::ReadableKey;
use crate::tedge_toml::TEdgeConfigDtoAws;
use crate::tedge_toml::TEdgeConfigDtoAz;
use crate::tedge_toml::TEdgeConfigDtoC8y;
use crate::TEdgeConfig;
use crate::TEdgeConfigDto;
use crate::TEdgeConfigReader;

use super::super::models::auth_method::AuthMethod;
use super::super::models::AbsolutePath;
use super::super::models::AutoFlag;
use super::super::models::AutoLogUpload;
use super::super::models::ConnectUrl;
use super::super::models::HostPort;
use super::super::models::MqttPayloadLimit;
use super::super::models::SecondsOrHumanTime;
use super::super::models::SoftwareManagementApiFlag;
use super::super::models::TemplatesSet;
use super::super::models::TimeFormat;
use super::super::models::TopicPrefix;
use super::super::models::HTTPS_PORT;
use super::super::models::MQTT_TLS_PORT;
use super::MultiError;
use super::OptionalConfig;
use super::ReadError;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use certificate::PemCertificate;
use serde::de::DeserializeOwned;
use std::borrow::Cow;
use std::fmt::Display;
use std::net::IpAddr;
use std::ops::Deref;
use std::sync::Arc;
use tedge_config_macros::MultiDto;
use tedge_config_macros::ProfileName;

pub use compat::load_cloud_mapper_config;
pub use compat::FromCloudConfig;

/// Device-specific configuration fields shared across all cloud types
pub struct DeviceConfig {
    /// Device identifier (optional, will be derived from certificate if not set)
    id: OptionalConfig<String>,

    /// Path to the device's private key
    pub key_path: AbsolutePath,

    /// Path to the device's certificate
    pub cert_path: AbsolutePath,

    /// Path to the device's certificate signing request
    pub csr_path: AbsolutePath,

    /// PKCS#11 URI of the private key (optional)
    pub key_uri: Option<Arc<str>>,

    /// User PIN for PKCS#11 token (optional)
    pub key_pin: Option<Arc<str>>,
}

impl DeviceConfig {
    /// Get the device ID, deriving it from the certificate if not explicitly set
    ///
    /// This will parse the certificate and extract the Common Name if device.id is not set.
    /// Note: This method allocates a String when deriving from certificate.
    pub fn id(&self) -> Result<String, MapperConfigError> {
        if let OptionalConfig::Present { value: ref id, .. } = self.id {
            Ok(id.clone())
        } else {
            // Try to derive from certificate
            device_id_from_cert(&self.cert_path)
        }
    }

    pub fn id_key(&self) -> &Cow<'static, str> {
        self.id.key()
    }
}

/// Bridge configuration fields shared across all cloud types
pub struct BridgeConfig {
    /// The topic prefix for the bridge MQTT topic
    pub topic_prefix: Keyed<TopicPrefix>,

    /// The amount of time after which the bridge should send a ping
    pub keepalive_interval: SecondsOrHumanTime,
}

pub struct C8yBridgeConfig {
    pub include: C8yBridgeIncludeConfig,
}

/// Trait linking cloud-specific config to its DTO and config reader
pub trait SpecialisedCloudConfig:
    Sized + ExpectedCloudType + FromCloudConfig + Send + Sync + 'static
{
    type CloudDto: HasPath + Default + DeserializeOwned + Send + Sync + 'static;

    fn into_config_reader(
        dto: Self::CloudDto,
        base_config: &TEdgeConfig,
        profile: Option<&str>,
    ) -> Self::CloudConfigReader;
}

pub trait HasPath {
    fn set_path(&mut self, path: Utf8PathBuf);
    fn get_path(&self) -> Option<&Utf8Path>;
}

impl HasPath for TEdgeConfigDtoC8y {
    fn set_path(&mut self, path: Utf8PathBuf) {
        self.read_from = Some(path)
    }

    fn get_path(&self) -> Option<&Utf8Path> {
        self.read_from.as_deref()
    }
}

impl HasPath for TEdgeConfigDtoAz {
    fn set_path(&mut self, path: Utf8PathBuf) {
        self.read_from = Some(path)
    }

    fn get_path(&self) -> Option<&Utf8Path> {
        self.read_from.as_deref()
    }
}

impl HasPath for TEdgeConfigDtoAws {
    fn set_path(&mut self, path: Utf8PathBuf) {
        self.read_from = Some(path)
    }

    fn get_path(&self) -> Option<&Utf8Path> {
        self.read_from.as_deref()
    }
}

/// Base mapper configuration with common fields and cloud-specific fields via generics
pub struct MapperConfig<T: SpecialisedCloudConfig> {
    pub tedge_config_reader: T::CloudConfigReader,

    /// Endpoint URL of the cloud tenant
    url: OptionalConfig<ConnectUrl>,

    /// Path where cloud root certificate(s) are stored
    pub root_cert_path: Keyed<AbsolutePath>,

    /// Device-specific configuration
    pub device: DeviceConfig,

    /// Set of MQTT topics the mapper should subscribe to
    pub topics: TemplatesSet,

    /// Bridge configuration
    pub bridge: BridgeConfig,

    pub mapper: MapperMapperConfig,

    /// Cloud-specific configuration fields
    pub cloud_specific: T,
}

/// AWS cloud-specific mapper configuration
pub struct AwsCloudMapperConfig {
    /// Whether to add timestamps to messages
    pub timestamp: bool,

    /// The timestamp format to use
    pub timestamp_format: TimeFormat,
}

/// Azure cloud-specific mapper configuration
pub struct AzCloudMapperConfig {
    /// Whether to add timestamps to messages
    pub timestamp: bool,

    /// The timestamp format to use
    pub timestamp_format: TimeFormat,
}

pub struct MapperMapperConfig {
    pub mqtt: MqttConfig,
}

pub struct MqttConfig {
    /// Maximum MQTT payload size
    pub max_payload_size: MqttPayloadLimit,
}

/// SmartREST configuration for Cumulocity
pub struct SmartrestConfig {
    /// Set of SmartREST template IDs the device should subscribe to
    pub templates: TemplatesSet,

    /// Switch using 501-503 or 504-506 SmartREST messages for operation status update
    pub use_operation_id: bool,

    /// SmartREST child device configuration
    pub child_device: SmartrestChildDeviceConfig,
}

pub struct Smartrest1Config {
    /// Set of SmartREST 1 template IDs the device should subscribe to
    pub templates: TemplatesSet,
}

/// Child device SmartREST configuration
pub struct SmartrestChildDeviceConfig {
    /// Attach the c8y_IsDevice fragment to child devices on creation
    pub create_with_device_marker: bool,
}

/// Proxy bind configuration
pub struct ProxyBindConfig {
    /// The IP address local proxy binds to
    pub address: IpAddr,

    /// The port local proxy binds to
    pub port: Keyed<u16>,
}

#[derive(Debug)]
pub struct Keyed<T> {
    value: T,
    key: ReadableKey,
}

impl<T> Keyed<T> {
    fn new(value: T, key: ReadableKey) -> Self {
        Self { value, key }
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn key(&self) -> &ReadableKey {
        &self.key
    }
}

impl<T: Display> Display for Keyed<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value().fmt(f)
    }
}

impl<T> Deref for Keyed<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value()
    }
}

impl<T: PartialEq> PartialEq<T> for Keyed<T> {
    fn eq(&self, other: &T) -> bool {
        self.value() == other
    }
}

pub struct ProxyClientConfig {
    /// The address of the host on which the proxy is running
    pub host: Arc<str>,

    /// The port number on which the proxy is running
    pub port: u16,
}

/// HTTP proxy configuration for Cumulocity
pub struct ProxyConfig {
    /// Proxy bind configuration
    pub bind: ProxyBindConfig,

    /// Proxy client configuration
    pub client: ProxyClientConfig,

    /// Server certificate path for the proxy
    pub cert_path: OptionalConfig<AbsolutePath>,

    /// Server private key path for the proxy
    pub key_path: OptionalConfig<AbsolutePath>,

    /// CA certificates path for the proxy
    pub ca_path: OptionalConfig<AbsolutePath>,
}

/// Entity store configuration
pub struct EntityStoreConfig {
    /// Enable auto registration feature
    pub auto_register: bool,

    /// On a clean start, resend the whole device state to the cloud
    pub clean_start: bool,
}

/// Software management configuration
pub struct SoftwareManagementConfig {
    /// Software management API to use (legacy or advanced)
    pub api: SoftwareManagementApiFlag,

    /// Enable publishing c8y_SupportedSoftwareTypes fragment
    pub with_types: bool,
}

/// Operations configuration
pub struct OperationsConfig {
    /// Auto-upload the operation log once it finishes
    pub auto_log_upload: AutoLogUpload,
}

/// Availability/heartbeat configuration for Cumulocity
pub struct AvailabilityConfig {
    /// Enable sending heartbeat to Cumulocity periodically
    pub enable: bool,

    /// Heartbeat interval to be sent to Cumulocity
    pub interval: SecondsOrHumanTime,
}

/// Feature enable/disable flags
pub struct EnableConfig {
    /// Enable log_upload feature
    pub log_upload: bool,

    /// Enable config_snapshot feature
    pub config_snapshot: bool,

    /// Enable config_update feature
    pub config_update: bool,

    /// Enable firmware_update feature
    pub firmware_update: bool,

    /// Enable device_profile feature
    pub device_profile: bool,
}

/// Bridge include configuration
pub struct C8yBridgeIncludeConfig {
    /// Set the bridge local clean session flag
    pub local_cleansession: AutoFlag,
}

/// MQTT service configuration for Cumulocity
pub struct MqttServiceConfig {
    /// Whether to connect to the MQTT service endpoint or not
    pub enabled: bool,

    /// Set of MQTT topics for the MQTT service endpoint
    pub topics: TemplatesSet,
}

/// Cumulocity-specific mapper configuration fields
pub struct C8yMapperSpecificConfig {
    /// Authentication method (certificate, basic, or auto)
    pub auth_method: AuthMethod,

    /// Path to credentials file for basic auth
    pub credentials_path: AbsolutePath,

    /// SmartREST configuration
    pub smartrest: SmartrestConfig,

    /// SmartREST1 configuration
    pub smartrest1: Smartrest1Config,

    /// HTTP endpoint for Cumulocity
    pub http: OptionalConfig<HostPort<HTTPS_PORT>>,

    /// MQTT endpoint for Cumulocity
    pub mqtt: OptionalConfig<HostPort<MQTT_TLS_PORT>>,

    /// HTTP proxy configuration
    pub proxy: ProxyConfig,

    /// Entity store configuration
    pub entity_store: EntityStoreConfig,

    /// Software management configuration
    pub software_management: SoftwareManagementConfig,

    /// Operations configuration
    pub operations: OperationsConfig,

    /// Availability/heartbeat configuration
    pub availability: AvailabilityConfig,

    /// Feature enable/disable flags
    pub enable: EnableConfig,

    /// MQTT service configuration
    pub mqtt_service: MqttServiceConfig,

    pub bridge: C8yBridgeConfig,
}

/// Azure IoT-specific mapper configuration fields
pub struct AzMapperSpecificConfig {
    pub mapper: AzCloudMapperConfig,
}

/// AWS IoT-specific mapper configuration fields
pub struct AwsMapperSpecificConfig {
    pub mapper: AwsCloudMapperConfig,
}

/// CloudConfig implementation for C8y
impl SpecialisedCloudConfig for C8yMapperSpecificConfig {
    type CloudDto = TEdgeConfigDtoC8y;

    fn into_config_reader(
        dto: Self::CloudDto,
        base_config: &TEdgeConfig,
        profile: Option<&str>,
    ) -> Self::CloudConfigReader {
        let mut multi_dto = MultiDto::default();
        match profile {
            Some(profile) => {
                multi_dto.profiles.insert(profile.parse().unwrap(), dto);
            }
            None => multi_dto.non_profile = dto,
        };
        let mut reader = TEdgeConfigReader::from_dto(
            &TEdgeConfigDto {
                c8y: multi_dto,
                ..base_config.dto.clone()
            },
            &base_config.location,
        );
        match profile {
            None => reader.c8y.non_profile,
            Some(profile) => reader
                .c8y
                .profiles
                .remove(&profile.parse::<ProfileName>().unwrap())
                .unwrap(),
        }
    }
}

/// CloudConfig implementation for Azure
impl SpecialisedCloudConfig for AzMapperSpecificConfig {
    type CloudDto = TEdgeConfigDtoAz;

    fn into_config_reader(
        dto: Self::CloudDto,
        base_config: &TEdgeConfig,
        profile: Option<&str>,
    ) -> Self::CloudConfigReader {
        let mut multi_dto = MultiDto::default();
        match profile {
            Some(profile) => {
                multi_dto.profiles.insert(profile.parse().unwrap(), dto);
            }
            None => multi_dto.non_profile = dto,
        };
        let mut reader = TEdgeConfigReader::from_dto(
            &TEdgeConfigDto {
                az: multi_dto,
                ..base_config.dto.clone()
            },
            &base_config.location,
        );
        match profile {
            None => reader.az.non_profile,
            Some(profile) => reader
                .az
                .profiles
                .remove(&profile.parse::<ProfileName>().unwrap())
                .unwrap(),
        }
    }
}

/// CloudConfig implementation for AWS
impl SpecialisedCloudConfig for AwsMapperSpecificConfig {
    type CloudDto = TEdgeConfigDtoAws;

    fn into_config_reader(
        dto: Self::CloudDto,
        base_config: &TEdgeConfig,
        profile: Option<&str>,
    ) -> Self::CloudConfigReader {
        let mut multi_dto = MultiDto::default();
        match profile {
            Some(profile) => {
                multi_dto.profiles.insert(profile.parse().unwrap(), dto);
            }
            None => multi_dto.non_profile = dto,
        };
        // TODO take TEdgeConfigLocation
        let mut reader = TEdgeConfigReader::from_dto(
            &TEdgeConfigDto {
                aws: multi_dto,
                ..base_config.dto.clone()
            },
            &base_config.location,
        );
        match profile {
            None => reader.aws.non_profile,
            Some(profile) => reader
                .aws
                .profiles
                .remove(&profile.parse::<ProfileName>().unwrap())
                .unwrap(),
        }
    }
}

/// Type alias for Cumulocity mapper configuration
pub type C8yMapperConfig = MapperConfig<C8yMapperSpecificConfig>;

/// Type alias for Azure IoT mapper configuration
pub type AzMapperConfig = MapperConfig<AzMapperSpecificConfig>;

/// Type alias for AWS IoT mapper configuration
pub type AwsMapperConfig = MapperConfig<AwsMapperSpecificConfig>;

/// Error type for mapper configuration loading
#[derive(Debug, thiserror::Error)]
pub enum MapperConfigError {
    /// Failed to read the configuration file
    #[error("Failed to read mapper configuration file: {0}")]
    FileRead(#[from] std::io::Error),

    /// Failed to parse TOML configuration
    #[error("Failed to parse mapper configuration: {0}")]
    TomlParse(#[from] toml::de::Error),

    /// Failed to read from tedge config
    #[error("Failed to read from tedge config: {0}")]
    ConfigRead(String),
}

impl From<ReadError> for MapperConfigError {
    fn from(err: ReadError) -> Self {
        MapperConfigError::ConfigRead(err.to_string())
    }
}

impl From<MultiError> for MapperConfigError {
    fn from(err: MultiError) -> Self {
        MapperConfigError::ConfigRead(err.to_string())
    }
}

/// Load and populate a mapper configuration from an external TOML file
///
/// This function reads a mapper configuration file and applies defaults from
/// the root tedge configuration for any missing common fields (device, bridge, etc.).
///
/// # Arguments
/// * `config_path` - Path to the external mapper configuration TOML file
/// * `tedge_config` - Root tedge configuration reader for default values
///
/// # Returns
/// * `Ok(MapperConfig<T>)` - Fully populated mapper configuration
/// * `Err(MapperConfigError)` - If file cannot be read, parsed, or required fields are missing
/// ```
pub(crate) async fn load_mapper_config<T>(
    config_path: &AbsolutePath,
    tedge_config: &TEdgeConfig,
    profile: Option<&str>,
) -> Result<MapperConfig<T>, MapperConfigError>
where
    T: SpecialisedCloudConfig,
{
    let toml_content = tokio::fs::read_to_string(config_path.as_std_path()).await?;
    load_mapper_config_from_string(&toml_content, tedge_config, profile)
}

fn load_mapper_config_from_string<T>(
    toml_content: &str,
    tedge_config: &TEdgeConfig,
    profile: Option<&str>,
) -> Result<MapperConfig<T>, MapperConfigError>
where
    T: SpecialisedCloudConfig,
{
    let cloud_dto: T::CloudDto = toml::from_str(toml_content)?;
    let cloud_reader = T::into_config_reader(cloud_dto, tedge_config, profile);
    compat::build_mapper_config(cloud_reader, profile)
}

pub trait ExpectedCloudType {
    fn expected_cloud_type() -> CloudType;
}

impl ExpectedCloudType for C8yMapperSpecificConfig {
    fn expected_cloud_type() -> CloudType {
        CloudType::C8y
    }
}

impl ExpectedCloudType for AzMapperSpecificConfig {
    fn expected_cloud_type() -> CloudType {
        CloudType::Az
    }
}

impl ExpectedCloudType for AwsMapperSpecificConfig {
    fn expected_cloud_type() -> CloudType {
        CloudType::Aws
    }
}

pub trait HasUrl {
    // The configured URL field, used to check whether profiles are
    fn configured_url(&self) -> &OptionalConfig<ConnectUrl>;
}

impl<T: SpecialisedCloudConfig> HasUrl for MapperConfig<T> {
    fn configured_url(&self) -> &OptionalConfig<ConnectUrl> {
        &self.url
    }
}

fn to_optional_config<T>(field: Option<T>, key: &ReadableKey) -> OptionalConfig<T> {
    match field {
        Some(value) => OptionalConfig::Present {
            value,
            key: key.to_cow_str(),
        },
        None => OptionalConfig::Empty(key.to_cow_str()),
    }
}

/// Helper function to extract device ID from certificate
fn device_id_from_cert(cert_path: &Utf8Path) -> Result<String, MapperConfigError> {
    let pem = PemCertificate::from_pem_file(cert_path)
        .map_err(|err| cert_error_into_config_error(ReadableKey::DeviceId.to_cow_str(), err))?;

    let device_id = pem.subject_common_name().map_err(|err| {
        MapperConfigError::ConfigRead(format!(
            "Failed to extract device ID from certificate {cert_path}: {err}"
        ))
    })?;

    Ok(device_id)
}

// Allow access to url directly for az and aws, but require c8y dependent crates
// to access the url through mqtt/http variables
impl MapperConfig<AzMapperSpecificConfig> {
    /// Get the cloud URL for Azure
    pub fn url(&self) -> &OptionalConfig<ConnectUrl> {
        &self.url
    }
}

impl MapperConfig<AwsMapperSpecificConfig> {
    /// Get the cloud URL for AWS
    pub fn url(&self) -> &OptionalConfig<ConnectUrl> {
        &self.url
    }
}

impl MapperConfig<C8yMapperSpecificConfig> {
    /// Get the MQTT endpoint for Cumulocity
    pub fn mqtt(&self) -> &OptionalConfig<HostPort<MQTT_TLS_PORT>> {
        &self.cloud_specific.mqtt
    }

    /// Get the HTTP endpoint for Cumulocity
    pub fn http(&self) -> &OptionalConfig<HostPort<HTTPS_PORT>> {
        &self.cloud_specific.http
    }
}

#[cfg(test)]
mod tests {
    use crate::TEdgeConfigDto;
    use crate::TEdgeConfigLocation;

    use super::*;

    #[test]
    fn empty_file_deserializes_with_all_defaults() {
        let config = deserialize_from_str::<C8yMapperSpecificConfig>("").unwrap();

        // Verify all defaults are applied
        assert_eq!(config.cloud_specific.auth_method, AuthMethod::Certificate);
        assert!(config.cloud_specific.smartrest.use_operation_id);
        assert!(config.cloud_specific.entity_store.auto_register);
        assert!(config.cloud_specific.entity_store.clean_start);
        assert_eq!(
            config.cloud_specific.software_management.api,
            SoftwareManagementApiFlag::Legacy
        );
        assert!(!config.cloud_specific.software_management.with_types);
        assert_eq!(
            config.cloud_specific.operations.auto_log_upload,
            AutoLogUpload::OnFailure
        );
        assert!(config.cloud_specific.enable.log_upload);
        assert!(config.cloud_specific.enable.config_snapshot);
        assert!(config.cloud_specific.enable.config_update);
        assert!(config.cloud_specific.enable.firmware_update);
        assert!(config.cloud_specific.enable.device_profile);
    }

    #[test]
    fn partial_config_applies_missing_defaults() {
        let toml = r#"
            url = "tenant.example.com"

            [smartrest]
            use_operation_id = false

            [enable]
            log_upload = false

            [proxy.bind]
            port = 4312
        "#;

        let config = deserialize_from_str::<C8yMapperSpecificConfig>(toml).unwrap();

        // Explicit values preserved
        assert!(!config.cloud_specific.smartrest.use_operation_id);
        assert!(!config.cloud_specific.enable.log_upload);

        // Defaults applied for missing fields
        assert_eq!(config.cloud_specific.auth_method, AuthMethod::Certificate);
        assert!(config.cloud_specific.entity_store.auto_register);
        assert!(config.cloud_specific.enable.config_snapshot);

        // Runtime defaults: proxy port inheritance
        assert_eq!(config.cloud_specific.proxy.bind.port, 4312);
        assert_eq!(config.cloud_specific.proxy.client.port, 4312);

        // Runtime defaults: http/mqtt derived from url
        assert_eq!(
            config.cloud_specific.http.or_none().unwrap().to_string(),
            "tenant.example.com:443"
        );
        assert_eq!(
            config.cloud_specific.mqtt.or_none().unwrap().to_string(),
            "tenant.example.com:8883"
        );
    }

    #[test]
    fn explicit_values_override_all_defaults() {
        let toml = r#"
            auth_method = "basic"

            [smartrest]
            use_operation_id = false

            [entity_store]
            auto_register = false
            clean_start = false

            [software_management]
            api = "advanced"
            with_types = true

            [operations]
            auto_log_upload = "always"

            [enable]
            log_upload = false
            config_snapshot = false
            config_update = false
            firmware_update = false
            device_profile = false
        "#;

        let config = deserialize_from_str::<C8yMapperSpecificConfig>(toml).unwrap();
        let c8y_config = &config.cloud_specific;

        // All explicit values preserved, no defaults applied
        assert_eq!(c8y_config.auth_method, AuthMethod::Basic);
        assert!(!c8y_config.smartrest.use_operation_id);
        assert!(!c8y_config.entity_store.auto_register);
        assert!(!c8y_config.entity_store.clean_start);
        assert_eq!(
            c8y_config.software_management.api,
            SoftwareManagementApiFlag::Advanced
        );
        assert!(c8y_config.software_management.with_types);
        assert_eq!(c8y_config.operations.auto_log_upload, AutoLogUpload::Always);
        assert!(!c8y_config.enable.log_upload);
        assert!(!c8y_config.enable.config_snapshot);
        assert!(!c8y_config.enable.config_update);
        assert!(!c8y_config.enable.firmware_update);
        assert!(!c8y_config.enable.device_profile);
    }

    #[test]
    fn device_fields_populate_from_tedge_config() {
        let tedge_toml = r#"
            device.id = "test-id"
        "#;

        let mapper_toml = r#"
            url = "tenant.example.com"
        "#;

        let tedge_config = TEdgeConfig::from_dto(
            toml::from_str(tedge_toml).unwrap(),
            TEdgeConfigLocation::from_custom_root("/not/a/real/directory"),
        );
        let config: C8yMapperConfig =
            load_mapper_config_from_string(mapper_toml, &tedge_config, Some("random-profile"))
                .unwrap();

        // Device fields should come from tedge_config defaults
        // Call the id() method to get the device ID (which should be set from tedge_toml)
        assert_eq!(config.device.id().unwrap(), "test-id");
        // Other device fields have paths that come from the default tedge config
        assert!(config.device.key_path.as_str().contains("tedge"));
        assert!(config.device.cert_path.as_str().contains("tedge"));
        assert!(config.device.csr_path.as_str().contains("tedge"));
    }

    #[test]
    fn http_endpoint_derives_from_url_when_missing() {
        let toml = r#"
            url = "my-tenant.cumulocity.com"
        "#;

        let config = deserialize_from_str::<C8yMapperSpecificConfig>(toml).unwrap();

        // http should be derived from url with HTTPS port
        assert_eq!(
            config.http().or_none().unwrap().to_string(),
            "my-tenant.cumulocity.com:443"
        );
    }

    #[test]
    fn mqtt_key_contains_filename_if_missing() {
        let toml = "";

        let config = deserialize_from_str::<C8yMapperSpecificConfig>(toml).unwrap();

        // For C8y, we check mqtt key since url is private
        assert_eq!(config.mqtt().key(), "c8y.url");
    }

    #[test]
    fn mqtt_endpoint_derives_from_url_when_missing() {
        let toml = r#"
            url = "my-tenant.cumulocity.com"
        "#;

        let config = deserialize_from_str::<C8yMapperSpecificConfig>(toml).unwrap();

        // mqtt should be derived from url with MQTT TLS port
        assert_eq!(
            config.mqtt().or_none().unwrap().to_string(),
            "my-tenant.cumulocity.com:8883"
        );
    }

    #[test]
    fn proxy_client_port_inherits_bind_port_when_unset() {
        let toml = r#"
            url = "tenant.example.com"

            [proxy.bind]
            port = 9001
        "#;

        let config = deserialize_from_str::<C8yMapperSpecificConfig>(toml).unwrap();

        // Verify inheritance: client.port should match bind.port
        assert_eq!(config.cloud_specific.proxy.bind.port, 9001);
        assert_eq!(config.cloud_specific.proxy.client.port, 9001);
    }

    #[test]
    fn explicit_proxy_client_port_not_overridden() {
        let toml = r#"
            url = "tenant.example.com"

            [proxy.bind]
            port = 9001

            [proxy.client]
            port = 7001
        "#;

        let config = deserialize_from_str::<C8yMapperSpecificConfig>(toml).unwrap();

        // Explicit client.port should be preserved, not inherited from bind.port
        assert_eq!(config.cloud_specific.proxy.bind.port, 9001);
        assert_eq!(config.cloud_specific.proxy.client.port, 7001);
    }

    #[test]
    fn root_cert_path_has_default() {
        let toml = r#"
            url = "tenant.example.com"
        "#;

        let config = deserialize_from_str::<C8yMapperSpecificConfig>(toml).unwrap();

        // root_cert_path should have default value
        assert_eq!(config.root_cert_path.as_str(), "/etc/ssl/certs");
    }

    #[test]
    fn bridge_config_has_defaults() {
        let toml = r#"
            url = "tenant.example.com"
        "#;

        let config = deserialize_from_str::<C8yMapperSpecificConfig>(toml).unwrap();

        // Bridge should have default values (currently c8y defaults)
        assert_eq!(config.bridge.topic_prefix.as_str(), "c8y");
        assert_eq!(config.bridge.keepalive_interval.duration().as_secs(), 60);
    }

    #[test]
    fn max_payload_size_has_c8y_default() {
        let toml = r#"
            url = "tenant.example.com"
        "#;

        let config = deserialize_from_str::<C8yMapperSpecificConfig>(toml).unwrap();

        // max_payload_size should have C8Y default (16184 bytes)
        assert_eq!(config.mapper.mqtt.max_payload_size.0, 16184);
    }

    #[test]
    fn az_max_payload_size_has_azure_default() {
        let toml = r#"
            url = "mydevice.azure-devices.net"
        "#;

        let config = deserialize_from_str::<AzMapperSpecificConfig>(toml).unwrap();

        // max_payload_size should have Azure default (256 KB = 262144 bytes)
        assert_eq!(config.mapper.mqtt.max_payload_size.0, 262144);
    }

    #[test]
    fn aws_max_payload_size_has_aws_default() {
        let toml = r#"
            url = "mydevice.amazonaws.com"
        "#;

        let config = deserialize_from_str::<AwsMapperSpecificConfig>(toml).unwrap();

        // max_payload_size should have AWS default (128 KB = 131072 bytes)
        assert_eq!(config.mapper.mqtt.max_payload_size.0, 131072);
    }

    #[test]
    fn c8y_topics_include_twin_metadata() {
        let toml = r#"
            url = "tenant.cumulocity.com"
        "#;

        let config = deserialize_from_str::<C8yMapperSpecificConfig>(toml).unwrap();

        // C8y topics should include twin and metadata topics
        let topics_str = config.topics.to_string();
        assert!(topics_str.contains("twin"));
        assert!(topics_str.contains("meta"));
    }

    #[test]
    fn az_topics_exclude_twin_metadata() {
        let toml = r#"
            url = "mydevice.azure-devices.net"
        "#;

        let config = deserialize_from_str::<AzMapperSpecificConfig>(toml).unwrap();

        // Azure topics should NOT include twin or metadata topics (simpler set)
        let topics_str = config.topics.to_string();
        assert!(!topics_str.contains("twin"));
        assert!(!topics_str.contains("meta"));
    }

    #[test]
    fn aws_topics_exclude_twin_metadata() {
        let toml = r#"
            url = "mydevice.amazonaws.com"
        "#;

        let config = deserialize_from_str::<AwsMapperSpecificConfig>(toml).unwrap();

        // AWS topics should NOT include twin or metadata topics (simpler set)
        let topics_str = config.topics.to_string();
        assert!(!topics_str.contains("twin"));
        assert!(!topics_str.contains("meta"));
    }

    #[test]
    fn c8y_bridge_has_c8y_topic_prefix() {
        let toml = r#"
            url = "tenant.cumulocity.com"
        "#;

        let config = deserialize_from_str::<C8yMapperSpecificConfig>(toml).unwrap();

        assert_eq!(config.bridge.topic_prefix.as_str(), "c8y");
    }

    #[test]
    fn az_bridge_has_az_topic_prefix() {
        let toml = r#"
            url = "mydevice.azure-devices.net"
        "#;

        let config = deserialize_from_str::<AzMapperSpecificConfig>(toml).unwrap();

        assert_eq!(config.bridge.topic_prefix.as_str(), "az");
    }

    #[test]
    fn aws_bridge_has_aws_topic_prefix() {
        let toml = r#"
            url = "mydevice.amazonaws.com"
        "#;

        let config = deserialize_from_str::<AwsMapperSpecificConfig>(toml).unwrap();

        assert_eq!(config.bridge.topic_prefix.as_str(), "aws");
    }

    #[test]
    fn aws_config_can_have_specialised_and_non_specialised_mapper_fields() {
        let toml = r#"
            mapper.timestamp = false
            mapper.mqtt.max_payload_size = 12345
        "#;

        let config = deserialize_from_str::<AwsMapperSpecificConfig>(toml).unwrap();

        assert_eq!(config.mapper.mqtt.max_payload_size, MqttPayloadLimit(12345));
        assert!(!config.cloud_specific.mapper.timestamp);
    }

    #[test]
    fn empty_proxy_cert_path_matches_legacy_c8y_key_name() {
        let toml = r#"
            url = "tenant.cumulocity.com"
        "#;

        let config = deserialize_from_str::<C8yMapperSpecificConfig>(toml).unwrap();

        assert_eq!(
            config.cloud_specific.proxy.cert_path.key(),
            "c8y.proxy.cert_path"
        )
    }

    fn deserialize_from_str<T>(toml: &str) -> Result<MapperConfig<T>, MapperConfigError>
    where
        T: SpecialisedCloudConfig,
    {
        let tedge_config =
            TEdgeConfig::from_dto(TEdgeConfigDto::default(), TEdgeConfigLocation::default());
        load_mapper_config_from_string(toml, &tedge_config, None)
    }
}
