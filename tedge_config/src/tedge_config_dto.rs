//! Crate-private plain-old data-type used for serialization.

use crate::*;
use serde::{Deserialize, Serialize};

#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct TEdgeConfigDto {
    /// Captures the device specific configurations
    #[serde(default)]
    pub(crate) device: DeviceConfigDto,

    /// Captures the configurations required to connect to Cumulocity
    #[serde(default)]
    pub(crate) c8y: CumulocityConfigDto,
    #[serde(default)]
    pub(crate) azure: AzureConfigDto,
}

/// Represents the device specific configurations defined in the [device] section
/// of the thin edge configuration TOML file
#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct DeviceConfigDto {
    /// The unique id of the device
    pub(crate) id: Option<String>,

    /// Path where the device's private key is stored.
    /// Defaults to $HOME/.tedge/tedge-private.pem
    pub(crate) key_path: Option<String>,

    /// Path where the device's certificate is stored.
    /// Defaults to $HOME/.tedge/tedge-certificate.crt
    pub(crate) cert_path: Option<String>,
}

/// Represents the Cumulocity specific configurations defined in the
/// [c8y] section of the thin edge configuration TOML file
#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct CumulocityConfigDto {
    /// Preserves the current status of the connection
    pub(crate) connect: Option<String>,

    /// Endpoint URL of the Cumulocity tenant
    pub(crate) url: Option<ConnectUrl>,

    /// The path where Cumulocity root certificate(s) are stored.
    /// The value can be a directory path as well as the path of the direct certificate file.
    pub(crate) root_cert_path: Option<String>,
}

#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct AzureConfigDto {
    pub(crate) connect: Option<String>,
    pub(crate) url: Option<ConnectUrl>,
    pub(crate) root_cert_path: Option<String>,
}
