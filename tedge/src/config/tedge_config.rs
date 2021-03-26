use crate::types::*;
use serde::{Deserialize, Serialize};

/// Represents the complete configuration of a thin edge device.
/// This configuration is a wrapper over the device specific configurations
/// as well as the IoT cloud provider specific configurations.
///
/// The following example showcases how the thin edge configuration can be read
/// and how individual configuration values can be retrieved out of it:
///
/// # Examples
/// ```
/// /// Read the default tedge.toml file into a TEdgeConfig object
/// let config: TEdgeConfig = TEdgeConfig::from_default_config().unwrap();
///
/// /// Fetch the device config from the TEdgeConfig object
/// let device_config: DeviceConfig = config.device;
/// /// Fetch the device id from the DeviceConfig object
/// let device_id = device_config.id.unwrap();
///
/// /// Fetch the Cumulocity config from the TEdgeConfig object
/// let cumulocity_config: CumulocityConfig = config.c8y;
/// /// Fetch the Cumulocity URL from the CumulocityConfig object
/// let cumulocity_url = cumulocity_config.url.unwrap();
/// ```
///
#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct TEdgeConfig {
    /// Captures the device specific configurations
    #[serde(default)]
    pub device: DeviceConfig,

    /// Captures the configurations required to connect to Cumulocity
    #[serde(default)]
    pub c8y: CumulocityConfig,
    #[serde(default)]
    pub azure: AzureConfig,
}

/// Represents the device specific configurations defined in the [device] section
/// of the thin edge configuration TOML file
#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct DeviceConfig {
    /// The unique id of the device
    pub id: Option<String>,

    /// Path where the device's private key is stored.
    /// Defaults to $HOME/.tedge/tedge-private.pem
    pub key_path: Option<String>,

    /// Path where the device's certificate is stored.
    /// Defaults to $HOME/.tedge/tedge-certificate.crt
    pub cert_path: Option<String>,
}

/// Represents the Cumulocity specific configurations defined in the
/// [c8y] section of the thin edge configuration TOML file
#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CumulocityConfig {
    /// Preserves the current status of the connection
    connect: Option<String>,

    /// Endpoint URL of the Cumulocity tenant
    pub url: Option<ConnectUrl>,

    /// The path where Cumulocity root certificate(s) are stored.
    /// The value can be a directory path as well as the path of the direct certificate file.
    pub root_cert_path: Option<String>,
}

#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct AzureConfig {
    connect: Option<String>,
    pub url: Option<ConnectUrl>,
    pub root_cert_path: Option<String>,
}
