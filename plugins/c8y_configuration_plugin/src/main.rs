mod child_device;
mod config;
mod config_manager;
mod download;
mod error;
mod operation;
mod topic;
mod upload;

#[cfg(test)]
mod tests;

use crate::config::PluginConfig;

use anyhow::Result;
use c8y_api::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use clap::Parser;
use config_manager::ConfigManager;
use tedge_config::system_services::{get_log_level, set_log_level};
use tokio::sync::Mutex;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tedge_config::{
    ConfigRepository, ConfigSettingAccessor, DeviceIdSetting, HttpBindAddressSetting,
    HttpPortSetting, MqttPortSetting, TEdgeConfig, TmpPathSetting, DEFAULT_TEDGE_CONFIG_PATH,
};
use tedge_utils::file::{create_directory_with_user_group, create_file_with_user_group};
use tracing::{error, info};

const AFTER_HELP_TEXT: &str = r#"On start, `c8y_configuration_plugin` notifies the cloud tenant of the managed configuration files, listed in the `CONFIG_FILE`, sending this list with a `119` on `c8y/s/us`.
`c8y_configuration_plugin` subscribes then to `c8y/s/ds` listening for configuration operation requests (messages `524` and `526`).
notifying the Cumulocity tenant of their progress (messages `501`, `502` and `503`).

The thin-edge `CONFIG_DIR` is used to find where:
  * to store temporary files on download: `tedge config get tmp.path`,
  * to log operation errors and progress: `tedge config get log.path`,
  * to connect the MQTT bus: `tedge config get mqtt.port`."#;

#[derive(Debug, clap::Parser)]
#[clap(
name = clap::crate_name!(),
version = clap::crate_version!(),
about = clap::crate_description!(),
after_help = AFTER_HELP_TEXT
)]
pub struct ConfigPluginOpt {
    /// Turn-on the debug log level.
    ///
    /// If off only reports ERROR, WARN, and INFO
    /// If on also reports DEBUG and TRACE
    #[clap(long)]
    pub debug: bool,

    /// Create supported operation files
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
    let config_plugin_opt = ConfigPluginOpt::parse();

    if config_plugin_opt.init {
        init(config_plugin_opt.config_dir)?;
        return Ok(());
    }

    // Load tedge config from the provided location
    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(&config_plugin_opt.config_dir);
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());

    let log_level = if config_plugin_opt.debug {
        tracing::Level::TRACE
    } else {
        get_log_level(
            "c8y_configuration_plugin",
            tedge_config_location.tedge_config_root_path.to_path_buf(),
        )?
    };
    set_log_level(log_level);

    let tedge_config = config_repository.load()?;

    let tedge_device_id = tedge_config.query(DeviceIdSetting)?;

    let mqtt_port = tedge_config.query(MqttPortSetting)?.into();
    let http_client = create_http_client(&tedge_config).await?;
    let http_client: Arc<Mutex<dyn C8YHttpProxy>> = Arc::new(Mutex::new(http_client));
    let tmp_dir = tedge_config.query(TmpPathSetting)?.into();

    let http_port: u16 = tedge_config.query(HttpPortSetting)?.into();
    let http_address = tedge_config.query(HttpBindAddressSetting)?.to_string();
    let local_http_host = format!("{}:{}", http_address, http_port);

    let mut config_manager = ConfigManager::new(
        tedge_device_id,
        mqtt_port,
        http_client,
        local_http_host,
        tmp_dir,
        config_plugin_opt.config_dir,
    )
    .await?;

    config_manager.run().await
}

fn init(cfg_dir: PathBuf) -> Result<(), anyhow::Error> {
    info!("Creating supported operation files");
    create_operation_files(&cfg_dir)?;
    Ok(())
}

fn create_operation_files(config_dir: &Path) -> Result<(), anyhow::Error> {
    create_directory_with_user_group(
        format!("{}/c8y", config_dir.display()),
        "root",
        "root",
        0o1777,
    )?;
    let example_config = r#"# Add the configurations to be managed by c8y-configuration-plugin

files = [
#    { path = '/etc/tedge/tedge.toml' },
#    { path = '/etc/tedge/mosquitto-conf/c8y-bridge.conf', type = 'c8y-bridge.conf' },
#    { path = '/etc/tedge/mosquitto-conf/tedge-mosquitto.conf', type = 'tedge-mosquitto.conf' },
#    { path = '/etc/mosquitto/mosquitto.conf', type = 'mosquitto.conf' },
#    { path = '/etc/tedge/c8y/example.txt', type = 'example', user = 'tedge', group = 'tedge', mode = 0o444 }
]"#;

    create_file_with_user_group(
        format!("{}/c8y/c8y-configuration-plugin.toml", config_dir.display()),
        "root",
        "root",
        0o644,
        Some(example_config),
    )?;

    create_directory_with_user_group(
        format!("{}/operations/c8y", config_dir.display()),
        "tedge",
        "tedge",
        0o775,
    )?;
    create_file_with_user_group(
        format!(
            "{}/operations/c8y/c8y_UploadConfigFile",
            config_dir.display()
        ),
        "tedge",
        "tedge",
        0o644,
        None,
    )?;
    create_file_with_user_group(
        format!(
            "{}/operations/c8y/c8y_DownloadConfigFile",
            config_dir.display()
        ),
        "tedge",
        "tedge",
        0o644,
        None,
    )?;
    Ok(())
}
