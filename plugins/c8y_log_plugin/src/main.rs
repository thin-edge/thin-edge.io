mod config;
mod error;
mod logfile_request;

use anyhow::Result;
use c8y_api::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use c8y_api::utils::bridge::{is_c8y_bridge_up, C8Y_BRIDGE_HEALTH_TOPIC};
use c8y_smartrest::smartrest_deserializer::{SmartRestLogRequest, SmartRestRequestGeneric};
use c8y_smartrest::topic::C8yTopic;
use clap::Parser;

use mqtt_channel::{Connection, Message, StreamExt, TopicFilter};
use std::path::{Path, PathBuf};
use tedge_config::{
    ConfigRepository, ConfigSettingAccessor, LogPathSetting, MqttPortSetting, TEdgeConfig,
    DEFAULT_TEDGE_CONFIG_PATH,
};
use tedge_utils::{
    file::{create_directory_with_user_group, create_file_with_user_group},
    fs_notify::{fs_notify_stream, pin_mut, FileEvent},
};
use thin_edge_json::health::{health_check_topics, send_health_status};
use tracing::{error, info};

use crate::config::LogPluginConfig;
use crate::logfile_request::{
    handle_dynamic_log_type_update, handle_logfile_request_operation, read_log_config,
};

const DEFAULT_PLUGIN_CONFIG_FILE: &str = "c8y/c8y-log-plugin.toml";
const AFTER_HELP_TEXT: &str = r#"On start, `c8y_log_plugin` notifies the cloud tenant of the log types listed in the `CONFIG_FILE`, sending this list with a `118` on `c8y/s/us`.
`c8y_log_plugin` subscribes then to `c8y/s/ds` listening for logfile operation requests (`522`) notifying the Cumulocity tenant of their progress (messages `501`, `502` and `503`).

The thin-edge `CONFIG_DIR` is used to store:
  * c8y-log-plugin.toml - the configuration file that specifies which logs to be retrieved"#;

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
    let mut topics: TopicFilter = health_check_topics("c8y-log-plugin");

    topics.add_unchecked(C8yTopic::SmartRestRequest.as_str());
    // subscribing also to c8y bridge health topic to know when the bridge is up
    topics.add(C8Y_BRIDGE_HEALTH_TOPIC)?;

    let mqtt_config = mqtt_channel::Config::default()
        .with_session_name("c8y-log-plugin")
        .with_port(mqtt_port)
        .with_subscriptions(topics);

    let mqtt_client = mqtt_channel::Connection::new(&mqtt_config).await?;
    Ok(mqtt_client)
}

pub async fn create_http_client(
    tedge_config: &TEdgeConfig,
) -> Result<JwtAuthHttpProxy, anyhow::Error> {
    let mut http_proxy = JwtAuthHttpProxy::try_new(tedge_config).await?;
    http_proxy.init().await?;
    Ok(http_proxy)
}

async fn run(
    config_dir: &Path,
    config_file_name: &str,
    mqtt_client: &mut Connection,
    http_client: &mut JwtAuthHttpProxy,
) -> Result<(), anyhow::Error> {
    let mut plugin_config = LogPluginConfig::default();

    let health_check_topics = health_check_topics("c8y-log-plugin");
    let config_file_path = config_dir.join(config_file_name);
    handle_dynamic_log_type_update(&plugin_config, mqtt_client).await?;

    let fs_notification_stream = fs_notify_stream(&[(
        config_dir,
        Some(config_file_name.to_string()),
        &[FileEvent::Modified, FileEvent::Deleted, FileEvent::Created],
    )])?;
    pin_mut!(fs_notification_stream);

    loop {
        tokio::select! {
                message = mqtt_client.received.next() => {
                if let Some(message) = message {
                    process_mqtt_message(message, &plugin_config, mqtt_client, http_client, &config_file_path, health_check_topics.clone()).await?;
                } else {
                    // message is None and the connection has been closed
                    return Ok(())
                }
            }
            Some(Ok((_path, mask))) = fs_notification_stream.next() => {
                match mask {
                    FileEvent::Created | FileEvent::Deleted | FileEvent::Modified => {
                        plugin_config = read_log_config(&config_file_path);
                        handle_dynamic_log_type_update(&plugin_config, mqtt_client).await?;
                    }
                }
            }
        }
    }
}

pub async fn process_mqtt_message(
    message: Message,
    plugin_config: &LogPluginConfig,
    mqtt_client: &mut Connection,
    http_client: &mut JwtAuthHttpProxy,
    config_file: &Path,
    health_check_topics: TopicFilter,
) -> Result<(), anyhow::Error> {
    if is_c8y_bridge_up(&message) {
        let plugin_config = read_log_config(config_file);
        handle_dynamic_log_type_update(&plugin_config, mqtt_client).await?;
    } else if health_check_topics.accept(&message) {
        send_health_status(&mut mqtt_client.published, "c8y-log-plugin").await;
    } else if let Ok(payload) = message.payload_str() {
        let result = match payload.split(',').next().unwrap_or_default() {
            "522" => {
                info!("Log request received: {payload}");
                // retrieve smartrest object from payload
                let maybe_smartrest_obj = SmartRestLogRequest::from_smartrest(payload);
                if let Ok(smartrest_obj) = maybe_smartrest_obj {
                    handle_logfile_request_operation(
                        &smartrest_obj,
                        plugin_config,
                        mqtt_client,
                        http_client,
                    )
                    .await
                } else {
                    error!("Incorrect SmartREST payload: {}", payload);
                    Ok(())
                }
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
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config_plugin_opt = LogfileRequestPluginOpt::parse();
    let config_dir = PathBuf::from(
        &config_plugin_opt
            .config_dir
            .to_str()
            .unwrap_or(DEFAULT_TEDGE_CONFIG_PATH),
    );

    tedge_utils::logging::initialise_tracing_subscriber(config_plugin_opt.debug);

    // Load tedge config from the provided location
    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(&config_plugin_opt.config_dir);
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
    let tedge_config = config_repository.load()?;

    let logs_dir = tedge_config.query(LogPathSetting)?;
    let logs_dir = PathBuf::from(logs_dir.to_string());

    if config_plugin_opt.init {
        init(&config_plugin_opt.config_dir, &logs_dir)?;
        return Ok(());
    }

    // Create required clients
    let mut mqtt_client = create_mqtt_client(&tedge_config).await?;
    let mut http_client = create_http_client(&tedge_config).await?;

    run(
        &config_dir,
        DEFAULT_PLUGIN_CONFIG_FILE,
        &mut mqtt_client,
        &mut http_client,
    )
    .await?;
    Ok(())
}

fn init(config_dir: &Path, logs_dir: &Path) -> Result<(), anyhow::Error> {
    info!("Creating supported operation files");
    create_init_logs_directories_and_files(config_dir, logs_dir)?;
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
/// - CONFIG_DIR/c8y/c8y-log-plugin.toml
fn create_init_logs_directories_and_files(
    config_dir: &Path,
    logs_dir: &Path,
) -> Result<(), anyhow::Error> {
    // creating logs_dir
    create_directory_with_user_group(
        format!("{}/tedge", logs_dir.display()),
        "tedge",
        "tedge",
        0o755,
    )?;
    create_directory_with_user_group(
        format!("{}/tedge/agent", logs_dir.display()),
        "tedge",
        "tedge",
        0o755,
    )?;
    // creating /operations/c8y directories
    create_directory_with_user_group(
        format!("{}/operations", config_dir.display()),
        "tedge",
        "tedge",
        0o755,
    )?;
    create_directory_with_user_group(
        format!("{}/operations/c8y", config_dir.display()),
        "tedge",
        "tedge",
        0o755,
    )?;
    // creating c8y_LogfileRequest operation file
    create_file_with_user_group(
        format!("{}/operations/c8y/c8y_LogfileRequest", config_dir.display()),
        "tedge",
        "tedge",
        0o644,
        None,
    )?;
    // creating c8y directory
    create_directory_with_user_group(
        format!("{}/c8y", config_dir.display()),
        "root",
        "root",
        0o1777,
    )?;

    // creating c8y-log-plugin.toml
    let logs_path = format!("{}/tedge/agent/software-*", logs_dir.display());
    let data = format!(
        r#"files = [
    {{ type = "software-management", path = "{logs_path}" }},
]"#
    );

    create_file_with_user_group(
        format!("{}/{DEFAULT_PLUGIN_CONFIG_FILE}", config_dir.display()),
        "root",
        "root",
        0o644,
        Some(&data),
    )?;

    Ok(())
}
