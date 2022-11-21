mod child_device;
mod config;
mod download;
mod error;
mod operation;
mod topic;
mod upload;

#[cfg(test)]
mod tests;

use crate::upload::handle_config_upload_request;
use crate::{
    child_device::ConfigOperationResponse,
    download::{
        handle_child_device_config_update_response, handle_config_download_request,
        DownloadConfigFileStatusMessage,
    },
};
use crate::{config::PluginConfig, upload::handle_child_device_config_snapshot_response};

use anyhow::Result;
use c8y_api::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use c8y_api::smartrest::smartrest_deserializer::{
    SmartRestConfigDownloadRequest, SmartRestConfigUploadRequest, SmartRestRequestGeneric,
};
use c8y_api::smartrest::smartrest_serializer::TryIntoOperationStatusMessage;
use c8y_api::smartrest::topic::C8yTopic;
use child_device::get_child_id_from_child_topic;
use clap::Parser;
use mqtt_channel::{Connection, Message, MqttError, SinkExt, StreamExt, Topic, TopicFilter};
use operation::ConfigOperation;
use upload::UploadConfigFileStatusMessage;

use std::path::{Path, PathBuf};
use tedge_config::{
    ConfigRepository, ConfigSettingAccessor, DeviceIdSetting, HttpPortSetting, IpAddress,
    MqttBindAddressSetting, MqttExternalBindAddressSetting, MqttPortSetting, TEdgeConfig,
    TmpPathSetting, DEFAULT_TEDGE_CONFIG_PATH,
};
use tedge_utils::{
    file::{create_directory_with_user_group, create_file_with_user_group},
    notify::fs_notify_stream,
    paths::PathsError,
};
use thin_edge_json::health::{health_check_topics, send_health_status};
use topic::ConfigOperationResponseTopic;

use tedge_utils::notify::FsEvent;
use tracing::{error, info};

pub const DEFAULT_PLUGIN_CONFIG_FILE_NAME: &str = "c8y-configuration-plugin.toml";
pub const DEFAULT_OPERATION_DIR_NAME: &str = "c8y/";
pub const DEFAULT_PLUGIN_CONFIG_TYPE: &str = "c8y-configuration-plugin";
pub const CONFIG_CHANGE_TOPIC: &str = "tedge/configuration_change";

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

async fn create_mqtt_client(mqtt_port: u16) -> Result<mqtt_channel::Connection, anyhow::Error> {
    let mut topic_filter =
        mqtt_channel::TopicFilter::new_unchecked(&C8yTopic::SmartRestRequest.to_string());
    topic_filter.add_all(health_check_topics("c8y-configuration-plugin"));

    topic_filter.add_all(ConfigOperationResponseTopic::SnapshotResponse.into());
    topic_filter.add_all(ConfigOperationResponseTopic::UpdateResponse.into());

    let mqtt_config = mqtt_channel::Config::default()
        .with_session_name("c8y-configuration-plugin")
        .with_port(mqtt_port)
        .with_subscriptions(topic_filter);

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
        tedge_config::TEdgeConfigLocation::from_custom_root(&config_plugin_opt.config_dir);
    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
    let tedge_config = config_repository.load()?;

    let tedge_device_id = tedge_config.query(DeviceIdSetting)?;

    // match to external bind address if there is one,
    // otherwise match to internal bind address
    let internal_bind_address: IpAddress = tedge_config.query(MqttBindAddressSetting)?;
    let external_bind_address_or_err = tedge_config.query(MqttExternalBindAddressSetting);
    let bind_address = match external_bind_address_or_err {
        Ok(external_bind_address) => external_bind_address,
        Err(_) => internal_bind_address,
    };

    let mqtt_port = tedge_config.query(MqttPortSetting)?.into();
    let mut http_client = create_http_client(&tedge_config).await?;
    let tmp_dir = tedge_config.query(TmpPathSetting)?.into();
    let http_port: u16 = tedge_config.query(HttpPortSetting)?.into();

    let local_http_host = format!("{}:{}", bind_address.to_string().as_str(), http_port);

    run(
        tedge_device_id,
        mqtt_port,
        &mut http_client,
        &local_http_host,
        tmp_dir,
        &config_plugin_opt.config_dir,
    )
    .await
}

async fn run(
    tedge_device_id: String,
    mqtt_port: u16,
    http_client: &mut impl C8YHttpProxy,
    local_http_host: &str,
    tmp_dir: PathBuf,
    config_dir: &Path,
) -> Result<(), anyhow::Error> {
    // `config_file_dir` expands to: /etc/tedge/c8y or `config-dir`/c8y
    let config_file_dir = config_dir.join(DEFAULT_OPERATION_DIR_NAME);
    let mut plugin_config =
        PluginConfig::new(&config_file_dir.join(DEFAULT_PLUGIN_CONFIG_FILE_NAME));
    let mut mqtt_client = create_mqtt_client(mqtt_port).await?;

    // Publish supported configuration types
    publish_supported_config_types(&mut mqtt_client, &plugin_config).await?;

    // Get pending operations
    let msg = Message::new(&C8yTopic::SmartRestResponse.to_topic()?, "500");
    mqtt_client.published.send(msg).await?;

    // we watch `config_file_path` for any change to a file named `DEFAULT_PLUGIN_CONFIG_FILE_NAME`
    let mut fs_notification_stream = fs_notify_stream(&[(
        &config_file_dir,
        Some(DEFAULT_PLUGIN_CONFIG_FILE_NAME.to_string()),
        &[
            FsEvent::Modified,
            FsEvent::FileDeleted,
            FsEvent::FileCreated,
        ],
    )])?;

    loop {
        tokio::select! {
            message = mqtt_client.received.next() => {
            if let Some(message) = message {
                let topic = message.topic.name.clone();
                if let Err(err) = process_mqtt_message(
                    message,
                    &mut mqtt_client,
                    http_client,
                    local_http_host,
                    tmp_dir.clone(),
                    tedge_device_id.as_str(),
                    config_dir
                )
                .await {
                    error!("Processing the message received on {topic} failed with {err}");
                }
            } else {
                // message is None and the connection has been closed
                return Ok(())
            }
        }
        Some((path, mask)) = fs_notification_stream.rx.recv() => {
            match mask {
                FsEvent::Modified | FsEvent::FileDeleted | FsEvent::FileCreated => {
                    match path.file_name() {
                        Some(file_name) => {
                            // this if check is done to avoid matching on temporary files created by editors
                            if file_name.eq(DEFAULT_PLUGIN_CONFIG_FILE_NAME) {
                                let parent_dir_name = path.parent().and_then(|dir| dir.file_name()).ok_or(PathsError::ParentDirNotFound {path: path.as_os_str().into()})?;

                                if parent_dir_name.eq("c8y") {
                                    plugin_config = PluginConfig::new(&path);
                                    let message = plugin_config.to_supported_config_types_message()?;
                                    mqtt_client.published.send(message).await?;
                                } else {
                                    // this is a child device
                                    plugin_config = PluginConfig::new(&path);
                                    let message = plugin_config.to_supported_config_types_message_for_child(&parent_dir_name.to_string_lossy())?;
                                    mqtt_client.published.send(message).await?;

                                }
                            }

                        },
                        None => {}
                    }
                },
                _ => {
                    // ignore other FsEvent(s)
                }
            }
        }}
    }
}

async fn process_mqtt_message(
    message: Message,
    mqtt_client: &mut Connection,
    http_client: &mut impl C8YHttpProxy,
    local_http_host: &str,
    tmp_dir: PathBuf,
    tedge_device_id: &str,
    config_dir: &Path,
) -> Result<(), anyhow::Error> {
    let health_check_topics = health_check_topics("c8y-configuration-plugin");
    let config_snapshot_response: TopicFilter =
        ConfigOperationResponseTopic::SnapshotResponse.into();
    let config_update_response: TopicFilter = ConfigOperationResponseTopic::UpdateResponse.into();
    let c8y_request_topic: TopicFilter = C8yTopic::SmartRestRequest.into();

    if health_check_topics.accept(&message) {
        send_health_status(&mut mqtt_client.published, "c8y-configuration-plugin").await;
        return Ok(());
    } else if config_snapshot_response.accept(&message) {
        info!("config snapshot response");
        handle_child_device_config_operation_response(
            &message,
            mqtt_client,
            http_client,
            config_dir,
        )
        .await?;
    } else if config_update_response.accept(&message) {
        info!("config update response");
        handle_child_device_config_operation_response(
            &message,
            mqtt_client,
            http_client,
            config_dir,
        )
        .await?;
    } else if c8y_request_topic.accept(&message) {
        let payload = message.payload_str()?;
        for smartrest_message in payload.split('\n') {
            let result = match smartrest_message.split(',').next().unwrap_or_default() {
                "524" => {
                    let maybe_config_download_request =
                        SmartRestConfigDownloadRequest::from_smartrest(smartrest_message);
                    if let Ok(config_download_request) = maybe_config_download_request {
                        handle_config_download_request(
                            config_download_request,
                            tmp_dir.clone(),
                            mqtt_client,
                            http_client,
                            local_http_host,
                            tedge_device_id,
                            config_dir,
                        )
                        .await
                    } else {
                        error!(
                            "Incorrect Download SmartREST payload: {}",
                            smartrest_message
                        );
                        Ok(())
                    }
                }
                "526" => {
                    // retrieve config file upload smartrest request from payload
                    let maybe_config_upload_request =
                        SmartRestConfigUploadRequest::from_smartrest(smartrest_message);

                    if let Ok(config_upload_request) = maybe_config_upload_request {
                        // handle the config file upload request
                        handle_config_upload_request(
                            config_upload_request,
                            mqtt_client,
                            http_client,
                            local_http_host,
                            tedge_device_id,
                            config_dir,
                        )
                        .await
                    } else {
                        error!("Incorrect Upload SmartREST payload: {}", smartrest_message);
                        Ok(())
                    }
                }
                _ => {
                    // Ignore operation messages not meant for this plugin
                    Ok(())
                }
            };

            if let Err(err) = result {
                error!("Handling of operation: '{smartrest_message}' failed with {err}");
            }
        }
    } else {
        error!(
            "Received unexpected message on topic: {}",
            message.topic.name
        );
    }
    Ok(())
}

pub async fn handle_child_device_config_operation_response(
    message: &Message,
    mqtt_client: &mut Connection,
    http_client: &mut impl C8YHttpProxy,
    config_dir: &Path,
) -> Result<(), anyhow::Error> {
    match ConfigOperationResponse::try_from(message) {
        Ok(config_response) => {
            let smartrest_response = match &config_response {
                ConfigOperationResponse::Update { .. } => {
                    handle_child_device_config_update_response(&config_response)?
                }
                ConfigOperationResponse::Snapshot { .. } => {
                    handle_child_device_config_snapshot_response(
                        &config_response,
                        http_client,
                        config_dir,
                    )
                    .await?
                }
            };

            mqtt_client.published.send(smartrest_response).await?;
            Ok(())
        }
        Err(err) => {
            fail_pending_config_operation_in_c8y(message, err.to_string(), mqtt_client).await
        }
    }
}

pub async fn fail_pending_config_operation_in_c8y(
    message: &Message,
    failure_reason: String,
    mqtt_client: &mut Connection,
) -> Result<(), anyhow::Error> {
    // Fail the operation in the cloud by sending EXECUTING and FAILED responses back to back
    let config_operation = message.try_into()?;
    let child_id = get_child_id_from_child_topic(&message.topic.name)?;

    let c8y_child_topic =
        Topic::new_unchecked(&C8yTopic::ChildSmartRestResponse(child_id).to_string());

    let (executing_msg, failed_msg) = match config_operation {
        ConfigOperation::Snapshot => {
            let executing_msg = Message::new(
                &c8y_child_topic,
                UploadConfigFileStatusMessage::status_executing()?,
            );
            let failed_msg = Message::new(
                &c8y_child_topic,
                UploadConfigFileStatusMessage::status_failed(failure_reason)?,
            );
            (executing_msg, failed_msg)
        }
        ConfigOperation::Update => {
            let executing_msg = Message::new(
                &c8y_child_topic,
                DownloadConfigFileStatusMessage::status_executing()?,
            );
            let failed_msg = Message::new(
                &c8y_child_topic,
                DownloadConfigFileStatusMessage::status_failed(failure_reason)?,
            );
            (executing_msg, failed_msg)
        }
    };
    mqtt_client.published.send(executing_msg).await?;
    mqtt_client.published.send(failed_msg).await?;

    Ok(())
}

fn init(cfg_dir: PathBuf) -> Result<(), anyhow::Error> {
    info!("Creating supported operation files");
    create_operation_files(&cfg_dir)?;
    Ok(())
}

async fn publish_supported_config_types(
    mqtt_client: &mut Connection,
    plugin_config: &PluginConfig,
) -> Result<(), MqttError> {
    let message = plugin_config.to_supported_config_types_message()?;
    mqtt_client.published.send(message).await?;
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
