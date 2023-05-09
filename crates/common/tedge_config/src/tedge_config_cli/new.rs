use crate::ConnectUrl;
use crate::TEdgeConfigLocation;
use crate::TemplatesSet;
use camino::Utf8PathBuf;
use certificate::CertificateError;
use certificate::PemCertificate;
use std::fmt;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::num::NonZeroU16;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use tedge_config_macros::define_tedge_config;
use tedge_config_macros::struct_field_aliases;
use tedge_config_macros::ConfigNotSet;
use tedge_config_macros::OptionalConfig;

const DEFAULT_ROOT_CERT_PATH: &str = "/etc/ssl/certs";

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

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(into = "&'static str", try_from = "String")]
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
}

impl TEdgeTomlVersion {
    pub fn migrations(self) -> Option<Vec<TomlMigrationStep>> {
        use WritableKey::*;
        if Self::One == self {
            let mv = |original, target: WritableKey| TomlMigrationStep::MoveKey {
                original,
                target: target.as_str(),
            };
            let update_version_field = TomlMigrationStep::UpdateFieldValue {
                key: "config.version",
                value: TEdgeTomlVersion::Two.into(),
            };

            Some(vec![
                mv("mqtt.port", MqttBindPort),
                mv("mqtt.bind_address", MqttBindAddress),
                mv("mqtt.client_host", MqttClientHost),
                mv("mqtt.client_port", MqttClientPort),
                mv("mqtt.client_cafile", MqttClientAuthCaFile),
                mv("mqtt.client_capath", MqttClientAuthCaDir),
                mv("mqtt.client_auth.cert_file", MqttClientAuthCertFile),
                mv("mqtt.client_auth.key_file", MqttClientAuthKeyFile),
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
                update_version_field,
            ])
        } else {
            None
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
        // TODO should tedge config support examples for read-only values
        #[doku(example = "Raspberrypi-4d18303a-6d3a-11eb-b1a6-175f6bb72665")]
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
        #[tedge_config(example = "thin-edge.io")]
        #[tedge_config(rename = "type")]
        ty: String,
    },

    c8y: {
        /// Endpoint URL of Azure IoT tenant
        #[tedge_config(example = "myazure.azure-devices.net")]
        url: ConnectUrl,

        /// The path where Azure IoT root certificate(s) are stared
        #[tedge_config(note = "The value can be a directory path as well as the path of the direct certificate file.")]
        #[tedge_config(example = "/etc/tedge/az-trusted-root-certificates.pem", default(variable = "DEFAULT_ROOT_CERT_PATH"))]
        #[doku(as = "PathBuf")]
        root_cert_path: Utf8PathBuf,

        smartrest_templates: TemplatesSet,
    },

    #[serde(alias = "azure")] // for 0.1.0 compatibility
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
        }
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
        }
    },

    mqtt: {
        bind: {
            /// The address mosquitto binds to for internal use
            #[tedge_config(example = "127.0.0.1", default(variable = "Ipv4Addr::LOCALHOST"))]
            address: IpAddr,

            /// The port mosquitto binds to for internal use
            #[tedge_config(example = "1883", default(function = "default_mqtt_port"), alternate_key = "mqtt.port")]
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
            #[tedge_config(default(from_path = "mqtt.bind.port"))]
            #[doku(as = "u16")]
            port: NonZeroU16,

            auth: {
                /// Path to the CA certificate used by MQTT clients to use when authenticating the MQTT broker
                #[tedge_config(example = "/etc/mosquitto/ca_certificates/ca.crt")]
                #[doku(as = "PathBuf")]
                #[serde(alias = "cafile")]
                ca_file: Utf8PathBuf,

                /// Path to the directory containing the CA certificates used by MQTT
                /// clients when authenticating the MQTT broker
                #[tedge_config(example = "/etc/mosquitto/ca_certificates")]
                #[doku(as = "PathBuf")]
                #[serde(alias = "capath")]
                ca_dir: Utf8PathBuf,

                /// Path to the client certficate
                #[doku(as = "PathBuf")]
                #[serde(alias = "certfile")]
                cert_file: Utf8PathBuf,

                /// Path to the client private key
                #[doku(as = "PathBuf")]
                #[serde(alias = "keyfile")]
                key_file: Utf8PathBuf,
            }
        },

        external: {
            bind: {
                /// The port mosquitto binds to for external use
                #[tedge_config(example = "8883", alternate_key = "mqtt.external.port")]
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
            #[serde(alias = "capath")]
            ca_path: Utf8PathBuf,

            /// Path to the certificate file which is used by the external MQTT listener
            #[tedge_config(note = "This setting shall be used together with `mqtt.external.key_file` for external connections.")]
            #[tedge_config(example = "/etc/tedge/device-certs/tedge-certificate.pem")]
            #[doku(as = "PathBuf")]
            #[serde(alias = "certfile")]
            cert_file: Utf8PathBuf,

            /// Path to the key file which is used by the external MQTT listener
            #[tedge_config(note = "This setting shall be used together with `mqtt.external.cert_file` for external connections.")]
            #[tedge_config(example = "/etc/tedge/device-certs/tedge-private-key.pem")]
            #[doku(as = "PathBuf")]
            #[serde(alias = "keyfile")]
            key_file: Utf8PathBuf,
        }
    },

    http: {
        bind: {
            #[tedge_config(example = "8000", alternate_key = "http.port")]
            port: u16,

            address: IpAddr,
        },
    },

    software: {
        plugin: {
            #[tedge_config(example = "apt")]
            default: String,
        }
    },

    run: {
        #[doku(as = "PathBuf")]
        path: Utf8PathBuf,

        lock_files: bool,
    },

    logs: {
        #[doku(as = "PathBuf")]
        path: Utf8PathBuf,
    },

    tmp: {
        #[doku(as = "PathBuf")]
        path: Utf8PathBuf,
    },

    data: {
        #[doku(as = "PathBuf")]
        path: Utf8PathBuf,
    },

    firmware: {
        child: {
            update: {
                #[tedge_config(example = "3600")]
                timeout: Seconds,
            }
        }
    },

    service: {
        #[tedge_config(rename = "type")]
        ty: String,
    },
}

#[derive(
    Copy, Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq, doku::Document,
)]
#[serde(transparent)]
pub struct Seconds(pub(crate) u64);

impl Seconds {
    pub fn duration(self) -> Duration {
        Duration::from_secs(self.0)
    }
}

impl FromStr for Seconds {
    type Err = <u64 as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        u64::from_str(s).map(Self)
    }
}

impl fmt::Display for Seconds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
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
