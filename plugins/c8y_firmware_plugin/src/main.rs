mod error;
mod firmware_manager;
mod message;

#[cfg(test)]
mod tests;

use crate::error::FirmwareManagementError;
use crate::firmware_manager::FirmwareManager;
use c8y_api::http_proxy::C8YHttpProxy;
use c8y_api::http_proxy::JwtAuthHttpProxy;
use clap::Parser;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tedge_config::system_services::get_log_level;
use tedge_config::system_services::set_log_level;
use tedge_config::ConfigRepository;
use tedge_config::ConfigSettingAccessor;
use tedge_config::DeviceIdSetting;
use tedge_config::FirmwareChildUpdateTimeoutSetting;
use tedge_config::HttpBindAddressSetting;
use tedge_config::HttpPortSetting;
use tedge_config::MqttPortSetting;
use tedge_config::TEdgeConfig;
use tedge_config::TmpPathSetting;
use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;
use tedge_utils::file::create_directory_with_user_group;
use tracing::info;

// TODO! We should make it configurable by tedge config later.
const PERSISTENT_DIR_PATH: &str = "/var/tedge";

pub const CACHE_DIR_NAME: &str = "cache";
pub const FILE_TRANSFER_DIR_NAME: &str = "file-transfer";
pub const PERSISTENT_STORE_DIR_NAME: &str = "firmware";

const AFTER_HELP_TEXT: &str = r#"`c8y-firmware-plugin` subscribes to `c8y/s/ds` listening for firmware operation requests (message `515`).
Notifying the Cumulocity tenant of their progress (messages `501`, `502` and `503`).
During a successful operation, `c8y-firmware-plugin` updates the installed firmware info in Cumulocity tenant with SmartREST message `115`.

The thin-edge `CONFIG_DIR` is used to find where:
  * to store temporary files on download: `tedge config get tmp.path`,
  * to log operation errors and progress: `tedge config get log.path`,
  * to connect the MQTT bus: `tedge config get mqtt.port`,
  * to timeout pending operations: `tedge config get firmware.child.update.timeout"#;

#[derive(Debug, clap::Parser)]
#[clap(
name = clap::crate_name!(),
version = clap::crate_version!(),
about = clap::crate_description!(),
after_help = AFTER_HELP_TEXT
)]
pub struct FirmwarePluginOpt {
    /// Turn-on the debug log level.
    ///
    /// If off only reports ERROR, WARN, and INFO
    /// If on also reports DEBUG and TRACE
    #[clap(long)]
    pub debug: bool,

    /// Create required directories
    #[clap(short, long)]
    pub init: bool,

    #[clap(long = "config-dir", default_value = DEFAULT_TEDGE_CONFIG_PATH)]
    pub config_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), FirmwareManagementError> {
    let fw_plugin_opt = FirmwarePluginOpt::parse();

    if fw_plugin_opt.init {
        init(Path::new(PERSISTENT_DIR_PATH))?;
        return Ok(());
    }

    // Load tedge config from the provided location
    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(&fw_plugin_opt.config_dir);
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());

    let log_level = if fw_plugin_opt.debug {
        tracing::Level::TRACE
    } else {
        get_log_level(
            "c8y-firmware-plugin",
            tedge_config_location.tedge_config_root_path.to_path_buf(),
        )?
    };
    set_log_level(log_level);

    let tedge_config = config_repository.load()?;

    let tedge_device_id = tedge_config.query(DeviceIdSetting)?;
    let mqtt_port = tedge_config.query(MqttPortSetting)?.into();

    let http_client = create_http_client(&tedge_config).await?;
    let http_client = Box::new(http_client);

    let http_port: u16 = tedge_config.query(HttpPortSetting)?.into();
    let http_address = tedge_config.query(HttpBindAddressSetting)?.to_string();
    let local_http_host = format!("{}:{}", http_address, http_port);

    let tmp_dir = tedge_config.query(TmpPathSetting)?.into();
    let timeout_sec = Duration::from_secs(
        tedge_config
            .query(FirmwareChildUpdateTimeoutSetting)?
            .into(),
    );

    let mut firmware_manager = FirmwareManager::new(
        tedge_device_id,
        mqtt_port,
        http_client,
        local_http_host,
        PathBuf::from(PERSISTENT_DIR_PATH),
        tmp_dir,
        timeout_sec,
    )
    .await?;

    firmware_manager.init().await?;
    firmware_manager.run().await?;

    Ok(())
}

pub async fn create_http_client(
    tedge_config: &TEdgeConfig,
) -> Result<JwtAuthHttpProxy, FirmwareManagementError> {
    let mut http_proxy = JwtAuthHttpProxy::try_new(tedge_config).await?;
    http_proxy.init().await?;
    Ok(http_proxy)
}

fn init(cfg_dir: &Path) -> Result<(), FirmwareManagementError> {
    info!("Creating required directories for c8y-firmware-plugin.");
    create_directories(cfg_dir)?;
    Ok(())
}

fn create_directories(persistent_dir: &Path) -> Result<(), FirmwareManagementError> {
    create_directory_with_user_group(
        format!("{}/{}", persistent_dir.display(), CACHE_DIR_NAME),
        "tedge",
        "tedge",
        0o755,
    )?;
    create_directory_with_user_group(
        format!("{}/{}", persistent_dir.display(), FILE_TRANSFER_DIR_NAME),
        "tedge",
        "tedge",
        0o755,
    )?;
    create_directory_with_user_group(
        format!("{}/{}", persistent_dir.display(), PERSISTENT_STORE_DIR_NAME),
        "tedge",
        "tedge",
        0o755,
    )?;
    Ok(())
}
