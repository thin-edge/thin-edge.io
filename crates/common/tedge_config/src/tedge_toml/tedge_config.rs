mod version;
use reqwest::NoProxy;
use version::TEdgeTomlVersion;

mod append_remove;
pub use append_remove::AppendRemoveItem;

use super::models::auth_method::AuthMethod;
use super::models::proxy_url::ProxyUrl;
use super::models::timestamp::TimeFormat;
use super::models::AptConfig;
use super::models::AutoFlag;
use super::models::AutoLogUpload;
use super::models::ConnectUrl;
use super::models::Cryptoki;
use super::models::HostPort;
use super::models::MqttPayloadLimit;
use super::models::SecondsOrHumanTime;
use super::models::SoftwareManagementApiFlag;
use super::models::TemplatesSet;
use super::models::TopicPrefix;
use super::models::HTTPS_PORT;
use super::models::MQTT_TLS_PORT;
use super::tedge_config_location::TEdgeConfigLocation;
use crate::models::AbsolutePath;
use anyhow::anyhow;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use certificate::parse_root_certificate::client_config_for_ca_certificates;
use certificate::parse_root_certificate::create_tls_config;
use certificate::parse_root_certificate::create_tls_config_without_client_cert;
use certificate::read_trust_store;
use certificate::CertificateError;
use certificate::CloudHttpConfig;
use certificate::PemCertificate;
use doku::Document;
use once_cell::sync::Lazy;
use reqwest::Certificate;
use std::borrow::Cow;
use std::io::Read;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::num::NonZeroU16;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
pub use tedge_config_macros::ConfigNotSet;
pub use tedge_config_macros::MultiError;
pub use tedge_config_macros::ProfileName;
use tedge_config_macros::*;
use tracing::error;

mod mqtt_config;
pub use mqtt_config::MqttAuthClientConfig;
pub use mqtt_config::MqttAuthConfig;
pub use mqtt_config::MqttAuthConfigCloudBroker;

const DEFAULT_ROOT_CERT_PATH: &str = "/etc/ssl/certs";

pub const C8Y_MQTT_PAYLOAD_LIMIT: u32 = 16184; // 16 KB
pub const AZ_MQTT_PAYLOAD_LIMIT: u32 = 262144; // 256 KB
pub const AWS_MQTT_PAYLOAD_LIMIT: u32 = 131072; // 128 KB

pub trait OptionalConfigError<T> {
    fn or_err(&self) -> Result<&T, ReadError>;
}

impl<T> OptionalConfigError<T> for OptionalConfig<T> {
    fn or_err(&self) -> Result<&T, ReadError> {
        self.or_config_not_set().map_err(ReadError::from)
    }
}

#[derive(Clone)]
pub struct TEdgeConfig(TEdgeConfigReader);

impl std::ops::Deref for TEdgeConfig {
    type Target = TEdgeConfigReader;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TEdgeConfig {
    pub fn from_dto(dto: &TEdgeConfigDto, location: &TEdgeConfigLocation) -> Self {
        Self(TEdgeConfigReader::from_dto(dto, location))
    }
}

/// The keys that can be read from the configuration
pub static READABLE_KEYS: Lazy<Vec<(Cow<'static, str>, doku::Type)>> = Lazy::new(|| {
    let ty = TEdgeConfigReader::ty();
    let doku::TypeKind::Struct {
        fields,
        transparent: false,
    } = ty.kind
    else {
        panic!("Expected struct but got {:?}", ty.kind)
    };
    let doku::Fields::Named { fields } = fields else {
        panic!("Expected named fields but got {:?}", fields)
    };
    struct_field_paths(None, &fields)
});

define_tedge_config! {
    #[tedge_config(reader(skip))]
    config: {
        #[tedge_config(default(variable = "TEdgeTomlVersion::One"))]
        version: TEdgeTomlVersion,
    },

    device: {
        /// Identifier of the device within the fleet. It must be globally
        /// unique and is derived from the device certificate.
        #[tedge_config(reader(function = "device_id", private))]
        #[tedge_config(example = "Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665")]
        #[doku(as = "String")]
        id: Result<String, ReadError>,

        /// Path where the device's private key is stored
        #[tedge_config(example = "/etc/tedge/device-certs/tedge-private-key.pem", default(function = "default_device_key"), reader(private))]
        key_path: AbsolutePath,

        /// Path where the device's certificate is stored
        #[tedge_config(example = "/etc/tedge/device-certs/tedge-certificate.pem", default(function = "default_device_cert"), reader(private))]
        cert_path: AbsolutePath,

        /// Path where the device's certificate signing request is stored
        #[tedge_config(example = "/etc/tedge/device-certs/tedge.csr", default(function = "default_device_csr"), reader(private))]
        csr_path: AbsolutePath,

        /// A PKCS#11 URI of the private key.
        ///
        /// See RFC #7512.
        #[tedge_config(example = "pkcs11:token=my-pkcs11-token;object=my-key")]
        key_uri: Arc<str>,

        cryptoki: {
            /// Whether to use a Hardware Security Module for authenticating the MQTT connection with the cloud.
            ///
            /// "off" to not use the HSM, "module" to use the provided cryptoki dynamic module, "socket" to access the
            /// HSM via tedge-p11-server signing service.
            #[tedge_config(default(variable = Cryptoki::Off))]
            #[tedge_config(example = "off", example = "module", example = "socket")]
            mode: Cryptoki,

            /// A path to the PKCS#11 module used for interaction with the HSM.
            ///
            /// Needs to be set when `device.cryptoki.mode` is set to `module`
            #[tedge_config(example = "/usr/lib/x86_64-linux-gnu/opensc-pkcs11.so")]
            #[doku(as = "PathBuf")]
            module_path: AbsolutePath,

            /// Pin value for logging into the HSM.
            #[tedge_config(example = "123456", default(value = "123456"))]
            pin: Arc<str>,

            /// A URI of the token/object to be used by tedge-p11-server.
            ///
            /// See RFC #7512.
            // NOTE: this field isn't actually used by anything in tedge-config yet - it can appear in tedge.toml though
            // because it's read by tedge-p11-server crate, but the crate doesn't use tedge-config to read it (because
            // adding tedge-config as dependency would introduce dependency cycle) but defines the schema itself. So
            // while this field could be removed and nothing would break, it's kept to inform readers that such field
            // can appear and to make tedge-p11-server actually use this field here once dependency cycles are resolved.
            #[tedge_config(example = "pkcs11:token=my-pkcs11-token;object=my-key")]
            uri: Arc<str>,

            /// A path to the tedge-p11-server socket.
            ///
            /// Needs to be set when `device.cryptoki.mode` is set to `module`
            #[tedge_config(default(value = "/run/tedge-p11-server/tedge-p11-server.sock"), example = "/run/tedge-p11-server/tedge-p11-server.sock")]
            #[doku(as = "PathBuf")]
            socket_path: Utf8PathBuf,
        },

        /// The default device type
        #[tedge_config(example = "thin-edge.io", default(value = "thin-edge.io"))]
        #[tedge_config(rename = "type")]
        ty: String,
    },

    certificate: {
        validity: {
            /// Requested validity duration for a new certificate
            #[tedge_config(note = "The CA might return certificates valid for period shorter than requested")]
            #[tedge_config(example = "365d", default(from_str = "365d"))]
            requested_duration: SecondsOrHumanTime,

            /// Minimum validity duration below which a new certificate should be requested
            #[tedge_config(note = "This is an advisory setting and the renewal has to be scheduled")]
            #[tedge_config(example = "30d", default(from_str = "30d"))]
            minimum_duration: SecondsOrHumanTime,
        },

        /// Organization name used for certificate signing requests
        #[tedge_config(example = "ACME", default(value = "Thin Edge"))]
        organization: Arc<str>,

        /// Organization unit used for certificate signing requests
        #[tedge_config(example = "IoT", default(value = "Device"))]
        organization_unit: Arc<str>,
    },

    #[tedge_config(multi)]
    c8y: {
        /// Endpoint URL of Cumulocity tenant
        #[tedge_config(example = "your-tenant.cumulocity.com")]
        // Config consumers should use `c8y.http`/`c8y.mqtt` as appropriate, hence this field is private
        #[tedge_config(reader(private))]
        url: ConnectUrl,

        /// Cumulocity tenant ID
        tenant_id: String,

        /// The path where Cumulocity root certificate(s) are stored
        #[tedge_config(note = "The value can be a directory path as well as the path of the certificate file.")]
        #[tedge_config(example = "/etc/tedge/c8y-trusted-root-certificates.pem", default(function = "default_root_cert_path"))]
        root_cert_path: AbsolutePath,

        /// The authentication method used to connect Cumulocity
        #[tedge_config(note = "In the auto mode, basic auth is used if c8y.credentials_path is set")]
        #[tedge_config(example = "certificate", example = "basic", example = "auto", default(variable = AuthMethod::Certificate))]
        auth_method: AuthMethod,

        /// The path where Cumulocity username/password are stored
        #[tedge_config(note = "The value must be the path of the credentials file.")]
        #[tedge_config(example = "/etc/tedge/credentials.toml", default(function = "default_credentials_path"))]
        credentials_path: AbsolutePath,

        device: {
            /// Identifier of the device within the fleet. It must be globally
            /// unique and is derived from the device certificate.
            #[tedge_config(reader(function = "c8y_device_id"))]
            #[tedge_config(default(from_optional_key = "device.id"))]
            #[tedge_config(example = "Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665")]
            #[doku(as = "String")]
            id: Result<String, ReadError>,

            /// Path where the device's private key is stored
            #[tedge_config(example = "/etc/tedge/device-certs/tedge-private-key.pem", default(from_key = "device.key_path"))]
            key_path: AbsolutePath,

            /// Path where the device's certificate is stored
            #[tedge_config(example = "/etc/tedge/device-certs/tedge-certificate.pem", default(from_key = "device.cert_path"))]
            cert_path: AbsolutePath,

            /// Path where the device's certificate signing request is stored
            #[tedge_config(example = "/etc/tedge/device-certs/tedge.csr", default(from_key = "device.csr_path"))]
            csr_path: AbsolutePath,

            /// A PKCS#11 URI of the private key.
            ///
            /// See RFC #7512.
            #[tedge_config(example = "pkcs11:token=my-pkcs11-token;object=my-key")]
            key_uri: Arc<str>,
        },

        smartrest: {
            /// Set of SmartREST template IDs the device should subscribe to
            #[tedge_config(example = "templateId1,templateId2", default(function = "TemplatesSet::default"))]
            templates: TemplatesSet,

            /// Switch using 501-503 (without operation ID) or 504-506 (with operation ID) SmartREST messages for operation status update
            #[tedge_config(example = "true", default(value = true))]
            use_operation_id: bool,

            child_device: {
                /// Attach the c8y_IsDevice fragment to child devices on creation
                #[tedge_config(example = "false", default(value = false))]
                create_with_device_marker: bool,
            }
        },

        smartrest1: {
            /// Set of SmartREST 1.0 template IDs the device should subscribe to
            #[tedge_config(example = "templateId1,templateId2", default(function = "TemplatesSet::default"))]
            templates: TemplatesSet,
        },

        /// HTTP Endpoint for the Cumulocity tenant, with optional port.
        #[tedge_config(example = "http.your-tenant.cumulocity.com:1234")]
        #[tedge_config(default(from_optional_key = "c8y.url"))]
        http: HostPort<HTTPS_PORT>,

        /// MQTT Endpoint for the Cumulocity tenant, with optional port.
        #[tedge_config(example = "mqtt.your-tenant.cumulocity.com:1234")]
        #[tedge_config(default(from_optional_key = "c8y.url"))]
        mqtt: HostPort<MQTT_TLS_PORT>,

        /// Set of MQTT topics the Cumulocity mapper should subscribe to
        #[tedge_config(example = "te/+/+/+/+/a/+,te/+/+/+/+/m/+,te/+/+/+/+/e/+")]
        #[tedge_config(default(value = "te/+/+/+/+,te/+/+/+/+/twin/+,te/+/+/+/+/m/+,te/+/+/+/+/e/+,te/+/+/+/+/a/+,te/+/+/+/+/status/health"))]
        topics: TemplatesSet,

        enable: {
            /// Enable log_upload feature
            #[tedge_config(example = "true", default(value = true), deprecated_name = "log_management")]
            log_upload: bool,

            /// Enable config_snapshot feature
            #[tedge_config(example = "true", default(value = true))]
            config_snapshot: bool,

            /// Enable config_update feature
            #[tedge_config(example = "true", default(value = true))]
            config_update: bool,

            /// Enable firmware_update feature
            #[tedge_config(example = "true", default(value = true))]
            firmware_update: bool,

            /// Enable device_profile feature
            #[tedge_config(example = "true", default(value = true))]
            device_profile: bool,
        },

        mapper: {
            mqtt: {
                /// The maximum message payload size that can be mapped to the cloud via MQTT
                #[tedge_config(example = "16184", default(function = "c8y_mqtt_payload_limit"))]
                max_payload_size: MqttPayloadLimit,
            }
        },

        proxy: {
            bind: {
                /// The IP address local Cumulocity HTTP proxy binds to
                #[tedge_config(example = "127.0.0.1", default(variable = "Ipv4Addr::LOCALHOST"))]
                address: IpAddr,

                /// The port local Cumulocity HTTP proxy binds to
                #[tedge_config(example = "8001", default(value = 8001u16))]
                port: u16,
            },

            client: {
                /// The address of the host on which the local Cumulocity HTTP Proxy is running, used by the Cumulocity
                /// mapper.
                #[tedge_config(default(value = "127.0.0.1"))]
                #[tedge_config(example = "127.0.0.1", example = "192.168.1.2", example = "tedge-hostname")]
                host: Arc<str>,

                /// The port number on the remote host on which the local Cumulocity HTTP Proxy is running, used by the
                /// Cumulocity mapper.
                #[tedge_config(example = "8001")]
                #[tedge_config(default(from_key = "c8y.proxy.bind.port"))]
                port: u16,
            },

            /// The file that will be used as the server certificate for the Cumulocity proxy
            #[tedge_config(example = "/etc/tedge/device-certs/c8y_proxy_certificate.pem")]
            cert_path: AbsolutePath,

            /// The file that will be used as the server private key for the Cumulocity proxy
            #[tedge_config(example = "/etc/tedge/device-certs/c8y_proxy_key.pem")]
            key_path: AbsolutePath,

            /// Path to a file containing the PEM encoded CA certificates that are
            /// trusted when checking incoming client certificates for the Cumulocity Proxy
            #[tedge_config(example = "/etc/ssl/certs")]
            ca_path: AbsolutePath,
        },

        bridge: {
            include: {
                /// Set the bridge local clean session flag (this requires mosquitto >= 2.0.0)
                #[tedge_config(note = "If set to 'auto', this cleans the local session accordingly the detected version of mosquitto.")]
                #[tedge_config(example = "auto", default(variable = "AutoFlag::Auto"))]
                local_cleansession: AutoFlag,
            },

            /// The topic prefix that will be used for the bridge MQTT topic. For instance,
            /// if this is set to "c8y", then messages published to `c8y/s/us` will be
            /// forwarded to Cumulocity on the `s/us` topic
            #[tedge_config(example = "c8y", default(function = "c8y_topic_prefix"))]
            topic_prefix: TopicPrefix,

            /// The amount of time after which the bridge should send a ping if no other traffic has occurred
            #[tedge_config(example = "60s", default(from_str = "60s"))]
            keepalive_interval: SecondsOrHumanTime,

        },

        entity_store: {
            /// Enable auto registration feature
            #[tedge_config(example = "true", default(value = true))]
            auto_register: bool,

            /// On a clean start, the whole state of the device, services and child-devices is resent to the cloud
            #[tedge_config(example = "true", default(value = true))]
            clean_start: bool,
        },

        software_management: {
            /// Switch legacy or advanced software management API to use. Value: legacy or advanced
            #[tedge_config(example = "advanced", default(variable = "SoftwareManagementApiFlag::Legacy"))]
            api: SoftwareManagementApiFlag,

            /// Enable publishing c8y_SupportedSoftwareTypes fragment to the c8y inventory API
            #[tedge_config(example = "true", default(value = false))]
            with_types: bool,
        },

        operations: {
            /// Auto-upload the operation log once it finishes.
            #[tedge_config(example = "always", example = "never", example = "on-failure", default(variable = "AutoLogUpload::OnFailure"))]
            auto_log_upload: AutoLogUpload,
        },

        availability: {
            /// Enable sending heartbeat to Cumulocity periodically. If set to false, c8y_RequiredAvailability won't be sent
            #[tedge_config(example = "true", default(value = true))]
            enable: bool,

            /// Heartbeat interval to be sent to Cumulocity as c8y_RequiredAvailability.
            /// The value must be greater than 1 minute.
            /// If set to a lower value or 0, the device is considered in maintenance mode in the Cumulocity context.
            /// Details: https://cumulocity.com/docs/device-integration/fragment-library/#device-availability
            #[tedge_config(example = "60m", default(from_str = "60m"))]
            interval: SecondsOrHumanTime,
        },
    },

    #[tedge_config(deprecated_name = "azure")] // for 0.1.0 compatibility
    #[tedge_config(multi)]
    az: {
        /// Endpoint URL of Azure IoT tenant
        #[tedge_config(example = "myazure.azure-devices.net")]
        url: ConnectUrl,

        /// The path where Azure IoT root certificate(s) are stored
        #[tedge_config(note = "The value can be a directory path as well as the path of the certificate file.")]
        #[tedge_config(example = "/etc/tedge/az-trusted-root-certificates.pem", default(function = "default_root_cert_path"))]
        root_cert_path: AbsolutePath,

        device: {
            /// Identifier of the device within the fleet. It must be globally
            /// unique and is derived from the device certificate.
            #[tedge_config(reader(function = "az_device_id"))]
            #[tedge_config(default(from_optional_key = "device.id"))]
            #[tedge_config(example = "Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665")]
            #[doku(as = "String")]
            id: Result<String, ReadError>,

            /// Path where the device's private key is stored
            #[tedge_config(example = "/etc/tedge/device-certs/tedge-private-key.pem", default(from_key = "device.key_path"))]
            key_path: AbsolutePath,

            /// Path where the device's certificate is stored
            #[tedge_config(example = "/etc/tedge/device-certs/tedge-certificate.pem", default(from_key = "device.cert_path"))]
            cert_path: AbsolutePath,

            /// Path where the device's certificate signing request is stored
            #[tedge_config(example = "/etc/tedge/device-certs/tedge.csr", default(from_key = "device.csr_path"))]
            csr_path: AbsolutePath,

            /// A PKCS#11 URI of the private key.
            ///
            /// See RFC #7512.
            #[tedge_config(example = "pkcs11:token=my-pkcs11-token;object=my-key")]
            key_uri: Arc<str>,
        },

        mapper: {
            /// Whether the Azure IoT mapper should add a timestamp or not
            #[tedge_config(example = "true")]
            #[tedge_config(default(value = true))]
            timestamp: bool,

            /// The format that will be used by the mapper when sending timestamps to Azure IoT
            #[tedge_config(example = "rfc-3339")]
            #[tedge_config(example = "unix")]
            #[tedge_config(default(variable = "TimeFormat::Unix"))]
            timestamp_format: TimeFormat,

            mqtt: {
                /// The maximum message payload size that can be mapped to the cloud via MQTT
                #[tedge_config(example = "262144", default(function = "az_mqtt_payload_limit"))]
                max_payload_size: MqttPayloadLimit,
            }
        },

        bridge: {
            /// The topic prefix that will be used for the bridge MQTT topic. For instance,
            /// if this is set to "az", then messages published to `az/twin/GET/#` will be
            /// forwarded to Azure on the `$iothub/twin/GET/#` topic
            #[tedge_config(example = "az", default(function = "az_topic_prefix"))]
            topic_prefix: TopicPrefix,

            /// The amount of time after which the bridge should send a ping if no other traffic has occurred
            #[tedge_config(example = "60s", default(from_str = "60s"))]
            keepalive_interval: SecondsOrHumanTime,
        },

        /// Set of MQTT topics the Azure IoT mapper should subscribe to
        #[tedge_config(example = "te/+/+/+/+/a/+,te/+/+/+/+/m/+,te/+/+/+/+/e/+")]
        #[tedge_config(default(value = "te/+/+/+/+/m/+,te/+/+/+/+/e/+,te/+/+/+/+/a/+,te/+/+/+/+/status/health"))]
        topics: TemplatesSet,
    },

    #[tedge_config(multi)]
    aws: {
        /// Endpoint URL of AWS IoT tenant
        #[tedge_config(example = "your-endpoint.amazonaws.com")]
        url: ConnectUrl,

        /// The path where AWS IoT root certificate(s) are stored
        #[tedge_config(note = "The value can be a directory path as well as the path of the certificate file.")]
        #[tedge_config(example = "/etc/tedge/aws-trusted-root-certificates.pem", default(function = "default_root_cert_path"))]
        root_cert_path: AbsolutePath,

        device: {
            /// Identifier of the device within the fleet. It must be globally
            /// unique and is derived from the device certificate.
            #[tedge_config(reader(function = "aws_device_id"))]
            #[tedge_config(default(from_optional_key = "device.id"))]
            #[tedge_config(example = "Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665")]
            #[doku(as = "String")]
            id: Result<String, ReadError>,

            /// Path where the device's private key is stored
            #[tedge_config(example = "/etc/tedge/device-certs/tedge-private-key.pem", default(from_key = "device.key_path"))]
            key_path: AbsolutePath,

            /// Path where the device's certificate is stored
            #[tedge_config(example = "/etc/tedge/device-certs/tedge-certificate.pem", default(from_key = "device.cert_path"))]
            cert_path: AbsolutePath,

            /// Path where the device's certificate signing request is stored
            #[tedge_config(example = "/etc/tedge/device-certs/tedge.csr", default(from_key = "device.csr_path"))]
            csr_path: AbsolutePath,

            /// A PKCS#11 URI of the private key.
            ///
            /// See RFC #7512.
            #[tedge_config(example = "pkcs11:model=PKCS%2315%20emulated")]
            key_uri: Arc<str>,
        },

        mapper: {
            /// Whether the AWS IoT mapper should add a timestamp or not
            #[tedge_config(example = "true")]
            #[tedge_config(default(value = true))]
            timestamp: bool,

            /// The format that will be used by the mapper when sending timestamps to AWS IoT
            #[tedge_config(example = "rfc-3339")]
            #[tedge_config(example = "unix")]
            #[tedge_config(default(variable = "TimeFormat::Unix"))]
            timestamp_format: TimeFormat,

            mqtt: {
                /// The maximum message payload size that can be mapped to the cloud via MQTT
                #[tedge_config(example = "131072", default(function = "aws_mqtt_payload_limit"))]
                max_payload_size: MqttPayloadLimit,
            }
        },

        bridge: {
            /// The topic prefix that will be used for the bridge MQTT topic. For instance,
            /// if this is set to "aws", then messages published to `aws/shadow/#` will be
            /// forwarded to AWS on the `$aws/things/shadow/#` topic
            #[tedge_config(example = "aws", default(function = "aws_topic_prefix"))]
            topic_prefix: TopicPrefix,


            /// The amount of time after which the bridge should send a ping if no other traffic has occurred
            #[tedge_config(example = "60s", default(from_str = "60s"))]
            keepalive_interval: SecondsOrHumanTime,
        },

        /// Set of MQTT topics the AWS IoT mapper should subscribe to
        #[tedge_config(example = "te/+/+/+/+/a/+,te/+/+/+/+/m/+,te/+/+/+/+/e/+")]
        #[tedge_config(default(value = "te/+/+/+/+/m/+,te/+/+/+/+/e/+,te/+/+/+/+/a/+,te/+/+/+/+/status/health"))]
        topics: TemplatesSet,
    },

    mqtt: {
        /// MQTT topic root
        #[tedge_config(default(value = "te"))]
        #[tedge_config(example = "te")]
        topic_root: String,

        /// The device MQTT topic identifier
        #[tedge_config(default(value = "device/main//"))]
        #[tedge_config(example = "device/main//")]
        #[tedge_config(example = "device/child_001//")]
        device_topic_id: String,

        bind: {
            /// The address mosquitto binds to for internal use
            #[tedge_config(example = "127.0.0.1", default(variable = "Ipv4Addr::LOCALHOST"))]
            address: IpAddr,

            /// The port mosquitto binds to for internal use
            #[tedge_config(example = "1883", default(function = "default_mqtt_port"), deprecated_key = "mqtt.port")]
            #[doku(as = "u16")]
            // This was originally u16, but I can't think of any way in which
            // tedge could actually connect to mosquitto if it bound to a random
            // free port, so I don't think 0 is *really* valid here
            port: NonZeroU16,
        },

        client: {
            /// The host that the thin-edge MQTT client should connect to
            #[tedge_config(example = "127.0.0.1", default(value = "127.0.0.1"))]
            host: String,

            /// The port that the thin-edge MQTT client should connect to
            #[tedge_config(default(from_key = "mqtt.bind.port"))]
            #[doku(as = "u16")]
            port: NonZeroU16,

            #[tedge_config(reader(private))]
            auth: {
                /// Path to the CA certificate used by MQTT clients to use when authenticating the MQTT broker
                #[tedge_config(example = "/etc/mosquitto/ca_certificates/ca.crt")]
                #[tedge_config(deprecated_name = "cafile")]
                ca_file: AbsolutePath,

                /// Path to the directory containing the CA certificates used by MQTT
                /// clients when authenticating the MQTT broker
                #[tedge_config(example = "/etc/mosquitto/ca_certificates")]
                #[tedge_config(deprecated_name = "cadir")]
                ca_dir: AbsolutePath,

                /// Path to the client certificate
                #[tedge_config(example = "/etc/mosquitto/auth_certificates/cert.pem")]
                #[tedge_config(deprecated_name = "certfile")]
                cert_file: AbsolutePath,

                /// Path to the client private key
                #[tedge_config(example = "/etc/mosquitto/auth_certificates/key.pem")]
                #[tedge_config(deprecated_name = "keyfile")]
                key_file: AbsolutePath,
            }
        },

        external: {
            bind: {
                /// The port mosquitto binds to for external use
                #[tedge_config(example = "8883", deprecated_key = "mqtt.external.port")]
                port: u16,

                /// The address mosquitto binds to for external use
                #[tedge_config(example = "0.0.0.0")]
                address: IpAddr,

                /// Name of the network interface which mosquitto limits incoming connections on
                #[tedge_config(example = "wlan0")]
                interface: String,
            },

            /// Path to a file containing the PEM encoded CA certificates that are
            /// trusted when checking incoming client certificates
            #[tedge_config(example = "/etc/ssl/certs")]
            #[tedge_config(deprecated_key = "mqtt.external.capath")]
            ca_path: AbsolutePath,

            /// Path to the certificate file which is used by the external MQTT listener
            #[tedge_config(note = "This setting shall be used together with `mqtt.external.key_file` for external connections.")]
            #[tedge_config(example = "/etc/tedge/device-certs/tedge-certificate.pem")]
            #[tedge_config(deprecated_key = "mqtt.external.certfile")]
            cert_file: AbsolutePath,

            /// Path to the key file which is used by the external MQTT listener
            #[tedge_config(note = "This setting shall be used together with `mqtt.external.cert_file` for external connections.")]
            #[tedge_config(example = "/etc/tedge/device-certs/tedge-private-key.pem")]
            #[tedge_config(deprecated_key = "mqtt.external.keyfile")]
            key_file: AbsolutePath,
        },

        bridge: {
            #[tedge_config(default(value = false))]
            #[tedge_config(example = "false")]
            #[tedge_config(note = "After changing this value, run `tedge reconnect <cloud>` to apply the changes")]
            /// Enables the built-in bridge when running tedge-mapper
            built_in: bool,

            reconnect_policy: {
                /// The minimum time the built-in bridge will wait before reconnecting
                #[tedge_config(example = "30s", default(from_str = "30s"))]
                initial_interval: SecondsOrHumanTime,

                /// The maximum time the built-in bridge will wait before reconnecting
                #[tedge_config(example = "10m", default(from_str = "10m"))]
                maximum_interval: SecondsOrHumanTime,

                /// How long to wait after successful reconnection before resetting the reconnect timeout
                #[tedge_config(example = "5m", default(from_str = "5m"))]
                reset_window: SecondsOrHumanTime,
            },
        },
    },

    http: {
        bind: {
            /// The port number of the File Transfer Service HTTP server binds to for internal use
            #[tedge_config(example = "8000", default(value = 8000u16), deprecated_key = "http.port")]
            port: u16,

            /// The address of the File Transfer Service HTTP server binds to for internal use
            #[tedge_config(default(function = "default_http_bind_address"), deprecated_key = "http.address")]
            #[tedge_config(example = "127.0.0.1", example = "192.168.1.2", example = "0.0.0.0")]
            address: IpAddr,
        },

        client: {
            /// The port number on the remote host on which the File Transfer Service HTTP server is running
            #[tedge_config(example = "8000", default(value = 8000u16))]
            port: u16,

            /// The address of the host on which the File Transfer Service HTTP server is running
            #[tedge_config(default(value = "127.0.0.1"))]
            #[tedge_config(example = "127.0.0.1", example = "192.168.1.2", example = "tedge-hostname")]
            host: Arc<str>,

            auth: {
                /// Path to the certificate which is used by the agent when connecting to external services
                #[tedge_config(reader(private))]
                cert_file: AbsolutePath,

                /// Path to the private key which is used by the agent when connecting to external services
                #[tedge_config(reader(private))]
                key_file: AbsolutePath,
            },
        },

        /// The file that will be used as the server certificate for the File Transfer Service
        #[tedge_config(example = "/etc/tedge/device-certs/file_transfer_certificate.pem")]
        cert_path: AbsolutePath,

        /// The file that will be used as the server private key for the File Transfer Service
        #[tedge_config(example = "/etc/tedge/device-certs/file_transfer_key.pem")]
        key_path: AbsolutePath,

        /// Path to a directory containing the PEM encoded CA certificates that are
        /// trusted when checking incoming client certificates for the File Transfer Service
        #[tedge_config(example = "/etc/ssl/certs")]
        ca_path: AbsolutePath,
    },

    agent: {
        state: {
            /// The directory where the tedge-agent persists its state across restarts
            #[tedge_config(note = "If the given directory doesn't exists, `/etc/tedge/.agent` is used as a fallback irrespective of the current setting.")]
            #[tedge_config(default(from_str = "/data/tedge/agent"))]
            #[tedge_config(example = "/data/tedge/agent")]
            path: AbsolutePath,
        },

        enable: {
            /// Determines if tedge-agent should enable config_update operation
            #[tedge_config(example = "true", default(value = true))]
            config_update: bool,

            /// Determines if tedge-agent should enable config_snapshot operation
            #[tedge_config(example = "true", default(value = true))]
            config_snapshot: bool,

            /// Determines if tedge-agent should enable log_upload operation
            #[tedge_config(example = "true", default(value = true))]
            log_upload: bool,
        },

        entity_store: {
            /// Enable auto registration feature
            #[tedge_config(example = "true", default(value = true), deprecated_key = "c8y.entity_store.auto_register")]
            auto_register: bool,

            /// On a clean start, the whole state of the device, services and child-devices is resent to the cloud
            #[tedge_config(example = "true", default(value = true), deprecated_key = "c8y.entity_store.clean_start")]
            clean_start: bool,
        },


    },

    software: {
        plugin: {
            /// The default software plugin to be used for software management on the device
            #[tedge_config(example = "apt")]
            default: String,

            /// The maximum number of software packages reported for each type of software package
            #[tedge_config(example = "1000", default(value = 1000u32))]
            max_packages: u32,

            /// The filtering criterion, in form of regex, that is used to filter packages list output
            #[tedge_config(example = "^(tedge|c8y).*")]
            include: String,

            /// The filtering criterion, in form of regex, that is used to filter out packages from the output list
            #[tedge_config(example = "^(glibc|lib|kernel-|iptables-module).*")]
            exclude: String,
        }
    },

    run: {
        /// The directory used to store runtime information, such as file locks
        #[tedge_config(example = "/run", default(from_str = "/run"))]
        path: AbsolutePath,

        /// Whether to create a lock file or not
        #[tedge_config(example = "true", default(value = true))]
        lock_files: bool,

        /// Interval at which the memory usage is logged (in seconds if no unit is provided). Logging is disabled if set to 0
        #[tedge_config(example = "60s", default(from_str = "0"))]
        log_memory_interval: SecondsOrHumanTime,
    },

    logs: {
        /// The directory used to store logs
        #[tedge_config(example = "/var/log/tedge", default(from_str = "/var/log/tedge"))]
        path: AbsolutePath,
    },

    tmp: {
        /// The temporary directory used to download files to the device
        #[tedge_config(example = "/tmp", default(from_str = "/tmp"))]
        path: AbsolutePath,
    },

    data: {
        /// The directory used to store data like cached files, runtime metadata, etc.
        #[tedge_config(example = "/var/tedge", default(from_str = "/var/tedge"))]
        path: AbsolutePath,
    },

    firmware: {
        child: {
            update: {
                /// The timeout limit in seconds for firmware update operations on child devices
                #[tedge_config(example = "1h", default(from_str = "1h"))]
                timeout: SecondsOrHumanTime,
            }
        }
    },

    service: {
        /// The thin-edge.io service's service type
        #[tedge_config(rename = "type", example = "systemd", default(value = "service"))]
        ty: String,

        /// The format that will be used for the timestamp when generating service "up" messages in thin-edge JSON
        #[tedge_config(example = "rfc-3339")]
        #[tedge_config(example = "unix")]
        #[tedge_config(default(variable = "TimeFormat::Unix"))]
        timestamp_format: TimeFormat,
    },

    apt: {
        /// The filtering criterion that is used to filter packages list output by name
        #[tedge_config(example = "tedge.*")]
        name: String,
        /// The filtering criterion that is used to filter packages list output by maintainer
        #[tedge_config(example = "thin-edge.io team.*")]
        maintainer: String,

        dpk: {
            options: {
                /// dpkg configuration option used to control the dpkg options "--force-confold" and
                /// "--force-confnew" and are applied when installing apt packages via the tedge-apt-plugin.
                /// Accepts either 'keepold' or 'keepnew'.
                #[tedge_config(note = "If set to 'keepold', this keeps the old configuration files of the package that is being installed")]
                #[tedge_config(example = "keepold", example = "keepnew", default(variable = "AptConfig::KeepOld"))]
                config: AptConfig,
            }
        },
    },

    sudo: {
        /// Determines if thin-edge should use `sudo` when attempting to write to files possibly
        /// not owned by `tedge`.
        #[tedge_config(default(value = true), example = "true", example = "false")]
        enable: bool,
    },

    proxy: {
        /// The address (scheme://address:port) of an HTTP CONNECT proxy to use
        /// when connecting to external HTTP/MQTT services
        #[doku(as = "String")]
        address: ProxyUrl,

        /// The username for the proxy connection to the cloud MQTT broker
        username: String,

        /// The password for the proxy connection to the cloud MQTT broker
        password: String,

        /// The "no-proxy" configuration, a comma-separated list of hosts to
        /// bypass the configured proxy for
        #[tedge_config(example = "127.0.0.1,example.com,192.168.1.0/24")]
        no_proxy: String,
    },
}

static CLOUD_ROOT_CERTIFICATES: OnceLock<Arc<[Certificate]>> = OnceLock::new();

impl TEdgeConfigReader {
    pub fn cloud_root_certs(&self) -> anyhow::Result<CloudHttpConfig> {
        let roots = CLOUD_ROOT_CERTIFICATES.get_or_init(|| {
            let c8y_roots = self.c8y.entries().flat_map(|(key, c8y)| {
                read_trust_store(&c8y.root_cert_path).unwrap_or_else(move |e| {
                    error!(
                        "Unable to read certificates from {}: {e:?}",
                        ReadableKey::C8yRootCertPath(key.map(<_>::to_owned))
                    );
                    vec![]
                })
            });
            let az_roots = self.az.entries().flat_map(|(key, az)| {
                read_trust_store(&az.root_cert_path).unwrap_or_else(move |e| {
                    error!(
                        "Unable to read certificates from {}: {e:?}",
                        ReadableKey::AzRootCertPath(key.map(<_>::to_owned))
                    );
                    vec![]
                })
            });
            let aws_roots = self.aws.entries().flat_map(|(key, aws)| {
                read_trust_store(&aws.root_cert_path).unwrap_or_else(move |e| {
                    error!(
                        "Unable to read certificates from {}: {e:?}",
                        ReadableKey::AwsRootCertPath(key.map(<_>::to_owned))
                    );
                    vec![]
                })
            });
            c8y_roots.chain(az_roots).chain(aws_roots).collect()
        });

        let proxy = if let Some(address) = self.proxy.address.or_none() {
            let url = address.url();
            let no_proxy = self
                .proxy
                .no_proxy
                .or_none()
                .and_then(|s| NoProxy::from_string(s))
                .or_else(NoProxy::from_env);
            let mut proxy = reqwest::Proxy::all(url)
                .context("Failed to configure HTTP proxy connection")?
                .no_proxy(no_proxy);
            if let Some((username, password)) =
                all_or_nothing((self.proxy.username.as_ref(), self.proxy.password.as_ref()))
                    .map_err(|e| anyhow::anyhow!("{}", e))?
            {
                proxy = proxy.basic_auth(username, password)
            }
            Some(proxy)
        } else {
            None
        };

        Ok(CloudHttpConfig::new(roots.clone(), proxy))
    }

    pub fn cloud_client_tls_config(&self) -> rustls::ClientConfig {
        // TODO do we want to unwrap here?
        client_config_for_ca_certificates(
            self.c8y
                .values()
                .map(|c8y| &c8y.root_cert_path)
                .chain(self.az.values().map(|az| &az.root_cert_path))
                .chain(self.aws.values().map(|aws| &aws.root_cert_path)),
        )
        .unwrap()
    }

    pub fn device_key_path<'a>(
        &self,
        cloud: Option<impl Into<Cloud<'a>>>,
    ) -> Result<&Utf8Path, MultiError> {
        Ok(match cloud.map(<_>::into) {
            None => &self.device.key_path,
            Some(Cloud::C8y(profile)) => &self.c8y.try_get(profile)?.device.key_path,
            Some(Cloud::Az(profile)) => &self.az.try_get(profile)?.device.key_path,
            Some(Cloud::Aws(profile)) => &self.aws.try_get(profile)?.device.key_path,
        })
    }

    pub fn device_cert_path<'a>(
        &self,
        cloud: Option<impl Into<Cloud<'a>>>,
    ) -> Result<&Utf8Path, MultiError> {
        Ok(match cloud.map(<_>::into) {
            None => &self.device.cert_path,
            Some(Cloud::C8y(profile)) => &self.c8y.try_get(profile)?.device.cert_path,
            Some(Cloud::Az(profile)) => &self.az.try_get(profile)?.device.cert_path,
            Some(Cloud::Aws(profile)) => &self.aws.try_get(profile)?.device.cert_path,
        })
    }

    pub fn device_csr_path<'a>(
        &self,
        cloud: Option<impl Into<Cloud<'a>>>,
    ) -> Result<&Utf8Path, MultiError> {
        Ok(match cloud.map(<_>::into) {
            None => &self.device.csr_path,
            Some(Cloud::C8y(profile)) => &self.c8y.try_get(profile)?.device.csr_path,
            Some(Cloud::Az(profile)) => &self.az.try_get(profile)?.device.csr_path,
            Some(Cloud::Aws(profile)) => &self.aws.try_get(profile)?.device.csr_path,
        })
    }

    pub fn device_id<'a>(&self, cloud: Option<impl Into<Cloud<'a>>>) -> Result<&str, ReadError> {
        Ok(match cloud.map(<_>::into) {
            None => self.device.id()?,
            Some(Cloud::C8y(profile)) => self.c8y.try_get(profile)?.device.id()?,
            Some(Cloud::Az(profile)) => self.az.try_get(profile)?.device.id()?,
            Some(Cloud::Aws(profile)) => self.aws.try_get(profile)?.device.id()?,
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Cloud<'a> {
    C8y(Option<&'a ProfileName>),
    Az(Option<&'a ProfileName>),
    Aws(Option<&'a ProfileName>),
}

pub trait CloudConfig {
    fn device_key_path(&self) -> &Utf8Path;
    fn device_cert_path(&self) -> &Utf8Path;
    fn root_cert_path(&self) -> &Utf8Path;
    fn key_uri(&self) -> Option<Arc<str>>;
}

impl CloudConfig for TEdgeConfigReaderC8y {
    fn device_key_path(&self) -> &Utf8Path {
        &self.device.key_path
    }

    fn device_cert_path(&self) -> &Utf8Path {
        &self.device.cert_path
    }

    fn root_cert_path(&self) -> &Utf8Path {
        &self.root_cert_path
    }

    fn key_uri(&self) -> Option<Arc<str>> {
        self.device.key_uri.or_none().cloned()
    }
}

impl CloudConfig for TEdgeConfigReaderAz {
    fn device_key_path(&self) -> &Utf8Path {
        &self.device.key_path
    }

    fn device_cert_path(&self) -> &Utf8Path {
        &self.device.cert_path
    }

    fn root_cert_path(&self) -> &Utf8Path {
        &self.root_cert_path
    }

    fn key_uri(&self) -> Option<Arc<str>> {
        self.device.key_uri.or_none().cloned()
    }
}

impl CloudConfig for TEdgeConfigReaderAws {
    fn device_key_path(&self) -> &Utf8Path {
        &self.device.key_path
    }

    fn device_cert_path(&self) -> &Utf8Path {
        &self.device.cert_path
    }

    fn root_cert_path(&self) -> &Utf8Path {
        &self.root_cert_path
    }

    fn key_uri(&self) -> Option<Arc<str>> {
        self.device.key_uri.or_none().cloned()
    }
}

fn c8y_topic_prefix() -> TopicPrefix {
    TopicPrefix::try_new("c8y").unwrap()
}

fn az_topic_prefix() -> TopicPrefix {
    TopicPrefix::try_new("az").unwrap()
}

fn aws_topic_prefix() -> TopicPrefix {
    TopicPrefix::try_new("aws").unwrap()
}

fn c8y_mqtt_payload_limit() -> MqttPayloadLimit {
    C8Y_MQTT_PAYLOAD_LIMIT.try_into().unwrap()
}

fn az_mqtt_payload_limit() -> MqttPayloadLimit {
    AZ_MQTT_PAYLOAD_LIMIT.try_into().unwrap()
}

fn aws_mqtt_payload_limit() -> MqttPayloadLimit {
    AWS_MQTT_PAYLOAD_LIMIT.try_into().unwrap()
}

fn default_http_bind_address(dto: &TEdgeConfigDto) -> IpAddr {
    let external_address = dto.mqtt.external.bind.address;
    external_address
        .or(dto.mqtt.bind.address)
        .unwrap_or(Ipv4Addr::LOCALHOST.into())
}

fn device_id_from_cert(cert_path: &Utf8Path) -> Result<String, ReadError> {
    let pem = PemCertificate::from_pem_file(cert_path)
        .map_err(|err| cert_error_into_config_error(ReadableKey::DeviceId.to_cow_str(), err))?;
    let device_id = pem
        .subject_common_name()
        .map_err(|err| cert_error_into_config_error(ReadableKey::DeviceId.to_cow_str(), err))?;
    Ok(device_id)
}

fn device_id(
    device: &TEdgeConfigReaderDevice,
    dto_value: &OptionalConfig<String>,
) -> Result<String, ReadError> {
    match (device_id_from_cert(&device.cert_path), dto_value.or_none()) {
        (Ok(common_name), _) => Ok(common_name),
        (Err(_), Some(dto_value)) => Ok(dto_value.to_string()),
        (Err(err), None) => Err(err),
    }
}

fn c8y_device_id(
    c8y_device: &TEdgeConfigReaderC8yDevice,
    dto_value: &OptionalConfig<String>,
) -> Result<String, ReadError> {
    match (
        device_id_from_cert(&c8y_device.cert_path),
        dto_value.or_none(),
    ) {
        (Ok(common_name), _) => Ok(common_name),
        (Err(_), Some(dto_value)) => Ok(dto_value.to_string()),
        (Err(err), None) => Err(err),
    }
}

fn az_device_id(
    az_device: &TEdgeConfigReaderAzDevice,
    dto_value: &OptionalConfig<String>,
) -> Result<String, ReadError> {
    match (
        device_id_from_cert(&az_device.cert_path),
        dto_value.or_none(),
    ) {
        (Ok(common_name), _) => Ok(common_name),
        (Err(_), Some(dto_value)) => Ok(dto_value.to_string()),
        (Err(err), None) => Err(err),
    }
}

fn aws_device_id(
    aws_device: &TEdgeConfigReaderAwsDevice,
    dto_value: &OptionalConfig<String>,
) -> Result<String, ReadError> {
    match (
        device_id_from_cert(&aws_device.cert_path),
        dto_value.or_none(),
    ) {
        (Ok(common_name), _) => Ok(common_name),
        (Err(_), Some(dto_value)) => Ok(dto_value.to_string()),
        (Err(err), None) => Err(err),
    }
}

fn cert_error_into_config_error(key: Cow<'static, str>, err: CertificateError) -> ReadError {
    match &err {
        CertificateError::IoError { error, .. } => match error.kind() {
            std::io::ErrorKind::NotFound => ReadError::ReadOnlyNotFound {
                key,
                message: concat!(
                    "The device id is read from the device certificate.\n",
                    "To set 'device.id' to some <id>, you can use `tedge cert create --device-id <id>`.",
                ),
            },
            _ => ReadError::DerivationFailed {
                key,
                cause: format!("{}", err),
            },
        },
        _ => ReadError::DerivationFailed {
            key,
            cause: format!("{}", err),
        },
    }
}

fn default_root_cert_path(_location: &TEdgeConfigLocation) -> AbsolutePath {
    AbsolutePath::try_new(DEFAULT_ROOT_CERT_PATH).unwrap()
}

fn default_device_key(location: &TEdgeConfigLocation) -> AbsolutePath {
    location
        .tedge_config_root_path()
        .join("device-certs")
        .join("tedge-private-key.pem")
        .try_into()
        .unwrap()
}

fn default_device_cert(location: &TEdgeConfigLocation) -> AbsolutePath {
    location
        .tedge_config_root_path()
        .join("device-certs")
        .join("tedge-certificate.pem")
        .try_into()
        .unwrap()
}

fn default_device_csr(location: &TEdgeConfigLocation) -> AbsolutePath {
    location
        .tedge_config_root_path()
        .join("device-certs")
        .join("tedge.csr")
        .try_into()
        .unwrap()
}

fn default_credentials_path(location: &TEdgeConfigLocation) -> AbsolutePath {
    location
        .tedge_config_root_path()
        .join("credentials.toml")
        .try_into()
        .unwrap()
}

fn default_mqtt_port() -> NonZeroU16 {
    NonZeroU16::try_from(1883).unwrap()
}

impl TEdgeConfigReaderMqttBridgeReconnectPolicy {
    /// Designed for injecting into tests without requiring a full [TEdgeConfig]
    pub fn test_value() -> Self {
        Self {
            initial_interval: "0".parse().unwrap(),
            maximum_interval: "10m".parse().unwrap(),
            reset_window: "15m".parse().unwrap(),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ReadError {
    #[error(transparent)]
    ConfigNotSet(#[from] ConfigNotSet),

    #[error(transparent)]
    Multi(#[from] MultiError),

    #[error("Config value {key}, cannot be read: {message} ")]
    ReadOnlyNotFound {
        key: Cow<'static, str>,
        message: &'static str,
    },

    #[error("Derivation for `{key}` failed: {cause}")]
    DerivationFailed {
        key: Cow<'static, str>,
        cause: String,
    },
}

/// An abstraction over the possible default functions for tedge config values
///
/// Some configuration defaults are relative to the config location, and
/// this trait allows us to pass that in, or the DTO, both, or neither!
pub trait TEdgeConfigDefault<T, Args> {
    type Output;
    fn call(self, data: &T, location: &TEdgeConfigLocation) -> Self::Output;
}

impl<F, Out, T> TEdgeConfigDefault<T, ()> for F
where
    F: FnOnce() -> Out + Clone,
{
    type Output = Out;
    fn call(self, _: &T, _: &TEdgeConfigLocation) -> Self::Output {
        (self)()
    }
}

impl<F, Out, T> TEdgeConfigDefault<T, &T> for F
where
    F: FnOnce(&T) -> Out + Clone,
{
    type Output = Out;
    fn call(self, data: &T, _location: &TEdgeConfigLocation) -> Self::Output {
        (self)(data)
    }
}

impl<F, Out, T> TEdgeConfigDefault<T, (&TEdgeConfigLocation,)> for F
where
    F: FnOnce(&TEdgeConfigLocation) -> Out + Clone,
{
    type Output = Out;
    fn call(self, _data: &T, location: &TEdgeConfigLocation) -> Self::Output {
        (self)(location)
    }
}

impl<F, Out, T> TEdgeConfigDefault<T, (&T, &TEdgeConfigLocation)> for F
where
    F: FnOnce(&T, &TEdgeConfigLocation) -> Out + Clone,
{
    type Output = Out;
    fn call(self, data: &T, location: &TEdgeConfigLocation) -> Self::Output {
        (self)(data, location)
    }
}

impl TEdgeConfigReaderHttp {
    pub fn client_tls_config(&self) -> anyhow::Result<rustls::ClientConfig> {
        let client_cert_key = crate::all_or_nothing((
            self.client.auth.key_file.as_ref(),
            self.client.auth.cert_file.as_ref(),
        ))
        .map_err(|e| anyhow!("{e}"))?;

        let root_certificates = self
            .ca_path
            .or_none()
            .map_or(DEFAULT_ROOT_CERT_PATH, |ca| ca.as_str());

        client_cert_key
            .map(|(key, cert)| create_tls_config(root_certificates, key, cert))
            .unwrap_or_else(|| create_tls_config_without_client_cert(root_certificates))
            .map_err(|e| anyhow!("{e}"))
    }
}

impl TEdgeConfigReaderHttpClientAuth {
    pub fn identity(&self) -> anyhow::Result<Option<reqwest::Identity>> {
        use ReadableKey::*;

        let client_cert_key =
            crate::all_or_nothing((self.cert_file.as_ref(), self.key_file.as_ref()))
                .map_err(|e| anyhow!("{e}"))?;

        Ok(match client_cert_key {
            Some((cert, key)) => {
                let mut pem = std::fs::read(key).with_context(|| {
                    format!("reading private key (from {HttpClientAuthKeyFile}): {key}")
                })?;
                let mut cert_file = std::fs::File::open(cert).with_context(|| {
                    format!("opening certificate (from {HttpClientAuthCertFile}): {cert}")
                })?;
                cert_file.read_to_end(&mut pem).with_context(|| {
                    format!("reading certificate (from {HttpClientAuthCertFile}): {cert}")
                })?;

                Some(reqwest::Identity::from_pem(&pem)?)
            }
            None => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case::test_case("device.id")]
    #[test_case::test_case("device.type")]
    #[test_case::test_case("device.key.path")]
    #[test_case::test_case("device.cert.path")]
    #[test_case::test_case("c8y.url")]
    #[test_case::test_case("c8y.root.cert.path")]
    #[test_case::test_case("c8y.smartrest.templates")]
    #[test_case::test_case("az.url")]
    #[test_case::test_case("az.root.cert.path")]
    #[test_case::test_case("aws.url")]
    #[test_case::test_case("aws.root.cert.path")]
    #[test_case::test_case("aws.mapper.timestamp")]
    #[test_case::test_case("az.mapper.timestamp")]
    #[test_case::test_case("mqtt.bind_address")]
    #[test_case::test_case("http.address")]
    #[test_case::test_case("mqtt.client.host")]
    #[test_case::test_case("mqtt.client.port")]
    #[test_case::test_case("mqtt.client.auth.cafile")]
    #[test_case::test_case("mqtt.client.auth.cadir")]
    #[test_case::test_case("mqtt.client.auth.certfile")]
    #[test_case::test_case("mqtt.client.auth.keyfile")]
    #[test_case::test_case("mqtt.port")]
    #[test_case::test_case("http.port")]
    #[test_case::test_case("mqtt.external.port")]
    #[test_case::test_case("mqtt.external.bind_address")]
    #[test_case::test_case("mqtt.external.bind_interface")]
    #[test_case::test_case("mqtt.external.capath")]
    #[test_case::test_case("mqtt.external.certfile")]
    #[test_case::test_case("mqtt.external.keyfile")]
    #[test_case::test_case("software.plugin.default")]
    #[test_case::test_case("software.plugin.exclude")]
    #[test_case::test_case("software.plugin.include")]
    #[test_case::test_case("software.plugin.max_packages")]
    #[test_case::test_case("tmp.path")]
    #[test_case::test_case("logs.path")]
    #[test_case::test_case("run.path")]
    #[test_case::test_case("data.path")]
    #[test_case::test_case("firmware.child.update.timeout")]
    #[test_case::test_case("service.type")]
    #[test_case::test_case("run.lock_files")]
    #[test_case::test_case("apt.name")]
    #[test_case::test_case("apt.maintainer")]
    fn all_0_10_keys_can_be_deserialised(key: &str) {
        key.parse::<ReadableKey>().unwrap();
    }

    #[test]
    fn missing_c8y_http_directs_user_towards_setting_c8y_url() {
        let dto = TEdgeConfigDto::default();

        let reader = TEdgeConfigReader::from_dto(&dto, &TEdgeConfigLocation::default());

        assert_eq!(
            reader.c8y.try_get::<str>(None).unwrap().http.key(),
            "c8y.url"
        );
    }
}
