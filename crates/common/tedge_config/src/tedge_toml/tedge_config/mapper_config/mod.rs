pub mod compat;

use crate::models::CloudType;
use crate::tedge_toml::tedge_config::cert_error_into_config_error;
use crate::tedge_toml::tedge_config::default_credentials_path;
use crate::tedge_toml::ReadableKey;
use crate::TEdgeConfig;

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
use certificate::PemCertificate;
use doku::Document;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::borrow::Cow;
use std::fmt::Display;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::ops::Deref;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub use compat::load_cloud_mapper_config;
pub use compat::FromCloudConfig;

/// Device-specific configuration fields shared across all cloud types
#[derive(Debug, Document)]
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
#[derive(Debug, Document)]
pub struct BridgeConfig {
    /// The topic prefix for the bridge MQTT topic
    pub topic_prefix: Keyed<TopicPrefix>,

    /// The amount of time after which the bridge should send a ping
    pub keepalive_interval: SecondsOrHumanTime,

    pub include: BridgeIncludeConfig,
}

/// Trait linking cloud-specific config to its mapper-specific configuration
pub trait SpecialisedCloudConfig:
    Sized
    + DeserializeOwned
    + ApplyRuntimeDefaults
    + ExpectedCloudType
    + FromCloudConfig
    + Send
    + Sync
    + 'static
{
    /// The mapper-specific configuration type for this cloud
    type SpecialisedMapperConfig: DeserializeOwned
        + std::fmt::Debug
        + Document
        + Default
        + Send
        + Sync;
}

/// Base mapper configuration with common fields and cloud-specific fields via generics
#[derive(Debug, Document)]
pub struct MapperConfig<T: SpecialisedCloudConfig> {
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

    pub mapper: MapperMapperConfig<T::SpecialisedMapperConfig>,

    /// Cloud-specific configuration fields (flattened into the same level)
    pub cloud_specific: T,
}

/// Empty mapper-specific configuration for C8y (no cloud-specific mapper fields)
#[derive(Debug, Deserialize, Document, Default)]
pub struct EmptyMapperSpecific {}

/// AWS-specific mapper configuration fields
#[derive(Debug, Deserialize, Document)]
pub struct AwsMapperSpecific {
    /// Whether to add timestamps to messages
    #[serde(default = "default_timestamp")]
    pub timestamp: bool,

    /// The timestamp format to use
    #[serde(default = "default_timestamp_format")]
    pub timestamp_format: TimeFormat,
}

/// Azure-specific mapper configuration fields
#[derive(Debug, Deserialize, Document)]
pub struct AzMapperSpecific {
    /// Whether to add timestamps to messages
    #[serde(default = "default_timestamp")]
    pub timestamp: bool,

    /// The timestamp format to use
    #[serde(default = "default_timestamp_format")]
    pub timestamp_format: TimeFormat,
}

#[derive(Debug, Deserialize, Document)]
pub struct PartialMapperMapperConfig<M> {
    #[serde(default)]
    mqtt: PartialMqttConfig,

    /// Cloud-specific mapper configuration (e.g., timestamp settings for AWS/Azure)
    #[serde(flatten)]
    pub cloud_specific: M,
}

#[derive(Debug, Deserialize, Document, Default)]
pub struct PartialMqttConfig {
    #[serde(default)]
    pub max_payload_size: Option<MqttPayloadLimit>,
}

#[derive(Debug, Document)]
pub struct MapperMapperConfig<M> {
    pub mqtt: MqttConfig,

    /// Cloud-specific mapper configuration
    pub cloud_specific: M,
}

#[derive(Debug, Document)]
pub struct MqttConfig {
    /// Maximum MQTT payload size
    pub max_payload_size: MqttPayloadLimit,
}

/// SmartREST configuration for Cumulocity
#[derive(Debug, Deserialize, Document)]
#[serde(default)]
pub struct SmartrestConfig {
    /// Set of SmartREST template IDs the device should subscribe to
    #[serde(default)]
    pub templates: TemplatesSet,

    /// Switch using 501-503 or 504-506 SmartREST messages for operation status update
    #[serde(default = "default_smartrest_use_operation_id")]
    pub use_operation_id: bool,

    /// SmartREST child device configuration
    #[serde(default)]
    pub child_device: SmartrestChildDeviceConfig,
}

#[derive(Debug, Deserialize, Document)]
#[serde(default)]
pub struct Smartrest1Config {
    /// Set of SmartREST 1 template IDs the device should subscribe to
    #[serde(default)]
    pub templates: TemplatesSet,
}

/// Child device SmartREST configuration
#[derive(Debug, Deserialize, Document)]
#[serde(default)]
pub struct SmartrestChildDeviceConfig {
    /// Attach the c8y_IsDevice fragment to child devices on creation
    #[serde(default = "default_smartrest_child_device_create_with_marker")]
    pub create_with_device_marker: bool,
}

/// Proxy bind configuration
#[derive(Debug, Deserialize, Document)]
#[serde(default)]
pub struct ProxyBindConfig {
    /// The IP address local proxy binds to
    #[serde(default = "default_proxy_bind_address")]
    pub address: IpAddr,

    /// The port local proxy binds to
    #[serde(default = "default_proxy_bind_port")]
    pub port: Keyed<u16>,
}

#[derive(Debug)]
pub struct Keyed<T> {
    value: T,
    key: Cow<'static, str>,
    accessed: Arc<AtomicBool>,
}

impl<T> Keyed<T> {
    fn new(value: T, key: impl Into<Cow<'static, str>>) -> Self {
        Self {
            value,
            key: key.into(),
            accessed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn value(&self) -> &T {
        self.accessed
            .store(true, std::sync::atomic::Ordering::SeqCst);
        &self.value
    }

    pub fn key(&self) -> &Cow<'static, str> {
        &self.key
    }
}

impl<T> Drop for Keyed<T> {
    fn drop(&mut self) {
        // The key should always be set, but this has to happen after deserialising
        // If the value has been used
        if self.accessed.load(std::sync::atomic::Ordering::SeqCst) {
            debug_assert!(
                !self.key.is_empty(),
                "Must set a key for a `Keyed<T>` value after deserialising"
            )
        }
    }
}

impl<T: Display> Display for Keyed<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value().fmt(f)
    }
}

impl<T: Document> Document for Keyed<T> {
    fn ty() -> doku::Type {
        T::ty()
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

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Keyed<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self {
            value: T::deserialize(deserializer)?,
            key: "".into(),
            accessed: Arc::new(AtomicBool::new(false)),
        })
    }
}

/// Proxy client configuration
#[derive(Debug, Deserialize, Document)]
#[serde(default)]
pub struct ProxyClientConfig {
    /// The address of the host on which the proxy is running
    #[serde(default = "default_proxy_client_host")]
    pub host: Arc<str>,

    /// The port number on which the proxy is running
    #[serde(default = "default_proxy_client_port")]
    pub port: u16,
}

/// Helper function to deserialize OptionalConfig<T> from Option<T>
fn deserialize_optional_config<'de, D, T>(deserializer: D) -> Result<OptionalConfig<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(|opt| {
        opt.map(|v| OptionalConfig::present(v, ""))
            .unwrap_or_else(|| OptionalConfig::empty(""))
    })
}

/// HTTP proxy configuration for Cumulocity
#[derive(Debug, Deserialize, Document)]
#[serde(default)]
pub struct ProxyConfig {
    /// Proxy bind configuration
    #[serde(default)]
    pub bind: ProxyBindConfig,

    /// Proxy client configuration
    #[serde(default)]
    pub client: ProxyClientConfig,

    /// Server certificate path for the proxy
    #[serde(
        default = "default_optional_config",
        deserialize_with = "deserialize_optional_config"
    )]
    pub cert_path: OptionalConfig<AbsolutePath>,

    /// Server private key path for the proxy
    #[serde(
        default = "default_optional_config",
        deserialize_with = "deserialize_optional_config"
    )]
    pub key_path: OptionalConfig<AbsolutePath>,

    /// CA certificates path for the proxy
    #[serde(
        default = "default_optional_config",
        deserialize_with = "deserialize_optional_config"
    )]
    pub ca_path: OptionalConfig<AbsolutePath>,
}

/// Entity store configuration
#[derive(Debug, Deserialize, Document)]
#[serde(default)]
pub struct EntityStoreConfig {
    /// Enable auto registration feature
    #[serde(default = "default_entity_store_auto_register")]
    pub auto_register: bool,

    /// On a clean start, resend the whole device state to the cloud
    #[serde(default = "default_entity_store_clean_start")]
    pub clean_start: bool,
}

/// Software management configuration
#[derive(Debug, Deserialize, Document)]
#[serde(default)]
pub struct SoftwareManagementConfig {
    /// Software management API to use (legacy or advanced)
    #[serde(default = "default_software_management_api")]
    pub api: SoftwareManagementApiFlag,

    /// Enable publishing c8y_SupportedSoftwareTypes fragment
    #[serde(default = "default_software_management_with_types")]
    pub with_types: bool,
}

/// Operations configuration
#[derive(Debug, Deserialize, Document)]
#[serde(default)]
pub struct OperationsConfig {
    /// Auto-upload the operation log once it finishes
    #[serde(default = "default_operations_auto_log_upload")]
    pub auto_log_upload: AutoLogUpload,
}

/// Availability/heartbeat configuration for Cumulocity
#[derive(Debug, Deserialize, Document)]
#[serde(default)]
pub struct AvailabilityConfig {
    /// Enable sending heartbeat to Cumulocity periodically
    #[serde(default = "default_availability_enable")]
    pub enable: bool,

    /// Heartbeat interval to be sent to Cumulocity
    #[serde(default = "default_availability_interval")]
    pub interval: SecondsOrHumanTime,
}

/// Feature enable/disable flags
#[derive(Debug, Deserialize, Document)]
#[serde(default)]
pub struct EnableConfig {
    /// Enable log_upload feature
    #[serde(default = "default_enable_log_upload")]
    pub log_upload: bool,

    /// Enable config_snapshot feature
    #[serde(default = "default_enable_config_snapshot")]
    pub config_snapshot: bool,

    /// Enable config_update feature
    #[serde(default = "default_enable_config_update")]
    pub config_update: bool,

    /// Enable firmware_update feature
    #[serde(default = "default_enable_firmware_update")]
    pub firmware_update: bool,

    /// Enable device_profile feature
    #[serde(default = "default_enable_device_profile")]
    pub device_profile: bool,
}

/// Bridge include configuration
#[derive(Debug, Deserialize, Document)]
pub struct BridgeIncludeConfig {
    /// Set the bridge local clean session flag
    #[serde(default = "default_bridge_include_local_cleansession")]
    pub local_cleansession: AutoFlag,
}

/// MQTT service configuration for Cumulocity
#[derive(Debug, Deserialize, Document)]
#[serde(default)]
pub struct MqttServiceConfig {
    /// Whether to connect to the MQTT service endpoint or not
    #[serde(default = "default_mqtt_service_enabled")]
    pub enabled: bool,

    /// Set of MQTT topics for the MQTT service endpoint
    #[serde(default = "default_mqtt_service_topics")]
    pub topics: TemplatesSet,
}

impl Default for AwsMapperSpecific {
    fn default() -> Self {
        Self {
            timestamp: default_timestamp(),
            timestamp_format: default_timestamp_format(),
        }
    }
}

impl Default for AzMapperSpecific {
    fn default() -> Self {
        Self {
            timestamp: default_timestamp(),
            timestamp_format: default_timestamp_format(),
        }
    }
}

/// Cumulocity-specific mapper configuration fields
#[derive(Debug, Deserialize, Document)]
#[serde(default)]
pub struct C8yMapperSpecificConfig {
    /// Authentication method (certificate, basic, or auto)
    #[serde(default = "default_auth_method")]
    pub auth_method: AuthMethod,

    /// Path to credentials file for basic auth
    #[serde(default = "serde_placeholder_credentials_path")]
    pub credentials_path: AbsolutePath,

    /// SmartREST configuration
    #[serde(default = "default_smartrest_config")]
    pub smartrest: SmartrestConfig,

    /// SmartREST1 configuration
    #[serde(default = "default_smartrest1_config")]
    pub smartrest1: Smartrest1Config,

    /// HTTP endpoint for Cumulocity
    // Note: http will be derived from url at runtime, no serde default
    #[serde(
        default = "default_optional_config",
        deserialize_with = "deserialize_optional_config"
    )]
    pub http: OptionalConfig<HostPort<HTTPS_PORT>>,

    /// MQTT endpoint for Cumulocity
    // Note: mqtt will be derived from url at runtime, no serde default
    #[serde(
        default = "default_optional_config",
        deserialize_with = "deserialize_optional_config"
    )]
    pub mqtt: OptionalConfig<HostPort<MQTT_TLS_PORT>>,

    /// HTTP proxy configuration
    #[serde(default)]
    pub proxy: ProxyConfig,

    /// Entity store configuration
    #[serde(default = "default_entity_store_config")]
    pub entity_store: EntityStoreConfig,

    /// Software management configuration
    #[serde(default = "default_software_management_config")]
    pub software_management: SoftwareManagementConfig,

    /// Operations configuration
    #[serde(default = "default_operations_config")]
    pub operations: OperationsConfig,

    /// Availability/heartbeat configuration
    #[serde(default)]
    pub availability: AvailabilityConfig,

    /// Feature enable/disable flags
    #[serde(default = "default_enable_config")]
    pub enable: EnableConfig,

    /// MQTT service configuration
    #[serde(default)]
    pub mqtt_service: MqttServiceConfig,
}

/// Azure IoT-specific mapper configuration fields
#[derive(Debug, Deserialize, Document)]
pub struct AzMapperSpecificConfig {}

/// AWS IoT-specific mapper configuration fields
#[derive(Debug, Deserialize, Document)]
pub struct AwsMapperSpecificConfig {}

/// CloudConfig implementation for C8y
impl SpecialisedCloudConfig for C8yMapperSpecificConfig {
    type SpecialisedMapperConfig = EmptyMapperSpecific;
}

/// CloudConfig implementation for Azure
impl SpecialisedCloudConfig for AzMapperSpecificConfig {
    type SpecialisedMapperConfig = AzMapperSpecific;
}

/// CloudConfig implementation for AWS
impl SpecialisedCloudConfig for AwsMapperSpecificConfig {
    type SpecialisedMapperConfig = AwsMapperSpecific;
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

/// Partial device configuration with all fields optional for deserialization
#[derive(Debug, Deserialize)]
struct PartialDeviceConfig {
    id: Option<String>,
    key_path: Option<AbsolutePath>,
    cert_path: Option<AbsolutePath>,
    csr_path: Option<AbsolutePath>,
    key_uri: Option<Arc<str>>,
    key_pin: Option<Arc<str>>,
}

/// Partial bridge configuration with all fields optional for deserialization
#[derive(Debug, Deserialize)]
struct PartialBridgeConfig {
    topic_prefix: Option<TopicPrefix>,
    keepalive_interval: Option<SecondsOrHumanTime>,
    include: BridgeIncludeConfig,
}

/// Partial mapper configuration with optional common fields
#[derive(Debug, Deserialize)]
#[serde(bound(
    deserialize = "T: DeserializeOwned, T::SpecialisedMapperConfig: Default + DeserializeOwned"
))]
struct PartialMapperConfig<T: SpecialisedCloudConfig> {
    url: Option<ConnectUrl>,
    root_cert_path: Option<AbsolutePath>,
    device: Option<PartialDeviceConfig>,
    topics: Option<TemplatesSet>,
    bridge: Option<PartialBridgeConfig>,

    #[serde(default)]
    mapper: PartialMapperMapperConfig<T::SpecialisedMapperConfig>,

    #[serde(flatten)]
    cloud_specific: T,
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
) -> Result<MapperConfig<T>, MapperConfigError>
where
    T: DeserializeOwned + ApplyRuntimeDefaults + SpecialisedCloudConfig,
{
    let toml_content = tokio::fs::read_to_string(config_path.as_std_path()).await?;
    load_mapper_config_from_string(&toml_content, tedge_config, config_path)
}

fn load_mapper_config_from_string<T>(
    toml_content: &str,
    tedge_config: &TEdgeConfig,
    config_path: &AbsolutePath,
) -> Result<MapperConfig<T>, MapperConfigError>
where
    T: DeserializeOwned + ApplyRuntimeDefaults + SpecialisedCloudConfig,
{
    let partial: PartialMapperConfig<T> = toml::from_str(toml_content)?;

    let device = if let Some(partial_device) = partial.device {
        DeviceConfig {
            // device.id is optional - will be derived from certificate if not set
            id: to_optional_config(
                partial_device.id,
                format!("{config_path}: device.id").into(),
            ),
            key_path: partial_device
                .key_path
                .unwrap_or_else(|| tedge_config.device.key_path.clone()),
            cert_path: partial_device
                .cert_path
                .unwrap_or_else(|| tedge_config.device.cert_path.clone()),
            csr_path: partial_device
                .csr_path
                .unwrap_or_else(|| tedge_config.device.csr_path.clone()),
            key_uri: partial_device
                .key_uri
                .or_else(|| tedge_config.device.key_uri.or_none().cloned()),
            key_pin: partial_device
                .key_pin
                .or_else(|| tedge_config.device.key_pin.or_none().cloned()),
        }
    } else {
        // No device section in file, use all defaults from tedge_config
        DeviceConfig {
            // device.id is optional - will be derived from certificate if not set
            id: to_optional_config(
                tedge_config.device.id().ok().map(|s| s.to_string()),
                "device.id".into(),
            ),
            key_path: tedge_config.device.key_path.clone(),
            cert_path: tedge_config.device.cert_path.clone(),
            csr_path: tedge_config.device.csr_path.clone(),
            key_uri: tedge_config.device.key_uri.or_none().cloned(),
            key_pin: tedge_config.device.key_pin.or_none().cloned(),
        }
    };

    // Apply defaults for bridge fields
    let bridge = if let Some(partial_bridge) = partial.bridge {
        BridgeConfig {
            topic_prefix: Keyed::new(
                partial_bridge
                    .topic_prefix
                    .unwrap_or_else(T::default_bridge_topic_prefix),
                format!("{config_path}: bridge.topic_prefix"),
            ),
            keepalive_interval: partial_bridge
                .keepalive_interval
                .unwrap_or_else(default_keepalive_interval),
            include: partial_bridge.include,
        }
    } else {
        // No bridge section, use all defaults
        BridgeConfig {
            topic_prefix: Keyed::new(
                T::default_bridge_topic_prefix(),
                format!("{config_path}: bridge.topic_prefix"),
            ),
            keepalive_interval: default_keepalive_interval(),
            include: default_bridge_include_config(),
        }
    };

    // Apply default for root_cert_path
    let root_cert_path = Keyed::new(
        partial
            .root_cert_path
            .unwrap_or_else(default_root_cert_path),
        format!("{config_path}: root_cert_path"),
    );

    let url = to_optional_config(partial.url, format!("{config_path}: url").into());

    // Apply default topics
    let topics = partial.topics.unwrap_or_else(T::default_topics);

    // Apply default max_payload_size
    let max_payload_size = partial
        .mapper
        .mqtt
        .max_payload_size
        .unwrap_or_else(T::default_max_payload_size);

    // Get cloud-specific config (already has serde defaults applied)
    let mut cloud_specific = partial.cloud_specific;

    // Apply runtime defaults to cloud_specific
    cloud_specific.apply_runtime_defaults(&url, tedge_config, config_path);

    // Construct the final configuration
    Ok(MapperConfig {
        url,
        root_cert_path,
        device,
        topics,
        bridge,
        mapper: MapperMapperConfig {
            mqtt: MqttConfig { max_payload_size },
            cloud_specific: partial.mapper.cloud_specific,
        },
        cloud_specific,
    })
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

/// Trait for applying runtime defaults to cloud-specific configurations
pub trait ApplyRuntimeDefaults {
    fn apply_runtime_defaults(
        &mut self,
        url: &OptionalConfig<ConnectUrl>,
        tedge_config: &TEdgeConfig,
        config_path: &AbsolutePath,
    );

    /// Returns the default bridge topic prefix for this cloud type
    fn default_bridge_topic_prefix() -> TopicPrefix;

    /// Returns the default topics for this cloud type
    fn default_topics() -> TemplatesSet;

    /// Returns the default max payload size for this cloud type
    fn default_max_payload_size() -> MqttPayloadLimit;
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

fn default_keepalive_interval() -> SecondsOrHumanTime {
    "60s".parse().expect("Valid duration")
}

// Common MapperConfig defaults
fn default_root_cert_path() -> AbsolutePath {
    "/etc/ssl/certs".parse().expect("Valid path")
}

// C8y mapper specific defaults
fn default_auth_method() -> AuthMethod {
    AuthMethod::Certificate
}

fn serde_placeholder_credentials_path() -> AbsolutePath {
    AbsolutePath::try_new("/").expect("valid path")
}

fn default_smartrest_config() -> SmartrestConfig {
    SmartrestConfig {
        templates: TemplatesSet::default(),
        use_operation_id: true,
        child_device: SmartrestChildDeviceConfig::default(),
    }
}

fn default_smartrest1_config() -> Smartrest1Config {
    Smartrest1Config {
        templates: TemplatesSet::default(),
    }
}

fn default_smartrest_use_operation_id() -> bool {
    true
}

fn default_smartrest_child_device_create_with_marker() -> bool {
    false
}

fn default_proxy_bind_address() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

fn default_proxy_bind_port() -> Keyed<u16> {
    Keyed::new(8001, "")
}

fn default_proxy_client_host() -> Arc<str> {
    Arc::from("127.0.0.1")
}

fn default_proxy_client_port() -> u16 {
    8001 // Will be overridden at runtime if bind.port differs
}

fn default_optional_config<T>() -> OptionalConfig<T> {
    OptionalConfig::empty("")
}

fn default_proxy_config() -> ProxyConfig {
    ProxyConfig {
        bind: ProxyBindConfig {
            address: default_proxy_bind_address(),
            port: default_proxy_bind_port(),
        },
        client: ProxyClientConfig {
            host: default_proxy_client_host(),
            port: default_proxy_client_port(),
        },
        cert_path: default_optional_config(),
        key_path: default_optional_config(),
        ca_path: default_optional_config(),
    }
}

fn default_bridge_include_local_cleansession() -> AutoFlag {
    AutoFlag::Auto
}

fn default_bridge_include_config() -> BridgeIncludeConfig {
    BridgeIncludeConfig {
        local_cleansession: default_bridge_include_local_cleansession(),
    }
}

fn default_entity_store_auto_register() -> bool {
    true
}

fn default_entity_store_clean_start() -> bool {
    true
}

fn default_entity_store_config() -> EntityStoreConfig {
    EntityStoreConfig {
        auto_register: default_entity_store_auto_register(),
        clean_start: default_entity_store_clean_start(),
    }
}

fn default_software_management_api() -> SoftwareManagementApiFlag {
    SoftwareManagementApiFlag::Legacy
}

fn default_software_management_with_types() -> bool {
    false
}

fn default_software_management_config() -> SoftwareManagementConfig {
    SoftwareManagementConfig {
        api: default_software_management_api(),
        with_types: default_software_management_with_types(),
    }
}

fn default_operations_auto_log_upload() -> AutoLogUpload {
    AutoLogUpload::OnFailure
}

fn default_operations_config() -> OperationsConfig {
    OperationsConfig {
        auto_log_upload: default_operations_auto_log_upload(),
    }
}

fn default_availability_enable() -> bool {
    true
}

fn default_availability_interval() -> SecondsOrHumanTime {
    "60m".parse().expect("Valid duration")
}

fn default_availability_config() -> AvailabilityConfig {
    AvailabilityConfig {
        enable: default_availability_enable(),
        interval: default_availability_interval(),
    }
}

fn default_mqtt_service_enabled() -> bool {
    false
}

fn default_mqtt_service_topics() -> TemplatesSet {
    "$demo,$error".parse().expect("Valid templates set")
}

fn default_enable_log_upload() -> bool {
    true
}

fn default_enable_config_snapshot() -> bool {
    true
}

fn default_enable_config_update() -> bool {
    true
}

fn default_enable_firmware_update() -> bool {
    true
}

fn default_enable_device_profile() -> bool {
    true
}

fn default_enable_config() -> EnableConfig {
    EnableConfig {
        log_upload: default_enable_log_upload(),
        config_snapshot: default_enable_config_snapshot(),
        config_update: default_enable_config_update(),
        firmware_update: default_enable_firmware_update(),
        device_profile: default_enable_device_profile(),
    }
}

// Azure/AWS timestamp defaults
fn default_timestamp() -> bool {
    true
}

fn default_timestamp_format() -> TimeFormat {
    TimeFormat::Unix
}

impl Default for SmartrestConfig {
    fn default() -> Self {
        default_smartrest_config()
    }
}

impl Default for Smartrest1Config {
    fn default() -> Self {
        default_smartrest1_config()
    }
}

impl Default for SmartrestChildDeviceConfig {
    fn default() -> Self {
        Self {
            create_with_device_marker: default_smartrest_child_device_create_with_marker(),
        }
    }
}

impl Default for ProxyBindConfig {
    fn default() -> Self {
        Self {
            address: default_proxy_bind_address(),
            port: default_proxy_bind_port(),
        }
    }
}

impl Default for ProxyClientConfig {
    fn default() -> Self {
        Self {
            host: default_proxy_client_host(),
            port: default_proxy_client_port(),
        }
    }
}

impl Default for ProxyConfig {
    fn default() -> Self {
        default_proxy_config()
    }
}

impl Default for EntityStoreConfig {
    fn default() -> Self {
        default_entity_store_config()
    }
}

impl Default for SoftwareManagementConfig {
    fn default() -> Self {
        default_software_management_config()
    }
}

impl Default for OperationsConfig {
    fn default() -> Self {
        default_operations_config()
    }
}

impl Default for AvailabilityConfig {
    fn default() -> Self {
        default_availability_config()
    }
}

impl Default for EnableConfig {
    fn default() -> Self {
        default_enable_config()
    }
}

impl Default for BridgeIncludeConfig {
    fn default() -> Self {
        default_bridge_include_config()
    }
}

impl Default for MqttServiceConfig {
    fn default() -> Self {
        Self {
            enabled: default_mqtt_service_enabled(),
            topics: default_mqtt_service_topics(),
        }
    }
}

impl Default for C8yMapperSpecificConfig {
    fn default() -> Self {
        Self {
            auth_method: default_auth_method(),
            credentials_path: AbsolutePath::try_new("/").expect("Valid path"),
            smartrest: default_smartrest_config(),
            smartrest1: default_smartrest1_config(),
            http: OptionalConfig::Empty("".into()), // Will be derived from url at runtime
            mqtt: OptionalConfig::Empty("".into()), // Will be derived from url at runtime
            proxy: ProxyConfig::default(),
            entity_store: default_entity_store_config(),
            software_management: default_software_management_config(),
            operations: default_operations_config(),
            availability: default_availability_config(),
            enable: default_enable_config(),
            mqtt_service: MqttServiceConfig::default(),
        }
    }
}

impl<T: Default> Default for PartialMapperMapperConfig<T> {
    fn default() -> Self {
        Self {
            mqtt: PartialMqttConfig::default(),
            cloud_specific: T::default(),
        }
    }
}

fn set_key_if_blank<T>(field: &mut OptionalConfig<T>, value: Cow<'static, str>) {
    use OptionalConfig as OC;
    match field {
        OC::Present { ref mut key, .. } | OC::Empty(ref mut key) if key.is_empty() => *key = value,
        _ => (),
    }
}

fn set_key_if_blank2<T>(field: &mut Keyed<T>, value: Cow<'static, str>) {
    if field.key.is_empty() {
        field.key = value
    }
}

fn convert_optional_value<T: Clone, U: From<T>>(field: &OptionalConfig<T>) -> OptionalConfig<U> {
    match field.clone() {
        OptionalConfig::Present { value, key } => OptionalConfig::Present {
            value: value.into(),
            key,
        },
        OptionalConfig::Empty(key) => OptionalConfig::Empty(key),
    }
}

fn to_optional_config<T>(field: Option<T>, key: Cow<'static, str>) -> OptionalConfig<T> {
    match field {
        Some(value) => OptionalConfig::Present { value, key },
        None => OptionalConfig::Empty(key),
    }
}

impl ApplyRuntimeDefaults for C8yMapperSpecificConfig {
    fn apply_runtime_defaults(
        &mut self,
        url: &OptionalConfig<ConnectUrl>,
        tedge_config: &TEdgeConfig,
        config_path: &AbsolutePath,
    ) {
        // Derive http endpoint from url if it's not been set
        if self.http.or_none().is_none() {
            self.http = convert_optional_value(url);
        }

        // Derive mqtt endpoint from url if it's not been set
        if self.mqtt.or_none().is_none() {
            self.mqtt = convert_optional_value(url);
        }

        // Apply proxy port inheritance: client.port defaults to bind.port
        if self.proxy.client.port == 8001 && self.proxy.bind.port != 8001 {
            self.proxy.client.port = *self.proxy.bind.port;
        }

        if self.credentials_path == serde_placeholder_credentials_path() {
            self.credentials_path = default_credentials_path(&tedge_config.location)
        }

        // Don't need to set the key for http or mqtt as these are set from url
        set_key_if_blank(
            &mut self.proxy.cert_path,
            format!("{}: proxy.cert_path", config_path).into(),
        );
        set_key_if_blank(
            &mut self.proxy.key_path,
            format!("{}: proxy.key_path", config_path).into(),
        );
        set_key_if_blank(
            &mut self.proxy.ca_path,
            format!("{}: proxy.ca_path", config_path).into(),
        );
        set_key_if_blank2(
            &mut self.proxy.bind.port,
            format!("{}: proxy.bind.port", config_path).into(),
        );
    }

    fn default_bridge_topic_prefix() -> TopicPrefix {
        TopicPrefix::try_new("c8y").unwrap()
    }

    fn default_topics() -> TemplatesSet {
        "te/+/+/+/+,te/+/+/+/+/twin/+,te/+/+/+/+/m/+,te/+/+/+/+/m/+/meta,te/+/+/+/+/e/+,te/+/+/+/+/a/+,te/+/+/+/+/status/health".parse().expect("Valid templateset")
    }

    fn default_max_payload_size() -> MqttPayloadLimit {
        super::c8y_mqtt_payload_limit()
    }
}

impl ApplyRuntimeDefaults for AzMapperSpecificConfig {
    fn apply_runtime_defaults(
        &mut self,
        _url: &OptionalConfig<ConnectUrl>,
        _tedge_config: &TEdgeConfig,
        _config_path: &AbsolutePath,
    ) {
        // Azure config has no runtime defaults currently
    }

    fn default_bridge_topic_prefix() -> TopicPrefix {
        TopicPrefix::try_new("az").unwrap()
    }

    fn default_topics() -> TemplatesSet {
        "te/+/+/+/+/m/+,te/+/+/+/+/e/+,te/+/+/+/+/a/+,te/+/+/+/+/status/health"
            .parse()
            .expect("Valid templateset")
    }

    fn default_max_payload_size() -> MqttPayloadLimit {
        super::az_mqtt_payload_limit()
    }
}

impl ApplyRuntimeDefaults for AwsMapperSpecificConfig {
    fn apply_runtime_defaults(
        &mut self,
        _url: &OptionalConfig<ConnectUrl>,
        _tedge_config: &TEdgeConfig,
        _config_path: &AbsolutePath,
    ) {
        // AWS config has no runtime defaults currently
    }

    fn default_bridge_topic_prefix() -> TopicPrefix {
        TopicPrefix::try_new("aws").unwrap()
    }
    fn default_topics() -> TemplatesSet {
        "te/+/+/+/+/m/+,te/+/+/+/+/e/+,te/+/+/+/+/a/+,te/+/+/+/+/status/health"
            .parse()
            .expect("Valid templateset")
    }

    fn default_max_payload_size() -> MqttPayloadLimit {
        super::aws_mqtt_payload_limit()
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
        let toml = "";
        let config: C8yMapperSpecificConfig = toml::from_str(toml).unwrap();

        // Verify all defaults are applied
        assert_eq!(config.auth_method, AuthMethod::Certificate);
        assert!(config.smartrest.use_operation_id);
        assert!(config.entity_store.auto_register);
        assert!(config.entity_store.clean_start);
        assert_eq!(
            config.software_management.api,
            SoftwareManagementApiFlag::Legacy
        );
        assert!(!config.software_management.with_types);
        assert_eq!(config.operations.auto_log_upload, AutoLogUpload::OnFailure);
        assert!(config.enable.log_upload);
        assert!(config.enable.config_snapshot);
        assert!(config.enable.config_update);
        assert!(config.enable.firmware_update);
        assert!(config.enable.device_profile);
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

        let config: C8yMapperSpecificConfig = toml::from_str(toml).unwrap();

        // All explicit values preserved, no defaults applied
        assert_eq!(config.auth_method, AuthMethod::Basic);
        assert!(!config.smartrest.use_operation_id);
        assert!(!config.entity_store.auto_register);
        assert!(!config.entity_store.clean_start);
        assert_eq!(
            config.software_management.api,
            SoftwareManagementApiFlag::Advanced
        );
        assert!(config.software_management.with_types);
        assert_eq!(config.operations.auto_log_upload, AutoLogUpload::Always);
        assert!(!config.enable.log_upload);
        assert!(!config.enable.config_snapshot);
        assert!(!config.enable.config_update);
        assert!(!config.enable.firmware_update);
        assert!(!config.enable.device_profile);
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
            &toml::from_str(tedge_toml).unwrap(),
            TEdgeConfigLocation::from_custom_root("/not/a/real/directory"),
        );
        let config: C8yMapperConfig = load_mapper_config_from_string(
            mapper_toml,
            &tedge_config,
            &AbsolutePath::try_new("notondisk.toml").unwrap(),
        )
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
        assert_eq!(config.mqtt().key(), "/not/on/disk.toml: url");
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
        assert!(!config.mapper.cloud_specific.timestamp);
    }

    #[test]
    fn empty_proxy_cert_path_has_file_in_empty_key_name() {
        let toml = r#"
            url = "tenant.cumulocity.com"
        "#;

        let config = deserialize_from_str::<C8yMapperSpecificConfig>(toml).unwrap();

        assert_eq!(
            config.cloud_specific.proxy.cert_path.key(),
            "/not/on/disk.toml: proxy.cert_path"
        )
    }

    fn deserialize_from_str<T>(toml: &str) -> Result<MapperConfig<T>, MapperConfigError>
    where
        T: DeserializeOwned + ApplyRuntimeDefaults + SpecialisedCloudConfig,
    {
        let tedge_config =
            TEdgeConfig::from_dto(&TEdgeConfigDto::default(), TEdgeConfigLocation::default());
        load_mapper_config_from_string(
            toml,
            &tedge_config,
            &AbsolutePath::try_new("/not/on/disk.toml").unwrap(),
        )
    }
}
