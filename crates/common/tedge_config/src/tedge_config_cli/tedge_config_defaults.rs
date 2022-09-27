use crate::tedge_config_cli::models::FilePath;
use crate::tedge_config_cli::models::IpAddress;
use crate::tedge_config_cli::models::TemplatesSet;
use crate::Flag;
use crate::Port;
use crate::TEdgeConfigLocation;
use std::path::Path;

const DEFAULT_ETC_PATH: &str = "/etc";
const DEFAULT_MQTT_PORT: u16 = 1883;
const DEFAULT_HTTP_PORT: u16 = 8000;
pub const DEFAULT_TMP_PATH: &str = "/tmp";
pub const DEFAULT_LOG_PATH: &str = "/var/log";
pub const DEFAULT_RUN_PATH: &str = "/run";
const DEFAULT_DEVICE_TYPE: &str = "thin-edge.io";

pub const DEFAULT_FILE_TRANSFER_ROOT_PATH: &str = "/var/tedge/file-transfer";

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TEdgeConfigDefaults {
    /// Default device cert path
    pub default_device_cert_path: FilePath,

    /// Default device key path
    pub default_device_key_path: FilePath,

    /// Default path for azure root certificates
    pub default_azure_root_cert_path: FilePath,

    /// Default path for AWS root certificates
    pub default_aws_root_cert_path: FilePath,

    /// Default path for c8y root certificates
    pub default_c8y_root_cert_path: FilePath,

    /// Default c8y smartrest templates
    pub default_c8y_smartrest_templates: TemplatesSet,

    /// Default mapper timestamp bool
    pub default_mapper_timestamp: Flag,

    /// Default port for mqtt internal listener
    pub default_mqtt_port: Port,

    /// Default port for http file transfer service
    pub default_http_port: Port,

    /// Default tmp path
    pub default_tmp_path: FilePath,

    /// Default log path
    pub default_logs_path: FilePath,

    /// Default run path
    pub default_run_path: FilePath,

    /// Default device type
    pub default_device_type: String,

    /// Default mqtt bind address
    pub default_mqtt_bind_address: IpAddress,

    /// Default htpp bind address
    pub default_http_bind_address: IpAddress,
}

impl From<&TEdgeConfigLocation> for TEdgeConfigDefaults {
    fn from(config_location: &TEdgeConfigLocation) -> Self {
        let system_cert_path = Path::new(DEFAULT_ETC_PATH).join("ssl").join("certs");
        let tmp_path = Path::new(DEFAULT_TMP_PATH);
        let logs_path = Path::new(DEFAULT_LOG_PATH);
        let run_path = Path::new(DEFAULT_RUN_PATH);
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
            default_aws_root_cert_path: system_cert_path.clone().into(),
            default_c8y_root_cert_path: system_cert_path.into(),
            default_mapper_timestamp: Flag(true),
            default_mqtt_port: Port(DEFAULT_MQTT_PORT),
            default_http_port: Port(DEFAULT_HTTP_PORT),
            default_tmp_path: tmp_path.into(),
            default_logs_path: logs_path.into(),
            default_run_path: run_path.into(),
            default_device_type: DEFAULT_DEVICE_TYPE.into(),
            default_mqtt_bind_address: IpAddress::default(),
            default_http_bind_address: IpAddress::default(),
            default_c8y_smartrest_templates: TemplatesSet::default(),
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
            default_aws_root_cert_path: FilePath::from("/etc/ssl/certs"),
            default_c8y_root_cert_path: FilePath::from("/etc/ssl/certs"),
            default_mapper_timestamp: Flag(true),
            default_mqtt_port: Port(DEFAULT_MQTT_PORT),
            default_http_port: Port(DEFAULT_HTTP_PORT),
            default_tmp_path: FilePath::from("/tmp"),
            default_logs_path: FilePath::from("/var/log"),
            default_run_path: FilePath::from("/run"),
            default_device_type: DEFAULT_DEVICE_TYPE.into(),
            default_mqtt_bind_address: IpAddress::default(),
            default_http_bind_address: IpAddress::default(),
            default_c8y_smartrest_templates: TemplatesSet::default(),
        }
    );
}
