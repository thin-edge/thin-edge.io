mod config;
mod error;
mod logfile_request;

use anyhow::Result;
use c8y_api::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use c8y_smartrest::smartrest_deserializer::{SmartRestLogRequest, SmartRestRequestGeneric};
use c8y_smartrest::topic::C8yTopic;
use clap::Parser;

use inotify::{EventMask, EventStream};
use inotify::{Inotify, WatchMask};
use mqtt_channel::{Connection, StreamExt};
use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};
use tedge_config::{
    ConfigRepository, ConfigSettingAccessor, LogPathSetting, MqttPortSetting, TEdgeConfig,
    DEFAULT_TEDGE_CONFIG_PATH,
};
use tedge_utils::file::{create_directory_with_user_group, create_file_with_user_group};
use tracing::{error, info};

use crate::logfile_request::{handle_dynamic_log_type_update, handle_logfile_request_operation};

const DEFAULT_PLUGIN_CONFIG_FILE: &str = "c8y/c8y-log-plugin.toml";
const AFTER_HELP_TEXT: &str = r#"On start, `c8y_log_plugin` notifies the cloud tenant of the log types listed in the `CONFIG_FILE`, sending this list with a `118` on `c8y/s/us`.
`c8y_log_plugin` subscribes then to `c8y/s/ds` listening for logfile operation requests (`522`) notifying the Cumulocity tenant of their progress (messages `501`, `502` and `503`).

The thin-edge `CONFIG_DIR` is used to store:
  * c8y-log-plugin.toml - the configuration file that specifies which logs to be retrived"#;

#[derive(Debug, clap::Parser, Clone)]
#[clap(
name = clap::crate_name!(),
version = clap::crate_version!(),
about = clap::crate_description!(),
after_help = AFTER_HELP_TEXT
)]
pub struct LogfileRequestPluginOpt {
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

fn create_inofity_file_watch_stream(
    config_file: &Path,
) -> Result<EventStream<[u8; 1024]>, anyhow::Error> {
    let buffer = [0; 1024];
    let mut inotify = Inotify::init().expect("Error while initializing inotify instance");

    inotify
        .add_watch(&config_file, WatchMask::CLOSE_WRITE)
        .expect("Failed to add file watch");

    Ok(inotify.event_stream(buffer)?)
}

async fn run(
    config_file: &Path,
    mqtt_client: &mut Connection,
    http_client: &mut JwtAuthHttpProxy,
) -> Result<(), anyhow::Error> {
    let mut plugin_config = handle_dynamic_log_type_update(mqtt_client, config_file).await?;

    let mut inotify_stream = create_inofity_file_watch_stream(config_file)?;

    loop {
        tokio::select! {
                message = mqtt_client.received.next() => {
                if let Some(message) = message {
                    if let Ok(payload) = message.payload_str() {
                        let result = match payload.split(',').next().unwrap_or_default() {
                            "522" => {
                                info!("Log request received: {payload}");
                                // retrieve smartrest object from payload
                                let smartrest_obj = SmartRestLogRequest::from_smartrest(payload)?;
                                handle_logfile_request_operation(
                                    &smartrest_obj,
                                    &plugin_config,
                                    mqtt_client,
                                    http_client,
                                )
                                .await
                            }
                            _ => {
                                // Ignore operation messages not meant for this plugin
                                Ok(())
                            }
                        };

                        if let Err(err) = result {
                            let error_message = format!("Handling of operation: '{}' failed with {}", payload, err);
                            error!("{}", error_message);
                        }
                    }
                }
                else {
                    // message is None and the connection has been closed
                    return Ok(());
                }
            }
            Some(Ok(event)) = inotify_stream.next() => {
                if event.mask == EventMask::CLOSE_WRITE {
                    plugin_config = handle_dynamic_log_type_update(mqtt_client, config_file).await?;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config_plugin_opt = LogfileRequestPluginOpt::parse();
    let config_file = PathBuf::from(&format!(
        "{}/{DEFAULT_PLUGIN_CONFIG_FILE}",
        &config_plugin_opt
            .config_dir
            .to_str()
            .unwrap_or(DEFAULT_TEDGE_CONFIG_PATH)
    ));

    tedge_utils::logging::initialise_tracing_subscriber(config_plugin_opt.debug);

    // Load tedge config from the provided location
    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(&config_plugin_opt.config_dir);
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
    let tedge_config = config_repository.load()?;

    let logs_dir = tedge_config.query(LogPathSetting)?;
    let logs_dir = PathBuf::from(logs_dir.to_string());

    if config_plugin_opt.init {
        let () = init(&config_plugin_opt.config_dir, &logs_dir)?;
        return Ok(());
    }

    // Create required clients
    let mut mqtt_client = create_mqtt_client(&tedge_config).await?;
    let mut http_client = create_http_client(&tedge_config).await?;

    let () = run(&config_file, &mut mqtt_client, &mut http_client).await?;
    Ok(())
}

fn init(config_dir: &Path, logs_dir: &Path) -> Result<(), anyhow::Error> {
    info!("Creating supported operation files");
    let config_dir = config_dir.display().to_string();
    let logs_dir = logs_dir.display().to_string();
    let () = create_init_logs_directories_and_files(config_dir.as_str(), logs_dir.as_str())?;
    Ok(())
}

/// append the log plugin file with software-management logs
/// assumes file is already created.
fn create_default_log_plugin_file(path_to_toml: &str, logs_dir: &str) -> Result<(), anyhow::Error> {
    let logs_path = format!("{logs_dir}/tedge/agent/software-*");
    let data = toml::toml! {
        files = [
            { type = "software-management", path = logs_path }
        ]
    };

    let mut toml_file = OpenOptions::new()
        .append(true)
        .create(false)
        .open(path_to_toml)
        .map_err(|error| {
            anyhow::anyhow!("Unable to open file: {}. Error: {}", path_to_toml, error)
        })?;
    toml_file.write_all(data.to_string().as_bytes())?;
    Ok(())
}

/// for the log plugin to work the following directories and files are needed:
///
/// Directories:
/// - LOGS_DIR/tedge/agent
/// - CONFIG_DIR/operations/c8y
/// - CONFIG_DIR/c8y
///
/// Files:
/// - CONFIG_DIR/operations/c8y/c8y_LogfileRequest
/// - CONFIG_DIR/c8y/log/c8y-log-plugin.toml
fn create_init_logs_directories_and_files(
    config_dir: &str,
    logs_dir: &str,
) -> Result<(), anyhow::Error> {
    // creating logs_dir
    create_directory_with_user_group(&format!("{logs_dir}/tedge"), "tedge", "tedge", 0o755)?;
    create_directory_with_user_group(&format!("{logs_dir}/tedge/agent"), "tedge", "tedge", 0o755)?;
    // creating /operations/c8y directories
    create_directory_with_user_group(&format!("{config_dir}/operations"), "tedge", "tedge", 0o755)?;
    create_directory_with_user_group(
        &format!("{config_dir}/operations/c8y"),
        "tedge",
        "tedge",
        0o755,
    )?;
    // creating c8y_LogfileRequest operation file
    create_file_with_user_group(
        &format!("{config_dir}/operations/c8y/c8y_LogfileRequest"),
        "tedge",
        "tedge",
        0o755,
    )?;
    // creating c8y directory
    create_directory_with_user_group(&format!("{config_dir}/c8y"), "tedge", "tedge", 0o755)?;
    // creating c8y-log-plugin.toml

    // NOTE: file needs 775 permission or inotify can not watch for changes inside the file
    create_file_with_user_group(
        &format!("{config_dir}/{DEFAULT_PLUGIN_CONFIG_FILE}"),
        "tedge",
        "tedge",
        0o775,
    )?;

    // append default content to c8y-log-plugin.toml
    create_default_log_plugin_file(
        &format!("{config_dir}/{DEFAULT_PLUGIN_CONFIG_FILE}"),
        logs_dir,
    )?;
    Ok(())
}
