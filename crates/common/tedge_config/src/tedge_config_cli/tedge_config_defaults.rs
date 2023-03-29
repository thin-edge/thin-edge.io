use crate::seconds::Seconds;
use crate::tedge_config_cli::models::IpAddress;
use crate::tedge_config_cli::models::TemplatesSet;
use crate::Flag;
use crate::Port;
use crate::TEdgeConfigLocation;
use camino::Utf8Path;
use camino::Utf8PathBuf;

const DEFAULT_ETC_PATH: &str = "/etc";
const DEFAULT_MQTT_PORT: u16 = 1883;
const DEFAULT_HTTP_PORT: u16 = 8000;
pub const DEFAULT_TMP_PATH: &str = "/tmp";
pub const DEFAULT_LOG_PATH: &str = "/var/log";
pub const DEFAULT_RUN_PATH: &str = "/run";
pub const DEFAULT_DATA_PATH: &str = "/var/tedge";
const DEFAULT_DEVICE_TYPE: &str = "thin-edge.io";
const DEFAULT_FIRMWARE_CHILD_UPDATE_TIMEOUT_SEC: u64 = 3600;
const DEFAULT_SERVICE_TYPE: &str = "service";

pub const DEFAULT_FILE_TRANSFER_DIR_NAME: &str = "file-transfer";

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
    pub default_device_cert_path: Utf8PathBuf,

    /// Default device key path
    pub default_device_key_path: Utf8PathBuf,

    /// Default path for azure root certificates
    pub default_azure_root_cert_path: Utf8PathBuf,

    /// Default path for AWS root certificates
    pub default_aws_root_cert_path: Utf8PathBuf,

    /// Default path for c8y root certificates
    pub default_c8y_root_cert_path: Utf8PathBuf,

    /// Default c8y smartrest templates
    pub default_c8y_smartrest_templates: TemplatesSet,

    /// Default mapper timestamp bool
    pub default_mapper_timestamp: Flag,

    /// Default port for mqtt internal listener
    pub default_mqtt_port: Port,

    /// Default port for http file transfer service
    pub default_http_port: Port,

    /// Default tmp path
    pub default_tmp_path: Utf8PathBuf,

    /// Default log path
    pub default_logs_path: Utf8PathBuf,

    /// Default run path
    pub default_run_path: Utf8PathBuf,

    /// Default run path
    pub default_data_path: Utf8PathBuf,

    /// Default device type
    pub default_device_type: String,

    /// Default mqtt bind address
    pub default_mqtt_bind_address: IpAddress,

    /// Default mqtt broker host used by mqtt clients
    pub default_mqtt_client_host: String,

    /// Default http bind address
    pub default_http_bind_address: IpAddress,

    /// Default firmware child device operation timeout in seconds
    pub default_firmware_child_update_timeout: Seconds,

    /// Default service type
    pub default_service_type: String,
    /// Default lock files bool
    pub default_lock_files: Flag,
}

impl From<&TEdgeConfigLocation> for TEdgeConfigDefaults {
    fn from(config_location: &TEdgeConfigLocation) -> Self {
        let system_cert_path = Utf8Path::new(DEFAULT_ETC_PATH).join("ssl").join("certs");
        let tmp_path = Utf8Path::new(DEFAULT_TMP_PATH);
        let logs_path = Utf8Path::new(DEFAULT_LOG_PATH);
        let run_path = Utf8Path::new(DEFAULT_RUN_PATH);
        let data_path = Utf8Path::new(DEFAULT_DATA_PATH);
        Self {
            default_device_cert_path: config_location
                .tedge_config_root_path()
                .join("device-certs")
                .join("tedge-certificate.pem"),
            default_device_key_path: config_location
                .tedge_config_root_path()
                .join("device-certs")
                .join("tedge-private-key.pem"),
            default_azure_root_cert_path: system_cert_path.clone(),
            default_aws_root_cert_path: system_cert_path.clone(),
            default_c8y_root_cert_path: system_cert_path,
            default_mapper_timestamp: Flag(true),
            default_mqtt_port: Port(DEFAULT_MQTT_PORT),
            default_http_port: Port(DEFAULT_HTTP_PORT),
            default_tmp_path: tmp_path.into(),
            default_logs_path: logs_path.into(),
            default_run_path: run_path.into(),
            default_data_path: data_path.into(),
            default_device_type: DEFAULT_DEVICE_TYPE.into(),
            default_mqtt_client_host: "localhost".into(),
            default_mqtt_bind_address: IpAddress::default(),
            default_http_bind_address: IpAddress::default(),
            default_c8y_smartrest_templates: TemplatesSet::default(),
            default_firmware_child_update_timeout: Seconds(
                DEFAULT_FIRMWARE_CHILD_UPDATE_TIMEOUT_SEC,
            ),
            default_service_type: DEFAULT_SERVICE_TYPE.into(),
            default_lock_files: Flag(true),
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
            default_device_cert_path: Utf8PathBuf::from(
                "/opt/etc/_tedge/device-certs/tedge-certificate.pem"
            ),
            default_device_key_path: Utf8PathBuf::from(
                "/opt/etc/_tedge/device-certs/tedge-private-key.pem"
            ),
            default_azure_root_cert_path: Utf8PathBuf::from("/etc/ssl/certs"),
            default_aws_root_cert_path: Utf8PathBuf::from("/etc/ssl/certs"),
            default_c8y_root_cert_path: Utf8PathBuf::from("/etc/ssl/certs"),
            default_mapper_timestamp: Flag(true),
            default_mqtt_port: Port(DEFAULT_MQTT_PORT),
            default_http_port: Port(DEFAULT_HTTP_PORT),
            default_tmp_path: Utf8PathBuf::from("/tmp"),
            default_logs_path: Utf8PathBuf::from("/var/log"),
            default_run_path: Utf8PathBuf::from("/run"),
            default_data_path: Utf8PathBuf::from("/var/tedge"),
            default_device_type: DEFAULT_DEVICE_TYPE.into(),
            default_mqtt_client_host: "localhost".to_string(),
            default_mqtt_bind_address: IpAddress::default(),
            default_http_bind_address: IpAddress::default(),
            default_c8y_smartrest_templates: TemplatesSet::default(),
            default_firmware_child_update_timeout: Seconds(
                DEFAULT_FIRMWARE_CHILD_UPDATE_TIMEOUT_SEC
            ),
            default_service_type: DEFAULT_SERVICE_TYPE.into(),
            default_lock_files: Flag(true),
        }
    );
}
