use crate::*;
use serde::{Deserialize, Serialize};

/// Represents the complete configuration of a thin edge device.
/// This configuration is a wrapper over the device specific configurations
/// as well as the IoT cloud provider specific configurations.
///
/// The following example showcases how the thin edge configuration can be read
/// and how individual configuration values can be retrieved out of it:
///
/// # Examples
/// ```ignore
/// /// Read the default tedge.toml file into a TEdgeConfigDto object
/// let config: TEdgeConfigDto = TEdgeConfigDto::from_default_config().unwrap();
///
/// /// Fetch the device config from the TEdgeConfigDto object
/// let device_config: DeviceConfigDto = config.device;
/// /// Fetch the device id from the DeviceConfigDto object
/// let device_id = device_config.id.unwrap();
///
/// /// Fetch the Cumulocity config from the TEdgeConfigDto object
/// let cumulocity_config: CumulocityConfigDto = config.c8y;
/// /// Fetch the Cumulocity URL from the CumulocityConfigDto object
/// let cumulocity_url = cumulocity_config.url.unwrap();
/// ```
///
#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct TEdgeConfigDto {
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
    connect: Option<String>,

    /// Endpoint URL of the Cumulocity tenant
    pub(crate) url: Option<ConnectUrl>,

    /// The path where Cumulocity root certificate(s) are stored.
    /// The value can be a directory path as well as the path of the direct certificate file.
    pub(crate) root_cert_path: Option<String>,
}

#[serde(deny_unknown_fields)]
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct AzureConfigDto {
    connect: Option<String>,
    pub(crate) url: Option<ConnectUrl>,
    pub(crate) root_cert_path: Option<String>,
}
