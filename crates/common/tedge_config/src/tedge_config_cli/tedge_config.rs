use super::models::timestamp::TimeFormat;
use crate::AptConfig;
use crate::AutoFlag;
use crate::AutoLogUpload;
use crate::ConnectUrl;
use crate::HostPort;
use crate::SecondsOrHumanTime;
use crate::SoftwareManagementApiFlag;
use crate::TEdgeConfigLocation;
use crate::TemplatesSet;
use crate::HTTPS_PORT;
use crate::MQTT_TLS_PORT;
use anyhow::anyhow;
use anyhow::ensure;
use anyhow::Context;
use camino::Utf8PathBuf;
use certificate::parse_root_certificate::client_config_for_ca_certificates;
use certificate::parse_root_certificate::create_tls_config;
use certificate::read_trust_store;
use certificate::CertificateError;
use certificate::CloudRootCerts;
use certificate::PemCertificate;
use doku::Document;
use doku::Type;
use once_cell::sync::Lazy;
use reqwest::Certificate;
use serde::Deserialize;
use std::borrow::Cow;
use std::fmt;
use std::fmt::Formatter;
use std::io::Read;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::num::NonZeroU16;
use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::OnceLock;
use tedge_config_macros::all_or_nothing;
use tedge_config_macros::define_tedge_config;
use tedge_config_macros::struct_field_aliases;
use tedge_config_macros::struct_field_paths;
pub use tedge_config_macros::ConfigNotSet;
use tedge_config_macros::OptionalConfig;
use toml::Table;
use tracing::error;

const DEFAULT_ROOT_CERT_PATH: &str = "/etc/ssl/certs";

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

    pub fn use_legacy_auth(&self) -> bool {
        return std::env::var("C8Y_DEVICE_TENANT").is_ok() && std::env::var("C8Y_DEVICE_USER").is_ok() && std::env::var("C8Y_DEVICE_PASSWORD").is_ok();
    }

    pub fn mqtt_config(&self) -> Result<mqtt_channel::Config, CertificateError> {
        let host = self.mqtt.client.host.as_str();
        let port = u16::from(self.mqtt.client.port);

        let mut mqtt_config = mqtt_channel::Config::default()
            .with_host(host)
            .with_port(port);

        // If these options are not set, just don't use them
        // Configure certificate authentication
        if let Some(ca_file) = self.mqtt.client.auth.ca_file.or_none() {
            mqtt_config.with_cafile(ca_file)?;
        }
        if let Some(ca_path) = self.mqtt.client.auth.ca_dir.or_none() {
            mqtt_config.with_cadir(ca_path)?;
        }

        // Both these options have to either be set or not set, so we keep
        // original error to rethrow when only one is set
        if let Ok(Some((client_cert, client_key))) = all_or_nothing((
            self.mqtt.client.auth.cert_file.as_ref(),
            self.mqtt.client.auth.key_file.as_ref(),
        )) {
            mqtt_config.with_client_auth(client_cert, client_key)?;
        }

        Ok(mqtt_config)
    }

    pub fn mqtt_client_auth_config(&self) -> MqttAuthConfig {
        let mut client_auth = MqttAuthConfig {
            ca_dir: self.mqtt.client.auth.ca_dir.or_none().cloned(),
            ca_file: self.mqtt.client.auth.ca_file.or_none().cloned(),
            client: None,
        };
        // Both these options have to either be set or not set
        if let Ok(Some((client_cert, client_key))) = all_or_nothing((
            self.mqtt.client.auth.cert_file.as_ref(),
            self.mqtt.client.auth.key_file.as_ref(),
        )) {
            client_auth.client = Some(MqttAuthClientConfig {
                cert_file: client_cert.clone(),
                key_file: client_key.clone(),
            })
        }
        client_auth
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(into = "&'static str", try_from = "String")]
/// A version of tedge.toml, used to manage migrations (see [Self::migrations])
pub enum TEdgeTomlVersion {
    One,
    Two,
}

impl Default for TEdgeTomlVersion {
    fn default() -> Self {
        Self::One
    }
}

impl TryFrom<String> for TEdgeTomlVersion {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "1" => Ok(Self::One),
            "2" => Ok(Self::Two),
            _ => todo!(),
        }
    }
}

impl From<TEdgeTomlVersion> for &'static str {
    fn from(value: TEdgeTomlVersion) -> Self {
        match value {
            TEdgeTomlVersion::One => "1",
            TEdgeTomlVersion::Two => "2",
        }
    }
}

impl From<TEdgeTomlVersion> for toml::Value {
    fn from(value: TEdgeTomlVersion) -> Self {
        let str: &str = value.into();
        toml::Value::String(str.to_owned())
    }
}

pub enum TomlMigrationStep {
    UpdateFieldValue {
        key: &'static str,
        value: toml::Value,
    },

    MoveKey {
        original: &'static str,
        target: &'static str,
    },

    RemoveTableIfEmpty {
        key: &'static str,
    },
}

impl TomlMigrationStep {
    pub fn apply_to(self, mut toml: toml::Value) -> toml::Value {
        match self {
            TomlMigrationStep::MoveKey { original, target } => {
                let mut doc = &mut toml;
                let (tables, field) = original.rsplit_once('.').unwrap();
                for key in tables.split('.') {
                    if doc.as_table().map(|table| table.contains_key(key)) == Some(true) {
                        doc = &mut doc[key];
                    } else {
                        return toml;
                    }
                }
                let value = doc.as_table_mut().unwrap().remove(field);

                if let Some(value) = value {
                    let mut doc = &mut toml;
                    let (tables, field) = target.rsplit_once('.').unwrap();
                    for key in tables.split('.') {
                        let table = doc.as_table_mut().unwrap();
                        if !table.contains_key(key) {
                            table.insert(key.to_owned(), toml::Value::Table(Table::new()));
                        }
                        doc = &mut doc[key];
                    }
                    let table = doc.as_table_mut().unwrap();
                    // TODO if this returns Some, something is going wrong? Maybe this could be an error, or maybe it doesn't matter
                    table.insert(field.to_owned(), value);
                }
            }
            TomlMigrationStep::UpdateFieldValue { key, value } => {
                let mut doc = &mut toml;
                let (tables, field) = key.rsplit_once('.').unwrap();
                for key in tables.split('.') {
                    let table = doc.as_table_mut().unwrap();
                    if !table.contains_key(key) {
                        table.insert(key.to_owned(), toml::Value::Table(Table::new()));
                    }
                    doc = &mut doc[key];
                }
                let table = doc.as_table_mut().unwrap();
                // TODO if this returns Some, something is going wrong? Maybe this could be an error, or maybe it doesn't matter
                table.insert(field.to_owned(), value);
            }
            TomlMigrationStep::RemoveTableIfEmpty { key } => {
                let mut doc = &mut toml;
                let (parents, target) = key.rsplit_once('.').unwrap();
                for key in parents.split('.') {
                    let table = doc.as_table_mut().unwrap();
                    if !table.contains_key(key) {
                        table.insert(key.to_owned(), toml::Value::Table(Table::new()));
                    }
                    doc = &mut doc[key];
                }
                let table = doc.as_table_mut().unwrap();
                if let Some(table) = table.get(target) {
                    let table = table.as_table().unwrap();
                    // TODO make sure this is covered in toml migration test
                    if !table.is_empty() {
                        return toml;
                    }
                }
                table.remove(target);
            }
        }

        toml
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

impl TEdgeTomlVersion {
    fn next(self) -> Self {
        match self {
            Self::One => Self::Two,
            Self::Two => Self::Two,
        }
    }

    /// The migrations to upgrade `tedge.toml` from its current version to the
    /// next version.
    ///
    /// If this returns `None`, the version of `tedge.toml` is the latest
    /// version, and no migrations need to be applied.
    pub fn migrations(self) -> Option<Vec<TomlMigrationStep>> {
        use WritableKey::*;
        let mv = |original, target: WritableKey| TomlMigrationStep::MoveKey {
            original,
            target: target.as_str(),
        };
        let update_version_field = || TomlMigrationStep::UpdateFieldValue {
            key: "config.version",
            value: self.next().into(),
        };
        let rm = |key| TomlMigrationStep::RemoveTableIfEmpty { key };

        match self {
            Self::One => Some(vec![
                mv("mqtt.port", MqttBindPort),
                mv("mqtt.bind_address", MqttBindAddress),
                mv("mqtt.client_host", MqttClientHost),
                mv("mqtt.client_port", MqttClientPort),
                mv("mqtt.client_ca_file", MqttClientAuthCaFile),
                mv("mqtt.client_ca_path", MqttClientAuthCaDir),
                mv("mqtt.client_auth.cert_file", MqttClientAuthCertFile),
                mv("mqtt.client_auth.key_file", MqttClientAuthKeyFile),
                rm("mqtt.client_auth"),
                mv("mqtt.external_port", MqttExternalBindPort),
                mv("mqtt.external_bind_address", MqttExternalBindAddress),
                mv("mqtt.external_bind_interface", MqttExternalBindInterface),
                mv("mqtt.external_capath", MqttExternalCaPath),
                mv("mqtt.external_certfile", MqttExternalCertFile),
                mv("mqtt.external_keyfile", MqttExternalKeyFile),
                mv("az.mapper_timestamp", AzMapperTimestamp),
                mv("aws.mapper_timestamp", AwsMapperTimestamp),
                mv("http.port", HttpBindPort),
                mv("http.bind_address", HttpBindAddress),
                mv("software.default_plugin_type", SoftwarePluginDefault),
                mv("run.lock_files", RunLockFiles),
                mv("firmware.child_update_timeout", FirmwareChildUpdateTimeout),
                mv("c8y.smartrest_templates", C8ySmartrestTemplates),
                update_version_field(),
            ]),
            Self::Two => None,
        }
    }
}

#[diagnostic::on_unimplemented(
    message = "To use `{Self}` as a tedge config type, it must implement the `AppendRemoveItem` trait",
    note = "This can be done using impl_append_remove_for_single_value! macro"
)]
pub trait AppendRemoveItem {
    type Item;

    fn append(current_value: Option<Self::Item>, new_value: Self::Item) -> Option<Self::Item>;

    fn remove(current_value: Option<Self::Item>, remove_value: Self::Item) -> Option<Self::Item>;
}

macro_rules! impl_append_remove_for_single_value {
    ($($type:ty),*) => {
        $(
            impl AppendRemoveItem for $type {
                type Item = $type;

                fn append(_current_value: Option<Self::Item>, new_value: Self::Item) -> Option<Self::Item> {
                    Some(new_value)
                }

                fn remove(current_value: Option<Self::Item>, remove_value: Self::Item) -> Option<Self::Item> {
                    match current_value {
                        Some(current) if current == remove_value => None,
                        _ => current_value
                    }
                }
            }
        )*
    }
}

impl_append_remove_for_single_value!(
    Utf8PathBuf,
    String,
    ConnectUrl,
    HostPort<HTTPS_PORT>,
    HostPort<MQTT_TLS_PORT>,
    bool,
    IpAddr,
    u16,
    Arc<str>,
    AutoFlag,
    TopicPrefix,
    SoftwareManagementApiFlag,
    AutoLogUpload,
    TimeFormat,
    NonZeroU16,
    SecondsOrHumanTime,
    u32,
    AptConfig
);

impl AppendRemoveItem for TemplatesSet {
    type Item = TemplatesSet;

    fn append(current_value: Option<Self::Item>, new_value: Self::Item) -> Option<Self::Item> {
        if let Some(current_value) = current_value {
            Some(TemplatesSet(
                current_value
                    .0
                    .into_iter()
                    .chain(new_value.0)
                    .collect::<std::collections::BTreeSet<String>>()
                    .into_iter()
                    .collect(),
            ))
        } else {
            Some(new_value)
        }
    }

    fn remove(current_value: Option<Self::Item>, remove_value: Self::Item) -> Option<Self::Item> {
        let mut current_value = current_value;

        if let Some(ref mut current_value) = current_value {
            let to_remove = std::collections::BTreeSet::from_iter(remove_value.0);

            current_value.0.retain(|value| !to_remove.contains(value));
        }

        current_value
    }
}

define_tedge_config! {
    #[tedge_config(reader(skip))]
    config: {
        #[tedge_config(default(variable = "TEdgeTomlVersion::One"))]
        version: TEdgeTomlVersion,
    },

    device: {
        /// Identifier of the device within the fleet. It must be globally
        /// unique and is derived from the device certificate.
        #[tedge_config(readonly(
            write_error = "\
                The device id is read from the device certificate and cannot be set directly.\n\
                To set 'device.id' to some <id>, you can use `tedge cert create --device-id <id>`.",
            function = "device_id",
        ))]
        #[tedge_config(example = "Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665")]
        #[tedge_config(note = "This setting is derived from the device certificate and is therefore read only.")]
        #[doku(as = "String")]
        id: Result<String, ReadError>,

        /// Path where the device's private key is stored
        #[tedge_config(example = "/etc/tedge/device-certs/tedge-private-key.pem", default(function = "default_device_key"))]
        #[doku(as = "PathBuf")]
        key_path: Utf8PathBuf,

        /// Path where the device's certificate is stored
        #[tedge_config(example = "/etc/tedge/device-certs/tedge-certificate.pem", default(function = "default_device_cert"))]
        #[doku(as = "PathBuf")]
        cert_path: Utf8PathBuf,

        /// Path where the device's certificate signing request is stored
        #[tedge_config(example = "/etc/tedge/device-certs/tedge.csr", default(function = "default_device_csr"))]
        #[doku(as = "PathBuf")]
        csr_path: Utf8PathBuf,

        /// The default device type
        #[tedge_config(example = "thin-edge.io", default(value = "thin-edge.io"))]
        #[tedge_config(rename = "type")]
        ty: String,
    },

    c8y: {
        /// Endpoint URL of Cumulocity tenant
        #[tedge_config(example = "your-tenant.cumulocity.com")]
        // Config consumers should use `c8y.http`/`c8y.mqtt` as appropriate, hence this field is private
        #[tedge_config(reader(private))]
        url: ConnectUrl,

        /// The path where Cumulocity root certificate(s) are stored
        #[tedge_config(note = "The value can be a directory path as well as the path of the certificate file.")]
        #[tedge_config(example = "/etc/tedge/c8y-trusted-root-certificates.pem", default(variable = "DEFAULT_ROOT_CERT_PATH"))]
        #[doku(as = "PathBuf")]
        root_cert_path: Utf8PathBuf,

        /// Cumulocity Username
        #[tedge_config(note = "The value can be a directory path as well as the path of the certificate file.")]
        #[tedge_config(example = "t12345/device_tedge001", default(variable = "DEFAULT_ROOT_CERT_PATH"))]
        username: String,

        /// Cumulocity Password
        #[tedge_config(note = "The value can be a directory path as well as the path of the certificate file.")]
        #[tedge_config(example = "d8aj1d8j1.81", default(variable = "DEFAULT_ROOT_CERT_PATH"))]
        password: String,

        smartrest: {
            /// Set of SmartREST template IDs the device should subscribe to
            #[tedge_config(example = "templateId1,templateId2", default(function = "TemplatesSet::default"))]
            templates: TemplatesSet,
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
            #[tedge_config(example = "true", default(value = false))]
            firmware_update: bool,
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
                #[tedge_config(example = "8001", default(value = 8001u16))]
                port: u16,
            },

            /// The file that will be used as the server certificate for the Cumulocity proxy
            #[tedge_config(example = "/etc/tedge/device-certs/c8y_proxy_certificate.pem")]
            #[doku(as = "PathBuf")]
            cert_path: Utf8PathBuf,

            /// The file that will be used as the server private key for the Cumulocity proxy
            #[tedge_config(example = "/etc/tedge/device-certs/c8y_proxy_key.pem")]
            #[doku(as = "PathBuf")]
            key_path: Utf8PathBuf,

            /// Path to a file containing the PEM encoded CA certificates that are
            /// trusted when checking incoming client certificates for the Cumulocity Proxy
            #[tedge_config(example = "/etc/ssl/certs")]
            #[doku(as = "PathBuf")]
            ca_path: Utf8PathBuf,
        },

        bridge: {
            include: {
                /// Set the bridge local clean session flag (this requires mosquitto >= 2.0.0)
                #[tedge_config(note = "If set to 'auto', this cleans the local session accordingly the detected version of mosquitto.")]
                #[tedge_config(example = "auto", default(variable = "AutoFlag::Auto"))]
                local_cleansession: AutoFlag,
            },

            /// The topic prefix that will be used for the mapper bridge MQTT topic. For instance,
            /// if this is set to "c8y", then messages published to `c8y/s/us` will be
            /// forwarded by to Cumulocity on the `s/us` topic
            #[tedge_config(example = "c8y", default(function = "c8y_topic_prefix"))]
            topic_prefix: TopicPrefix,
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
    az: {
        /// Endpoint URL of Azure IoT tenant
        #[tedge_config(example = "myazure.azure-devices.net")]
        url: ConnectUrl,

        /// The path where Azure IoT root certificate(s) are stored
        #[tedge_config(note = "The value can be a directory path as well as the path of the certificate file.")]
        #[tedge_config(example = "/etc/tedge/az-trusted-root-certificates.pem", default(variable = "DEFAULT_ROOT_CERT_PATH"))]
        #[doku(as = "PathBuf")]
        root_cert_path: Utf8PathBuf,

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
        },

        /// Set of MQTT topics the Azure IoT mapper should subscribe to
        #[tedge_config(example = "te/+/+/+/+/a/+,te/+/+/+/+/m/+,te/+/+/+/+/e/+")]
        #[tedge_config(default(value = "te/+/+/+/+/m/+,te/+/+/+/+/e/+,te/+/+/+/+/a/+,te/+/+/+/+/status/health"))]
        topics: TemplatesSet,
    },

    aws: {
        /// Endpoint URL of AWS IoT tenant
        #[tedge_config(example = "your-endpoint.amazonaws.com")]
        url: ConnectUrl,

        /// The path where AWS IoT root certificate(s) are stored
        #[tedge_config(note = "The value can be a directory path as well as the path of the certificate file.")]
        #[tedge_config(example = "/etc/tedge/aws-trusted-root-certificates.pem", default(variable = "DEFAULT_ROOT_CERT_PATH"))]
        #[doku(as = "PathBuf")]
        root_cert_path: Utf8PathBuf,

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
                #[doku(as = "PathBuf")]
                #[tedge_config(deprecated_name = "cafile")]
                ca_file: Utf8PathBuf,

                /// Path to the directory containing the CA certificates used by MQTT
                /// clients when authenticating the MQTT broker
                #[tedge_config(example = "/etc/mosquitto/ca_certificates")]
                #[doku(as = "PathBuf")]
                #[tedge_config(deprecated_name = "cadir")]
                ca_dir: Utf8PathBuf,

                /// Path to the client certificate
                #[doku(as = "PathBuf")]
                #[tedge_config(example = "/etc/mosquitto/auth_certificates/cert.pem")]
                #[tedge_config(deprecated_name = "certfile")]
                cert_file: Utf8PathBuf,

                /// Path to the client private key
                #[doku(as = "PathBuf")]
                #[tedge_config(example = "/etc/mosquitto/auth_certificates/key.pem")]
                #[tedge_config(deprecated_name = "keyfile")]
                key_file: Utf8PathBuf,
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
            #[doku(as = "PathBuf")]
            #[tedge_config(deprecated_key = "mqtt.external.capath")]
            ca_path: Utf8PathBuf,

            /// Path to the certificate file which is used by the external MQTT listener
            #[tedge_config(note = "This setting shall be used together with `mqtt.external.key_file` for external connections.")]
            #[tedge_config(example = "/etc/tedge/device-certs/tedge-certificate.pem")]
            #[doku(as = "PathBuf")]
            #[tedge_config(deprecated_key = "mqtt.external.certfile")]
            cert_file: Utf8PathBuf,

            /// Path to the key file which is used by the external MQTT listener
            #[tedge_config(note = "This setting shall be used together with `mqtt.external.cert_file` for external connections.")]
            #[tedge_config(example = "/etc/tedge/device-certs/tedge-private-key.pem")]
            #[doku(as = "PathBuf")]
            #[tedge_config(deprecated_key = "mqtt.external.keyfile")]
            key_file: Utf8PathBuf,
        },

        bridge: {
            #[tedge_config(default(value = false))]
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
                #[doku(as = "PathBuf")]
                #[tedge_config(reader(private))]
                cert_file: Utf8PathBuf,

                /// Path to the private key which is used by the agent when connecting to external services
                #[doku(as = "PathBuf")]
                #[tedge_config(reader(private))]
                key_file: Utf8PathBuf,
            },
        },

        /// The file that will be used as the server certificate for the File Transfer Service
        #[tedge_config(example = "/etc/tedge/device-certs/file_transfer_certificate.pem")]
        #[doku(as = "PathBuf")]
        cert_path: Utf8PathBuf,

        /// The file that will be used as the server private key for the File Transfer Service
        #[tedge_config(example = "/etc/tedge/device-certs/file_transfer_key.pem")]
        #[doku(as = "PathBuf")]
        key_path: Utf8PathBuf,

        /// Path to a directory containing the PEM encoded CA certificates that are
        /// trusted when checking incoming client certificates for the File Transfer Service
        #[tedge_config(example = "/etc/ssl/certs")]
        #[doku(as = "PathBuf")]
        ca_path: Utf8PathBuf,
    },

    agent: {
        state: {
            /// The directory where the tedge-agent persists its state across restarts
            #[tedge_config(note = "If the given directory doesn't exists, `/etc/tedge/.agent` is used as a fallback irrespective of the current setting.")]
            #[tedge_config(default(value = "/data/tedge/agent"))]
            #[tedge_config(example = "/data/tedge/agent")]
            #[doku(as = "PathBuf")]
            path: Utf8PathBuf,
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
        #[doku(as = "PathBuf")]
        #[tedge_config(example = "/run", default(value = "/run"))]
        path: Utf8PathBuf,

        /// Whether to create a lock file or not
        #[tedge_config(example = "true", default(value = true))]
        lock_files: bool,

        /// Interval at which the memory usage is logged (in seconds if no unit is provided). Logging is disabled if set to 0
        #[tedge_config(example = "60s", default(from_str = "0"))]
        log_memory_interval: SecondsOrHumanTime,
    },

    logs: {
        /// The directory used to store logs
        #[tedge_config(example = "/var/log/tedge", default(value = "/var/log/tedge"))]
        #[doku(as = "PathBuf")]
        path: Utf8PathBuf,
    },

    tmp: {
        /// The temporary directory used to download files to the device
        #[tedge_config(example = "/tmp", default(value = "/tmp"))]
        #[doku(as = "PathBuf")]
        path: Utf8PathBuf,
    },

    data: {
        /// The directory used to store data like cached files, runtime metadata, etc.
        #[tedge_config(example = "/var/tedge", default(value = "/var/tedge"))]
        #[doku(as = "PathBuf")]
        path: Utf8PathBuf,
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
}

static CLOUD_ROOT_CERTIFICATES: OnceLock<Arc<[Certificate]>> = OnceLock::new();

impl TEdgeConfigReader {
    pub fn cloud_root_certs(&self) -> CloudRootCerts {
        let roots = CLOUD_ROOT_CERTIFICATES.get_or_init(|| {
            let c8y_roots = read_trust_store(&self.c8y.root_cert_path).unwrap_or_else(move |e| {
                error!(
                    "Unable to read certificates from {}: {e:?}",
                    ReadableKey::C8yRootCertPath
                );
                vec![]
            });
            let az_roots = read_trust_store(&self.az.root_cert_path).unwrap_or_else(move |e| {
                error!(
                    "Unable to read certificates from {}: {e:?}",
                    ReadableKey::AzRootCertPath
                );
                vec![]
            });
            let aws_roots = read_trust_store(&self.aws.root_cert_path).unwrap_or_else(move |e| {
                error!(
                    "Unable to read certificates from {}: {e:?}",
                    ReadableKey::AwsRootCertPath
                );
                vec![]
            });
            c8y_roots
                .into_iter()
                .chain(az_roots)
                .chain(aws_roots)
                .collect()
        });

        CloudRootCerts::from(roots.clone())
    }

    pub fn cloud_client_tls_config(&self) -> rustls::ClientConfig {
        // TODO do we want to unwrap here?
        client_config_for_ca_certificates([
            &self.c8y.root_cert_path,
            &self.az.root_cert_path,
            &self.aws.root_cert_path,
        ])
        .unwrap()
    }
}

fn c8y_topic_prefix() -> TopicPrefix {
    TopicPrefix::try_new("c8y").unwrap()
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, serde::Serialize)]
#[serde(try_from = "Cow<'_, str>", into = "Arc<str>")]
/// A valid MQTT topic prefix, used to customise the c8y/ topic prefix
pub struct TopicPrefix(Arc<str>);

impl Document for TopicPrefix {
    fn ty() -> Type {
        String::ty()
    }
}

#[derive(thiserror::Error, Debug)]
#[error("{0}")]
pub struct InvalidTopicPrefix(#[from] anyhow::Error);

impl<'a> TryFrom<Cow<'a, str>> for TopicPrefix {
    type Error = InvalidTopicPrefix;
    fn try_from(value: Cow<'a, str>) -> Result<Self, Self::Error> {
        Self::try_new(&value).map_err(InvalidTopicPrefix)
    }
}

impl TryFrom<&str> for TopicPrefix {
    type Error = InvalidTopicPrefix;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_new(value).map_err(InvalidTopicPrefix)
    }
}

impl TopicPrefix {
    fn try_new(value: &str) -> Result<Self, anyhow::Error> {
        ensure!(!value.is_empty(), "Topic prefix must not be empty");
        ensure!(!value.contains('#'), "Topic prefix cannot contain '#'");
        ensure!(!value.contains('+'), "Topic prefix cannot contain '+'");
        ensure!(
            value != "c8y-internal",
            "Topic prefix cannot be c8y-internal"
        );
        Ok(Self(value.into()))
    }
}

impl FromStr for TopicPrefix {
    type Err = InvalidTopicPrefix;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_new(s).map_err(InvalidTopicPrefix)
    }
}

impl From<TopicPrefix> for Arc<str> {
    fn from(value: TopicPrefix) -> Self {
        value.0
    }
}

impl Deref for TopicPrefix {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl TopicPrefix {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TopicPrefix {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

fn default_http_bind_address(dto: &TEdgeConfigDto) -> IpAddr {
    let external_address = dto.mqtt.external.bind.address;
    external_address
        .or(dto.mqtt.bind.address)
        .unwrap_or(Ipv4Addr::LOCALHOST.into())
}

fn device_id(reader: &TEdgeConfigReader) -> Result<String, ReadError> {
    let pem = PemCertificate::from_pem_file(&reader.device.cert_path)
        .map_err(|err| cert_error_into_config_error(ReadOnlyKey::DeviceId.as_str(), err))?;
    let device_id = pem
        .subject_common_name()
        .map_err(|err| cert_error_into_config_error(ReadOnlyKey::DeviceId.as_str(), err))?;
    Ok(device_id)
}

fn cert_error_into_config_error(key: &'static str, err: CertificateError) -> ReadError {
    match &err {
        CertificateError::IoError(io_err) => match io_err.kind() {
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

fn default_device_key(location: &TEdgeConfigLocation) -> Utf8PathBuf {
    location
        .tedge_config_root_path()
        .join("device-certs")
        .join("tedge-private-key.pem")
}

fn default_device_cert(location: &TEdgeConfigLocation) -> Utf8PathBuf {
    location
        .tedge_config_root_path()
        .join("device-certs")
        .join("tedge-certificate.pem")
}

fn default_device_csr(location: &TEdgeConfigLocation) -> Utf8PathBuf {
    location
        .tedge_config_root_path()
        .join("device-certs")
        .join("tedge.csr")
}

fn default_mqtt_port() -> NonZeroU16 {
    NonZeroU16::try_from(1883).unwrap()
}

#[derive(thiserror::Error, Debug)]
pub enum ReadError {
    #[error(transparent)]
    ConfigNotSet(#[from] ConfigNotSet),

    #[error("Config value {key}, cannot be read: {message} ")]
    ReadOnlyNotFound {
        key: &'static str,
        message: &'static str,
    },

    #[error("Derivation for `{key}` failed: {cause}")]
    DerivationFailed { key: &'static str, cause: String },
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

#[derive(Debug, Clone, Default)]
pub struct MqttAuthConfig {
    pub ca_dir: Option<Utf8PathBuf>,
    pub ca_file: Option<Utf8PathBuf>,
    pub client: Option<MqttAuthClientConfig>,
}

#[derive(Debug, Clone, Default)]
pub struct MqttAuthClientConfig {
    pub cert_file: Utf8PathBuf,
    pub key_file: Utf8PathBuf,
}

impl TEdgeConfigReaderHttp {
    pub fn client_tls_config(&self) -> anyhow::Result<Option<rustls::ClientConfig>> {
        let client_cert_key = crate::all_or_nothing((
            self.client.auth.key_file.as_ref(),
            self.client.auth.cert_file.as_ref(),
        ))
        .map_err(|e| anyhow!("{e}"))?;

        client_cert_key
            .map(|(key, cert)| {
                create_tls_config(
                    self.ca_path
                        .or_none()
                        .map_or(DEFAULT_ROOT_CERT_PATH, |ca| ca.as_str()),
                    key,
                    cert,
                )
            })
            .transpose()
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

        assert_eq!(reader.c8y.http.key(), "c8y.url");
    }
}
