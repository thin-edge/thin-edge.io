mod child_device;
mod common;
mod download;
mod error;
mod firmware_manager;

use crate::firmware_manager::FirmwareManager;
use c8y_api::http_proxy::C8YHttpProxy;
use c8y_api::http_proxy::JwtAuthHttpProxy;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tedge_config::system_services::get_log_level;
use tedge_config::system_services::set_log_level;
use tedge_config::ConfigRepository;
use tedge_config::ConfigSettingAccessor;
use tedge_config::DeviceIdSetting;
use tedge_config::FirmwareTimeoutSetting;
use tedge_config::HttpBindAddressSetting;
use tedge_config::HttpPortSetting;
use tedge_config::MqttPortSetting;
use tedge_config::TEdgeConfig;
use tedge_config::TmpPathSetting;
use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;
use tokio::sync::Mutex;

// FIXME!: Think of good text
const AFTER_HELP_TEXT: &str = r#"We will write later!"#;

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

    /// Do nothing as of now
    #[clap(short, long)]
    pub init: bool,

    #[clap(long = "config-dir", default_value = DEFAULT_TEDGE_CONFIG_PATH)]
    pub config_dir: PathBuf,
}

pub async fn create_http_client(
    tedge_config: &TEdgeConfig,
) -> Result<JwtAuthHttpProxy, anyhow::Error> {
    let mut http_proxy = JwtAuthHttpProxy::try_new(tedge_config).await?;
    http_proxy.init().await?;
    Ok(http_proxy)
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let fw_plugin_opt = FirmwarePluginOpt::parse();

    if fw_plugin_opt.init {
        // Placeholder for the future enhancement
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
    let http_client: Arc<Mutex<dyn C8YHttpProxy>> = Arc::new(Mutex::new(http_client));

    let http_port: u16 = tedge_config.query(HttpPortSetting)?.into();
    let http_address = tedge_config.query(HttpBindAddressSetting)?.to_string();
    let local_http_host = format!("{}:{}", http_address, http_port);

    let tmp_dir = tedge_config.query(TmpPathSetting)?.into();
    let timeout_sec = Duration::from_secs(tedge_config.query(FirmwareTimeoutSetting)?.into());

    let mut firmware_manager = FirmwareManager::new(
        tedge_device_id,
        mqtt_port,
        http_client,
        local_http_host,
        tmp_dir,
        timeout_sec,
    )
    .await?;

    firmware_manager.run().await
}
