mod config;
mod download;
mod error;
mod smartrest;
mod upload;

use crate::config::PluginConfig;
use crate::download::handle_config_download_request;
use crate::upload::handle_config_upload_request;
use anyhow::Result;
use c8y_api::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use c8y_smartrest::smartrest_deserializer::{
    SmartRestConfigDownloadRequest, SmartRestConfigUploadRequest, SmartRestRequestGeneric,
};
use c8y_smartrest::topic::C8yTopic;
use clap::Parser;
use mqtt_channel::{SinkExt, StreamExt};
use std::path::PathBuf;
use tedge_config::{
    ConfigRepository, ConfigSettingAccessor, MqttPortSetting, TEdgeConfig,
    DEFAULT_TEDGE_CONFIG_PATH,
};
use tedge_utils::file::{create_directory_with_user_group, create_file_with_user_group};
use tracing::{debug, error, info};

const DEFAULT_PLUGIN_CONFIG_FILE_PATH: &str = "/etc/tedge/c8y/c8y-configuration-plugin.toml";
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

    #[clap(long = "config-file", default_value = DEFAULT_PLUGIN_CONFIG_FILE_PATH)]
    pub config_file: PathBuf,
}

async fn create_mqtt_client(
    tedge_config: &TEdgeConfig,
) -> Result<mqtt_channel::Connection, anyhow::Error> {
    let mqtt_port = tedge_config.query(MqttPortSetting)?.into();
    let mqtt_config = mqtt_channel::Config::default()
        .with_port(mqtt_port)
        .with_subscriptions(mqtt_channel::TopicFilter::new_unchecked(
            C8yTopic::SmartRestRequest.as_str(),
        ));

    let mqtt_client = mqtt_channel::Connection::new(&mqtt_config).await?;
    Ok(mqtt_client)
}

pub async fn create_http_client(
    tedge_config: &TEdgeConfig,
) -> Result<JwtAuthHttpProxy, anyhow::Error> {
    let mut http_proxy = JwtAuthHttpProxy::try_new(tedge_config).await?;
    let () = http_proxy.init().await?;
    Ok(http_proxy)
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config_plugin_opt = ConfigPluginOpt::parse();
    tedge_utils::logging::initialise_tracing_subscriber(config_plugin_opt.debug);

    if config_plugin_opt.init {
        init(config_plugin_opt.config_dir)?;
        return Ok(());
    }

    // Load tedge config from the provided location
    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(config_plugin_opt.config_dir);
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
    let tedge_config = config_repository.load()?;

    // Create required clients
    let mut mqtt_client = create_mqtt_client(&tedge_config).await?;
    let mut http_client = create_http_client(&tedge_config).await?;

    let plugin_config = PluginConfig::new(config_plugin_opt.config_file);

    // Publish supported configuration types
    let msg = plugin_config.to_message()?;
    let () = mqtt_client.published.send(msg).await?;

    // Mqtt message loop
    while let Some(message) = mqtt_client.received.next().await {
        debug!("Received {:?}", message);
        if let Ok(payload) = message.payload_str() {
            let result = match payload.split(',').next().unwrap_or_default() {
                "524" => {
                    let config_download_request =
                        SmartRestConfigDownloadRequest::from_smartrest(payload)?;

                    handle_config_download_request(
                        &plugin_config,
                        config_download_request,
                        &tedge_config,
                        &mut mqtt_client,
                        &mut http_client,
                    )
                    .await
                }
                "526" => {
                    // retrieve config file upload smartrest request from payload
                    let config_upload_request =
                        SmartRestConfigUploadRequest::from_smartrest(payload)?;

                    // handle the config file upload request
                    handle_config_upload_request(
                        config_upload_request,
                        &mut mqtt_client,
                        &mut http_client,
                    )
                    .await
                }
                _ => {
                    // Ignore operation messages not meant for this plugin
                    Ok(())
                }
            };

            if let Err(err) = result {
                error!("Handling of operation: '{}' failed with {}", payload, err);
            }
        }
    }

    mqtt_client.close().await;

    Ok(())
}

fn init(cfg_dir: PathBuf) -> Result<(), anyhow::Error> {
    info!("Creating supported operation files");
    let config_dir = cfg_dir.as_path().display().to_string();
    let () = create_operation_files(config_dir.as_str())?;
    Ok(())
}

fn create_operation_files(config_dir: &str) -> Result<(), anyhow::Error> {
    create_directory_with_user_group(
        &format!("{config_dir}/operations/c8y"),
        "tedge",
        "tedge",
        0o775,
    )?;
    create_file_with_user_group(
        &format!("{config_dir}/operations/c8y/c8y_UploadConfigFile"),
        "tedge",
        "tedge",
        0o644,
    )?;
    create_file_with_user_group(
        &format!("{config_dir}/operations/c8y/c8y_DownloadConfigFile"),
        "tedge",
        "tedge",
        0o644,
    )?;
    Ok(())
}
