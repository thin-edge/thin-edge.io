use crate::models::FilePath;
use crate::TEdgeConfigLocation;
use crate::{Flag, Port};
use std::fmt;
use std::path::Path;
use percentage::Percentage;

const DEFAULT_ETC_PATH: &str = "/etc";
const DEFAULT_PORT: u16 = 1883;
const DEFAULT_BUFFER_SIZE_IN_PERCENTAGE: &str = "5";

/// Stores default values for use by `TEdgeConfig` in case no configuration setting
/// is available.
///
/// We DO NOT base the defaults on the currently executing user. Instead, we derive
/// the defaults from the location of the `tedge.toml` file. This allows run
/// `sudo tedge -c '$HOME/.tedge/tedge.toml ...` where the defaults are picked up
/// correctly.
///
/// The choice, where to find `tedge.toml` on the other hand is based on the executing user AND the
/// env `$HOME`.  But once we have found `tedge.toml`, we never again have to care about the
/// executing user (except when `chown`ing files...).
///

impl fmt::Debug for Percentage {    
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{}", self.
        }
     }


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TEdgeConfigDefaults {
    /// Default device cert path
    pub default_device_cert_path: FilePath,

    /// Default device key path
    pub default_device_key_path: FilePath,

    /// Default path for azure root certificates
    pub default_azure_root_cert_path: FilePath,

    /// Default path for c8y root certificates
    pub default_c8y_root_cert_path: FilePath,

    /// Default mapper timestamp bool
    pub default_mapper_timestamp: Flag,

    /// Default port for mqtt internal listener
    pub default_mqtt_port: Port,

    // Default buffer size for downloading the file
    pub default_buffer_size_in_percentage: Percentage,
}

impl From<&TEdgeConfigLocation> for TEdgeConfigDefaults {
    fn from(config_location: &TEdgeConfigLocation) -> Self {
        let system_cert_path = Path::new(DEFAULT_ETC_PATH).join("ssl").join("certs");
        Self {
            default_device_cert_path: config_location
                .tedge_config_root_path()
                .join("device-certs")
                .join("tedge-certificate.pem")
                .into(),
            default_device_key_path: config_location
                .tedge_config_root_path()
                .join("device-certs")
                .join("tedge-private-key.pem")
                .into(),
            default_azure_root_cert_path: system_cert_path.clone().into(),
            default_c8y_root_cert_path: system_cert_path.into(),
            default_mapper_timestamp: Flag(true),
            default_mqtt_port: Port(DEFAULT_PORT),
            default_buffer_size_in_percentage: DEFAULT_BUFFER_SIZE_IN_PERCENTAGE.to_string(),
        }
    }
}

#[test]
fn test_from_tedge_config_location() {
    let config_location = TEdgeConfigLocation::from_custom_root("/opt/etc/_tedge");
    let defaults = TEdgeConfigDefaults::from(&config_location);

    assert_eq!(
        defaults,
        TEdgeConfigDefaults {
            default_device_cert_path: FilePath::from(
                "/opt/etc/_tedge/device-certs/tedge-certificate.pem"
            ),
            default_device_key_path: FilePath::from(
                "/opt/etc/_tedge/device-certs/tedge-private-key.pem"
            ),
            default_azure_root_cert_path: FilePath::from("/etc/ssl/certs"),
            default_c8y_root_cert_path: FilePath::from("/etc/ssl/certs"),
            default_mapper_timestamp: Flag(true),
            default_mqtt_port: Port(DEFAULT_PORT),
            default_buffer_size_in_percentage: DEFAULT_BUFFER_SIZE_IN_PERCENTAGE,
        }
    );
}
