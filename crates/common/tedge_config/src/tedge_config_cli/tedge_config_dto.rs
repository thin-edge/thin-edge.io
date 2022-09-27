//! Crate-private plain-old data-type used for serialization.

use crate::*;
use serde::Deserialize;
use serde::Serialize;
use tedge_utils::tedge_derive;

#[tedge_derive::serde_other]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct TEdgeConfigDto {
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
}

/// Represents the device specific configurations defined in the [device] section
/// of the thin edge configuration TOML file
#[tedge_derive::serde_other]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct DeviceConfigDto {
    /// The unique id of the device (DEPRECATED)
    /// This id is now derived from the device certificate
    #[serde(rename(deserialize = "id"), skip_serializing)]
    pub(crate) _id: Option<String>,

    /// Path where the device's private key is stored.
    /// Defaults to $HOME/.tedge/tedge-private.pem
    pub(crate) key_path: Option<FilePath>,

    /// Path where the device's certificate is stored.
    /// Defaults to $HOME/.tedge/tedge-certificate.crt
    pub(crate) cert_path: Option<FilePath>,

    #[serde(rename = "type")]
    pub(crate) device_type: Option<String>,
}

/// Represents the Cumulocity specific configurations defined in the
/// [c8y] section of the thin edge configuration TOML file
#[tedge_derive::serde_other]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct CumulocityConfigDto {
    /// Preserves the current status of the connection
    pub(crate) connect: Option<String>,

    /// Endpoint URL of the Cumulocity tenant
    pub(crate) url: Option<ConnectUrl>,

    /// The path where Cumulocity root certificate(s) are stored.
    /// The value can be a directory path as well as the path of the direct certificate file.
    pub(crate) root_cert_path: Option<FilePath>,

    /// Boolean whether Azure mapper adds timestamp or not.
    pub(crate) mapper_timestamp: Option<bool>,

    /// Set of c8y templates used for subscriptions.
    pub(crate) smartrest_templates: Option<TemplatesSet>,
}

#[tedge_derive::serde_other]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct AzureConfigDto {
    pub(crate) connect: Option<String>,
    pub(crate) url: Option<ConnectUrl>,
    pub(crate) root_cert_path: Option<FilePath>,
    pub(crate) mapper_timestamp: Option<bool>,
}

#[tedge_derive::serde_other]
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AwsConfigDto {
    pub(crate) connect: Option<String>,
    pub(crate) url: Option<ConnectUrl>,
    pub(crate) root_cert_path: Option<FilePath>,
    pub(crate) mapper_timestamp: Option<bool>,
}

#[tedge_derive::serde_other]
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MqttConfigDto {
    pub(crate) port: Option<u16>,
    pub(crate) bind_address: Option<IpAddress>,
    pub(crate) external_port: Option<u16>,
    pub(crate) external_bind_address: Option<IpAddress>,
    pub(crate) external_bind_interface: Option<String>,
    pub(crate) external_capath: Option<FilePath>,
    pub(crate) external_certfile: Option<FilePath>,
    pub(crate) external_keyfile: Option<FilePath>,
}

#[tedge_derive::serde_other]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct HttpConfigDto {
    pub(crate) port: Option<u16>,
    pub(crate) bind_address: Option<IpAddress>,
}

#[tedge_derive::serde_other]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct SoftwareConfigDto {
    pub(crate) default_plugin_type: Option<String>,
}

#[tedge_derive::serde_other]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct PathConfigDto {
    #[serde(rename = "path")]
    pub(crate) dir_path: Option<FilePath>,
}
