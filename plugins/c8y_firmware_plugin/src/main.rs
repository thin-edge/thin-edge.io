mod download;
mod error;
mod firmware_manager;
mod message;

#[cfg(test)]
mod tests;

use crate::download::DownloadManager;
use crate::download::DownloadRequest;
use crate::download::DownloadResponse;
use crate::error::FirmwareManagementError;
use crate::firmware_manager::create_directories;
use crate::firmware_manager::FirmwareManager;
use c8y_api::http_proxy::C8YHttpProxy;
use c8y_api::http_proxy::JwtAuthHttpProxy;
use clap::Parser;
use futures::channel::mpsc;
use std::path::PathBuf;
use std::time::Duration;
use tedge_config::system_services::get_log_level;
use tedge_config::system_services::set_log_level;
use tedge_config::ConfigRepository;
use tedge_config::ConfigSettingAccessor;
use tedge_config::DataPathSetting;
use tedge_config::DeviceIdSetting;
use tedge_config::FirmwareChildUpdateTimeoutSetting;
use tedge_config::HttpBindAddressSetting;
use tedge_config::HttpPortSetting;
use tedge_config::MqttClientHostSetting;
use tedge_config::MqttClientPortSetting;
use tedge_config::TEdgeConfig;
use tedge_config::TmpPathSetting;
use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;

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

    // Load tedge config from the provided location
    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(&fw_plugin_opt.config_dir);
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
    let tedge_config = config_repository.load()?;

    if fw_plugin_opt.init {
        init(&tedge_config)?;
        return Ok(());
    }

    let log_level = if fw_plugin_opt.debug {
        tracing::Level::TRACE
    } else {
        get_log_level(
            "c8y-firmware-plugin",
            &tedge_config_location.tedge_config_root_path,
        )?
    };
    set_log_level(log_level);

    let tedge_device_id = tedge_config.query(DeviceIdSetting)?;
    let mqtt_host = tedge_config.query(MqttClientHostSetting)?;
    let mqtt_port = tedge_config.query(MqttClientPortSetting)?.into();

    let http_client = create_http_client(&tedge_config).await?;
    let http_client = Box::new(http_client);

    let http_port: u16 = tedge_config.query(HttpPortSetting)?.into();
    let http_address = tedge_config.query(HttpBindAddressSetting)?.to_string();
    let local_http_host = format!("{}:{}", http_address, http_port);

    let tmp_dir: PathBuf = tedge_config.query(TmpPathSetting)?.into();
    let data_dir: PathBuf = tedge_config.query(DataPathSetting)?.into();

    let timeout_sec = Duration::from_secs(
        tedge_config
            .query(FirmwareChildUpdateTimeoutSetting)?
            .into(),
    );

    let (req_sndr, req_rcvr) = mpsc::unbounded::<DownloadRequest>();
    let (res_sndr, res_rcvr) = mpsc::unbounded::<DownloadResponse>();
    let mut download_manager = DownloadManager::new(http_client, tmp_dir, req_rcvr, res_sndr);

    let mut firmware_manager = FirmwareManager::new(
        tedge_device_id,
        mqtt_host,
        mqtt_port,
        req_sndr,
        res_rcvr,
        local_http_host,
        data_dir,
        timeout_sec,
    )
    .await?;

    tokio::spawn(async move { download_manager.run().await });

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

fn init(tedge_config: &TEdgeConfig) -> Result<(), FirmwareManagementError> {
    let data_dir: PathBuf = tedge_config.query(DataPathSetting)?.into();
    create_directories(data_dir)?;
    Ok(())
}
