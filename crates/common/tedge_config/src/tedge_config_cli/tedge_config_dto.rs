//! Crate-private plain-old data-type used for serialization.

use std::num::NonZeroU16;
use std::path::PathBuf;

use crate::*;
use camino::Utf8PathBuf;
use doku::Document;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub struct TEdgeConfigDto {
    /// Captures the device specific configurations
    #[serde(default)]
    pub(crate) device: DeviceConfigDto,

    /// Captures the configurations required to connect to Cumulocity
    #[serde(default)]
    pub(crate) c8y: CumulocityConfigDto,

    #[serde(default, alias = "azure")] // for version 0.1.0 compatibility
    pub(crate) az: AzureConfigDto,

    #[serde(default, alias = "aws")] // for version 0.1.0 compatibility
    pub(crate) aws: AwsConfigDto,

    #[serde(default)]
    pub(crate) mqtt: MqttConfigDto,

    #[serde(default)]
    pub(crate) http: HttpConfigDto,

    #[serde(default)]
    pub(crate) software: SoftwareConfigDto,

    #[serde(default)]
    pub(crate) tmp: PathConfigDto,

    #[serde(default)]
    pub(crate) logs: PathConfigDto,

    #[serde(default)]
    pub(crate) run: PathConfigDto,

    #[serde(default)]
    pub(crate) data: PathConfigDto,

    #[serde(default)]
    pub(crate) firmware: FirmwareConfigDto,

    #[serde(default)]
    pub(crate) service: ServiceTypeConfigDto,
}

/// Represents the device specific configurations defined in the [device] section
/// of the thin edge configuration TOML file
#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub struct DeviceConfigDto {
    /// Path where the device's private key is stored
    #[doku(example = "/etc/tedge/device-certs/tedge-private-key.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) key_path: Option<Utf8PathBuf>,

    /// Path where the device's certificate is stored
    #[doku(example = "/etc/tedge/device-certs/tedge-certificate.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) cert_path: Option<Utf8PathBuf>,

    /// The default device type
    #[serde(rename = "type")]
    #[doku(example = "thin-edge.io")]
    pub(crate) device_type: Option<String>,
}

/// Represents the Cumulocity specific configurations defined in the
/// [c8y] section of the thin edge configuration TOML file
#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub(crate) struct CumulocityConfigDto {
    /// Endpoint URL of the Cumulocity tenant
    #[doku(example = "your-tenant.cumulocity.com", as = "String")]
    pub(crate) url: Option<ConnectUrl>,

    /// The path where Cumulocity root certificate(s) are stored.
    /// The value can be a directory path as well as the path of the direct certificate file.
    #[doku(example = "/etc/tedge/c8y-trusted-root-certificates.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) root_cert_path: Option<Utf8PathBuf>,

    // TODO improve these examples
    #[doku(example = "template1")]
    #[doku(example = "template2")]
    /// Set of c8y template names used for subscriptions
    pub(crate) smartrest_templates: Option<TemplatesSet>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub(crate) struct AzureConfigDto {
    /// Endpoint URL of Azure IoT tenant
    #[doku(example = "myazure.azure-devices.net", as = "String")]
    pub(crate) url: Option<ConnectUrl>,

    /// The path where Azure root certificate(s) are stored
    #[doku(example = "/etc/tedge/az-trusted-root-certificates.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) root_cert_path: Option<Utf8PathBuf>,

    /// Whether Azure mapper should add timestamp or not
    #[doku(example = "true")]
    pub(crate) mapper_timestamp: Option<bool>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub(crate) struct AwsConfigDto {
    /// Endpoint URL of AWS instance
    #[doku(example = "your-endpoint.amazonaws.com", as = "String")]
    pub(crate) url: Option<ConnectUrl>,

    /// The path where AWS root certificate(s) are stored
    #[doku(example = "/etc/tedge/aws-trusted-root-certificates.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) root_cert_path: Option<Utf8PathBuf>,

    /// Whether Azure mapper should add timestamp or not
    #[doku(example = "true")]
    pub(crate) mapper_timestamp: Option<bool>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub(crate) struct MqttConfigDto {
    /// The address mosquitto binds to for internal use
    pub(crate) bind_address: Option<IpAddress>,
    // TODO example
    /// The port mosquitto binds to for internal use
    pub(crate) port: Option<u16>,

    pub(crate) client_host: Option<String>,

    /// Mqtt broker port, which is used by the mqtt clients to publish or subscribe
    #[doku(example = "1883", as = "u16")]
    // When connecting to a host, port 0 is invalid. When binding, however, port 0 is accepted and
    // understood by the system to dynamically assign any free port to the process. The process then
    // needs to take notice of what port it received, which I'm not sure if we're doing. If we don't
    // want to allow binding to port 0, then we can also use `NonZeroU16` there as well, which
    // because it can never be 0, can make the `Option` completely free, because Option can use 0x0000
    // value for the `None` variant.
    pub(crate) client_port: Option<NonZeroU16>,

    /// The port mosquitto binds to for external use
    #[doku(example = "1883")]
    pub(crate) external_port: Option<u16>,

    /// The address mosquitto binds to for external use
    #[doku(example = "0.0.0.0")]
    pub(crate) external_bind_address: Option<IpAddress>,

    /// The interface mosquitto listens on for external use
    #[doku(example = "wlan0")]
    pub(crate) external_bind_interface: Option<String>,

    // All the paths relating to mosquitto are strings as they need to be safe to write to a configuration file (i.e. probably valid utf-8 at the least)
    /// Path to a file containing the PEM encoded CA certificates that are trusted when checking incoming client certificates
    #[doku(example = "/etc/ssl/certs", as = "PathBuf")]
    pub(crate) external_capath: Option<Utf8PathBuf>,

    /// Path to the certificate file which is used by the external MQTT listener
    #[doku(
        example = "/etc/tedge/device-certs/tedge-certificate.pem",
        as = "PathBuf"
    )]
    pub(crate) external_certfile: Option<Utf8PathBuf>,

    /// Path to the key file which is used by the external MQTT listener
    #[doku(example = "/etc/tedge/device-certs/tedge-private-key.pem")]
    #[doku(as = "PathBuf")]
    pub(crate) external_keyfile: Option<Utf8PathBuf>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub(crate) struct HttpConfigDto {
    /// HTTP server port used by the File Transfer Service
    #[doku(example = "8000")]
    #[serde(alias = "bind_port")]
    pub(crate) port: Option<u16>,

    /// HTTP bind address used by the File Transfer service
    #[doku(example = "127.0.0.1")]
    #[doku(example = "192.168.1.2")]
    pub(crate) bind_address: Option<IpAddress>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub struct SoftwareConfigDto {
    pub(crate) default_plugin_type: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub struct PathConfigDto {
    #[serde(rename = "path")]
    #[doku(as = "PathBuf")]
    pub(crate) dir_path: Option<Utf8PathBuf>,

    /// Whether create lock file or not
    pub(crate) lock_files: Option<bool>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub struct FirmwareConfigDto {
    pub(crate) child_update_timeout: Option<u64>,
}

#[derive(Debug, Default, Deserialize, Serialize, Document)]
pub struct ServiceTypeConfigDto {
    #[serde(rename = "type")]
    pub(crate) service_type: Option<String>,
}
