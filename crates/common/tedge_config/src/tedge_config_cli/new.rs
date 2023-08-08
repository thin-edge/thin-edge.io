use crate::ConnectUrl;
use crate::HostPort;
use crate::Seconds;
use crate::TEdgeConfigLocation;
use crate::TemplatesSet;
use crate::HTTPS_PORT;
use crate::MQTT_TLS_PORT;
use camino::Utf8PathBuf;
use certificate::CertificateError;
use certificate::PemCertificate;
use doku::Document;
use once_cell::sync::Lazy;
use std::borrow::Cow;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::num::NonZeroU16;
use std::path::PathBuf;
use tedge_config_macros::all_or_nothing;
use tedge_config_macros::define_tedge_config;
use tedge_config_macros::struct_field_aliases;
use tedge_config_macros::struct_field_paths;
pub use tedge_config_macros::ConfigNotSet;
use tedge_config_macros::OptionalConfig;
use toml::Table;

const DEFAULT_ROOT_CERT_PATH: &str = "/etc/ssl/certs";

pub trait OptionalConfigError<T> {
    fn or_err(&self) -> Result<&T, ReadError>;
}

impl<T> OptionalConfigError<T> for OptionalConfig<T> {
    fn or_err(&self) -> Result<&T, ReadError> {
        self.or_config_not_set().map_err(ReadError::from)
    }
}

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

    /// To get the value of `c8y.url`, which is a private field.
    pub fn c8y_url(&self) -> OptionalConfig<ConnectUrl> {
        self.c8y.url.clone()
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

        /// The default device type
        #[tedge_config(example = "thin-edge.io", default(value = "thin-edge.io"))]
        #[tedge_config(rename = "type")]
        ty: String,
    },

    c8y: {
        /// Endpoint URL of Cumulocity tenant
        #[tedge_config(example = "your-tenant.cumulocity.com")]
        #[tedge_config(reader(private))]
        url: ConnectUrl,

        /// The path where Cumulocity root certificate(s) are stared
        #[tedge_config(note = "The value can be a directory path as well as the path of the direct certificate file.")]
        #[tedge_config(example = "/etc/tedge/az-trusted-root-certificates.pem", default(variable = "DEFAULT_ROOT_CERT_PATH"))]
        #[doku(as = "PathBuf")]
        root_cert_path: Utf8PathBuf,

        smartrest: {
            /// Set of SmartREST template IDs the device should subscribe to
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
        #[tedge_config(example = "te/device/+/+/+/m/+,te/device/+/+/+/e/+")]
        #[tedge_config(default(value = "te/device/+/+/+/m/+,te/device/+/+/+/e/+,te/device/+/+/+/a/+"))]
        topics: TemplatesSet,

    },

    #[tedge_config(deprecated_name = "azure")] // for 0.1.0 compatibility
    az: {
        /// Endpoint URL of Azure IoT tenant
        #[tedge_config(example = "myazure.azure-devices.net")]
        url: ConnectUrl,

        /// The path where Azure IoT root certificate(s) are stared
        #[tedge_config(note = "The value can be a directory path as well as the path of the direct certificate file.")]
        #[tedge_config(example = "/etc/tedge/az-trusted-root-certificates.pem", default(variable = "DEFAULT_ROOT_CERT_PATH"))]
        #[doku(as = "PathBuf")]
        root_cert_path: Utf8PathBuf,

        mapper: {
            /// Whether the Azure IoT mapper should add a timestamp or not
            #[tedge_config(example = "true")]
            #[tedge_config(default(value = true))]
            timestamp: bool,
        },

        /// Set of MQTT topics the Azure IoT mapper should subscribe to
        #[tedge_config(example = "tedge/measurements,tedge/measurements/+")]
        #[tedge_config(default(value = "tedge/measurements,tedge/measurements/+,tedge/health/+,tedge/health/+/+"))]
        topics: TemplatesSet,
    },

    aws: {
        /// Endpoint URL of AWS IoT tenant
        #[tedge_config(example = "your-endpoint.amazonaws.com")]
        url: ConnectUrl,

        /// The path where AWS IoT root certificate(s) are stared
        #[tedge_config(note = "The value can be a directory path as well as the path of the direct certificate file.")]
        #[tedge_config(example = "/etc/tedge/aws-trusted-root-certificates.pem", default(variable = "DEFAULT_ROOT_CERT_PATH"))]
        #[doku(as = "PathBuf")]
        root_cert_path: Utf8PathBuf,

        mapper: {
            /// Whether the AWS IoT mapper should add a timestamp or not
            #[tedge_config(example = "true")]
            #[tedge_config(default(value = true))]
            timestamp: bool,
        },

        /// Set of MQTT topics the AWS IoT mapper should subscribe to
        #[tedge_config(example = "tedge/measurements,tedge/measurements/+")]
        #[tedge_config(default(value = "tedge/measurements,tedge/measurements/+,tedge/alarms/+/+,tedge/alarms/+/+/+,tedge/events/+,tedge/events/+/+,tedge/health/+,tedge/health/+/+"))]
        topics: TemplatesSet,
    },

    mqtt: {
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
            #[tedge_config(example = "localhost", default(value = "localhost"))]
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
        }
    },

    http: {
        bind: {
            /// Http server port used by the File Transfer Service
            #[tedge_config(example = "8000", default(value = 8000u16), deprecated_key = "http.port")]
            port: u16,

            /// Http server address used by the File Transfer Service
            #[tedge_config(default(function = "default_http_address"), deprecated_key = "http.address")]
            #[tedge_config(example = "127.0.0.1", example = "192.168.1.2")]
            address: IpAddr,
        },
    },

    software: {
        plugin: {
            /// The default software plugin to be used for software management on the device
            #[tedge_config(example = "apt")]
            default: String,

            /// The maximum number of software packages reported for each type of software package
            #[tedge_config(example = "1000", default(value = 1000u32))]
            max_packages: u32
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
    },

    logs: {
        /// The directory used to store logs
        #[tedge_config(example = "/var/log", default(value = "/var/log"))]
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
                #[tedge_config(example = "3600", default(value = 3600_u64))]
                timeout: Seconds,
            }
        }
    },

    service: {
        /// The thin-edge.io service's service type
        #[tedge_config(rename = "type", example = "systemd", default(value = "service"))]
        ty: String,
    },
}

fn default_http_address(dto: &TEdgeConfigDto) -> IpAddr {
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
            std::io::ErrorKind::NotFound => ReadError::ReadOnlyNotFound { key,
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
    #[test_case::test_case("software.plugin.max_packages")]
    #[test_case::test_case("tmp.path")]
    #[test_case::test_case("logs.path")]
    #[test_case::test_case("run.path")]
    #[test_case::test_case("data.path")]
    #[test_case::test_case("firmware.child.update.timeout")]
    #[test_case::test_case("service.type")]
    #[test_case::test_case("run.lock_files")]
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
