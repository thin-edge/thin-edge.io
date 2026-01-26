pub mod compat;

use crate::models::CloudType;
use crate::tedge_toml::tedge_config::cert_error_into_config_error;
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

#[derive(Clone, Debug)]
pub struct MapperConfigPath<'a> {
    pub(crate) base_dir: Cow<'a, Utf8Path>,
    pub(crate) cloud_type: CloudType,
}

impl MapperConfigPath<'_> {
    pub fn path_for(&self, profile: Option<&(impl AsRef<str> + ?Sized)>) -> Utf8PathBuf {
        let dir = &*self.base_dir;
        let ty = self.cloud_type;
        match profile {
            None => dir.join(format!("{ty}/tedge.toml")),
            Some(profile) => {
                let profile = profile.as_ref();
                dir.join(format!("{ty}.{profile}/tedge.toml"))
            }
        }
    }
}

pub trait HasPath {
    fn set_mappers_root_dir(&mut self, path: Utf8PathBuf);
    fn config_path(&self) -> Option<MapperConfigPath<'_>>;
    fn set_mapper_config_file(&mut self, path: Utf8PathBuf);
}

impl HasPath for TEdgeConfigDtoC8y {
    fn set_mappers_root_dir(&mut self, path: Utf8PathBuf) {
        self.mapper_config_dir = Some(path)
    }

    fn config_path(&self) -> Option<MapperConfigPath<'_>> {
        Some(MapperConfigPath {
            base_dir: Cow::Borrowed(self.mapper_config_dir.as_deref()?),
            cloud_type: CloudType::C8y,
        })
    }

    fn set_mapper_config_file(&mut self, path: Utf8PathBuf) {
        self.mapper_config_file = Some(path)
    }
}

impl HasPath for TEdgeConfigDtoAz {
    fn set_mappers_root_dir(&mut self, path: Utf8PathBuf) {
        self.mapper_config_dir = Some(path)
    }

    fn config_path(&self) -> Option<MapperConfigPath<'_>> {
        Some(MapperConfigPath {
            base_dir: Cow::Borrowed(self.mapper_config_dir.as_deref()?),
            cloud_type: CloudType::Az,
        })
    }

    fn set_mapper_config_file(&mut self, path: Utf8PathBuf) {
        self.mapper_config_file = Some(path)
    }
}

impl HasPath for TEdgeConfigDtoAws {
    fn set_mappers_root_dir(&mut self, path: Utf8PathBuf) {
        self.mapper_config_dir = Some(path)
    }

    fn config_path(&self) -> Option<MapperConfigPath<'_>> {
        Some(MapperConfigPath {
            base_dir: Cow::Borrowed(self.mapper_config_dir.as_deref()?),
            cloud_type: CloudType::Aws,
        })
    }

    fn set_mapper_config_file(&mut self, path: Utf8PathBuf) {
        self.mapper_config_file = Some(path)
    }
}

/// Base mapper configuration with common fields and cloud-specific fields via generics
pub struct MapperConfig<T: SpecialisedCloudConfig> {
    pub(crate) location: Utf8PathBuf,

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

    pub mapper: CommonMapperConfig,

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

pub struct CommonMapperConfig {
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

    /// Enable device restart feature
    pub device_restart: bool,
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
