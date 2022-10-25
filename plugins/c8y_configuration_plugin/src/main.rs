mod child_device;
mod config;
mod download;
mod error;
mod topic;
mod upload;

use crate::upload::handle_config_upload_request;
use crate::{
    child_device::ConfigOperationResponse,
    download::{handle_child_device_config_update_response, handle_config_download_request},
};
use crate::{config::PluginConfig, upload::handle_child_device_config_snapshot_response};

use anyhow::Result;
use c8y_api::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use c8y_api::smartrest::smartrest_deserializer::{
    SmartRestConfigDownloadRequest, SmartRestConfigUploadRequest, SmartRestRequestGeneric,
};
use c8y_api::smartrest::topic::C8yTopic;
use clap::Parser;
use mqtt_channel::{Connection, Message, PubChannel, SinkExt, StreamExt, TopicFilter};

use std::path::{Path, PathBuf};
use tedge_config::{
    ConfigRepository, ConfigSettingAccessor, DeviceIdSetting, IpAddress, MqttBindAddressSetting,
    MqttExternalBindAddressSetting, MqttPortSetting, TEdgeConfig, TmpPathSetting,
    DEFAULT_TEDGE_CONFIG_PATH,
};
use tedge_utils::{
    file::{create_directory_with_user_group, create_file_with_user_group},
    notify::fs_notify_stream,
    paths::PathsError,
};
use thin_edge_json::health::{health_check_topics, send_health_status};
use topic::ConfigOperationResponseTopic;

use tedge_utils::notify::FileEvent;
use tracing::{debug, error, info};

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

    //TODO: Port number to be read from HttpPortSetting
    let local_http_host = format!("{}:8000", bind_address.to_string().as_str());

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
    // `config_file_path` expands to: /etc/tedge/c8y or `config-dir`/c8y
    let config_file_path = config_dir.join(DEFAULT_OPERATION_DIR_NAME);
    let mut plugin_config =
        PluginConfig::new(&config_file_path.join(DEFAULT_PLUGIN_CONFIG_FILE_NAME));
    let mut mqtt_client = create_mqtt_client(mqtt_port).await?;

    // Publish supported configuration types
    let msg = plugin_config.to_supported_config_types_message()?;
    debug!("Plugin init message: {:?}", msg);
    mqtt_client.published.send(msg).await?;

    // Get pending operations
    let msg = Message::new(&C8yTopic::SmartRestResponse.to_topic()?, "500");
    mqtt_client.published.send(msg).await?;

    // we watch `config_file_path` for any change to a file named `DEFAULT_PLUGIN_CONFIG_FILE_NAME`
    let mut fs_notification_stream = fs_notify_stream(&[(
        &config_file_path,
        Some(DEFAULT_PLUGIN_CONFIG_FILE_NAME.to_string()),
        &[FileEvent::Modified, FileEvent::Deleted, FileEvent::Created],
    )])?;

    loop {
        tokio::select! {
            message = mqtt_client.received.next() => {
            if let Some(message) = message {
                process_mqtt_message(
                    message,
                    &mut mqtt_client,
                    http_client,
                    local_http_host,
                    tmp_dir.clone(),
                    tedge_device_id.as_str(),
                    config_dir
                )
                .await?;
            } else {
                // message is None and the connection has been closed
                return Ok(())
            }
        }
        Some((path, mask)) = fs_notification_stream.rx.recv() => {
            match mask {
                FileEvent::Modified | FileEvent::Deleted | FileEvent::Created => {
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

    if health_check_topics.accept(&message) {
        send_health_status(&mut mqtt_client.published, "c8y-configuration-plugin").await;
        return Ok(());
    }
    if config_snapshot_response.accept(&message) {
        info!("config snapshot response");
        let outgoing_message = handle_child_device_config_snapshot_response(
            &message,
            &tmp_dir,
            http_client,
            local_http_host,
            config_dir,
        )
        .await?;
        mqtt_client.published.publish(outgoing_message).await?;
        return Ok(());
    }
    if config_update_response.accept(&message) {
        info!("config update response");
        let child_config_management = ConfigOperationResponse::try_from(&message)?;
        let outgoing_message =
            handle_child_device_config_update_response(&message, &child_config_management)?;
        mqtt_client.published.publish(outgoing_message).await?;
        return Ok(());
    } else if let Ok(payload) = message.payload_str() {
        for smartrest_message in payload.split('\n') {
            let result = match message.topic.name.as_str() {
                "tedge/configuration_change/c8y-configuration-plugin" => {
                    // Reload the plugin config file
                    let config_file_path = config_dir.join(DEFAULT_PLUGIN_CONFIG_FILE_NAME);
                    let plugin_config = PluginConfig::new(&config_file_path);
                    // Resend the supported config types
                    let msg = plugin_config.to_supported_config_types_message()?;
                    mqtt_client.published.send(msg).await?;
                    Ok(())
                }
                _ => {
                    match smartrest_message.split(',').next().unwrap_or_default() {
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
                    }
                }
            };

            if let Err(err) = result {
                error!("Handling of operation: '{smartrest_message}' failed with {err}");
            }
        }
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use crate::child_device::{ChildDeviceRequestPayload, ChildDeviceResponsePayload};

    use super::*;
    use agent_interface::OperationStatus;
    use c8y_api::http_proxy::MockC8YHttpProxy;
    use mockall::predicate;
    use std::time::Duration;
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[serial_test::serial]
    async fn test_handle_config_upload_request_tedge_device() -> anyhow::Result<()> {
        let tedge_device_id = "tedge-device";
        let test_config_type = "c8y-configuration-plugin";
        let ttd = TempTedgeDir::new();
        ttd.dir("c8y").file("c8y-configuration-plugin.toml");

        let broker = mqtt_tests::test_mqtt_broker();

        let mut messages = broker.messages_published_on("c8y/s/us").await;

        let mut http_client = MockC8YHttpProxy::new();
        http_client
            .expect_upload_config_file()
            .with(
                predicate::always(),
                predicate::eq(test_config_type),
                predicate::eq(None),
            )
            .return_once(|_path, _type, _child_id| {
                Ok("http://server/some/test/config/url".to_string())
            });

        // Run the plugin's runtime logic in an async task
        tokio::spawn(async move {
            let _ = run(
                tedge_device_id.into(),
                broker.port,
                &mut http_client,
                "localhost",
                ttd.path().to_path_buf(),
                ttd.path(),
            )
            .await;
        });

        // Assert supported config types message(119) on plugin startup
        mqtt_tests::assert_received_all_expected(
            &mut messages,
            TEST_TIMEOUT_MS,
            &[format!("119,{test_config_type}")],
        )
        .await;

        // Send a config upload request to the plugin
        broker
            .publish(
                "c8y/s/ds",
                format!("526,{tedge_device_id},{test_config_type}").as_str(),
            )
            .await?;

        // Assert the c8y_UploadConfigFile operation transitioning from EXECUTING(501) to SUCCESSFUL(503) with the uploaded config URL
        mqtt_tests::assert_received_all_expected(
            &mut messages,
            TEST_TIMEOUT_MS,
            &[
                "501,c8y_UploadConfigFile",
                "503,c8y_UploadConfigFile,http://server/some/test/config/url",
            ],
        )
        .await;

        Ok(())
    }

    // Test c8y_UploadConfigFile SmartREST request mapping to tedge config_snapshot command
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[serial_test::serial]
    async fn test_handle_config_upload_request_child_device() -> anyhow::Result<()> {
        let tedge_device_id = "tedge-device";
        let child_device_id = "child-aa";
        let config_type = "file_a";
        let test_config_path = "/some/test/config";

        let tmp_dir = TempTedgeDir::new();
        tmp_dir
            .dir("c8y")
            .dir(child_device_id)
            .file("c8y-configuration-plugin.toml")
            .with_toml_content(toml::toml! {
                files = [
                    { path = test_config_path, type = "file_a" }
                ]
            });

        let server_address = mockito::server_address().to_string();

        let request = ChildDeviceRequestPayload {
            url: format!(
                "http://{server_address}/tedge/file-transfer/{child_device_id}/config_snapshot/file_a"
            ),
            path: test_config_path.into(),
            config_type: Some(config_type.into()),
        };
        let expected_request = serde_json::to_string(&request)?;

        let broker = mqtt_tests::test_mqtt_broker();
        let mut c8y_http_client = MockC8YHttpProxy::new();

        // Run the plugin's runtime logic in an async task
        tokio::spawn(async move {
            let _ = run(
                tedge_device_id.into(),
                broker.port,
                &mut c8y_http_client,
                &server_address,
                tmp_dir.path().to_path_buf(),
                tmp_dir.path(),
            )
            .await;
        });

        let mut tedge_command_messages = broker
            .messages_published_on(&format!(
                "tedge/{child_device_id}/commands/req/config_snapshot"
            ))
            .await;

        // Send a c8y_UploadConfigFile request to the plugin
        broker
            .publish(
                "c8y/s/ds",
                format!("526,{child_device_id},{config_type}").as_str(),
            )
            .await?;

        // Assert the mapping from c8y_UploadConfigFile request to tedge command
        mqtt_tests::assert_received_all_expected(
            &mut tedge_command_messages,
            TEST_TIMEOUT_MS,
            &[expected_request],
        )
        .await;

        Ok(())
    }

    // Test tedge config_snapshot command executing response mapping to SmartREST
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[serial_test::serial]
    async fn test_handle_config_upload_executing_response_child_device() -> anyhow::Result<()> {
        let tedge_device_id = "tedge-device";
        let child_device_id = "child-device";
        let config_type = "config_type";
        let test_config_path = "/some/test/config";

        let ttd = TempTedgeDir::new();
        ttd.dir("c8y")
            .dir(child_device_id)
            .file("c8y-configuration-plugin.toml")
            .with_toml_content(toml::toml! {
                files = [
                    { path = test_config_path, type = config_type }
                ]
            });

        let broker = mqtt_tests::test_mqtt_broker();
        let mut c8y_http_client = MockC8YHttpProxy::new();

        // Run the plugin's runtime logic in an async task
        tokio::spawn(async move {
            let _ = run(
                tedge_device_id.into(),
                broker.port,
                &mut c8y_http_client,
                &mockito::server_url(),
                ttd.path().to_path_buf(),
                ttd.path(),
            )
            .await;
        });

        let mut smartrest_messages = broker
            .messages_published_on(format!("c8y/s/us/{child_device_id}").as_str())
            .await;

        // Fake config_snapshot executing status response from child device
        //
        broker
            .publish(
                &format!("tedge/{child_device_id}/commands/res/config_snapshot"),
                &serde_json::to_string(&ChildDeviceResponsePayload {
                    status: Some(OperationStatus::Executing),
                    path: test_config_path.into(),
                    config_type: config_type.into(),
                    reason: None,
                })
                .unwrap(),
            )
            .await?;

        // Assert the c8y_UploadConfigFile operation status mapping to EXECUTING(501)
        mqtt_tests::assert_received_all_expected(
            &mut smartrest_messages,
            TEST_TIMEOUT_MS,
            &["501,c8y_UploadConfigFile"],
        )
        .await;

        Ok(())
    }

    // Test tedge config_snapshot command failed response mapping to SmartREST
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[serial_test::serial]
    async fn test_handle_config_upload_failed_response_child_device() -> anyhow::Result<()> {
        let tedge_device_id = "tedge-device";
        let child_device_id = "child-device";
        let config_type = "config_type";
        let test_config_path = "/some/test/config";

        let ttd = TempTedgeDir::new();
        ttd.dir("c8y")
            .dir(child_device_id)
            .file("c8y-configuration-plugin.toml")
            .with_toml_content(toml::toml! {
                files = [
                    { path = test_config_path, type = config_type }
                ]
            });

        let broker = mqtt_tests::test_mqtt_broker();
        let mut c8y_http_client = MockC8YHttpProxy::new();

        // Run the plugin's runtime logic in an async task
        tokio::spawn(async move {
            let _ = run(
                tedge_device_id.into(),
                broker.port,
                &mut c8y_http_client,
                &mockito::server_url(),
                ttd.path().to_path_buf(),
                ttd.path(),
            )
            .await;
        });

        let mut smartrest_messages = broker
            .messages_published_on(format!("c8y/s/us/{child_device_id}").as_str())
            .await;

        // Fake config_snapshot executing status response from child device
        broker
            .publish(
                &format!("tedge/{child_device_id}/commands/res/config_snapshot"),
                &serde_json::to_string(&ChildDeviceResponsePayload {
                    status: Some(OperationStatus::Failed),
                    path: test_config_path.into(),
                    config_type: config_type.into(),
                    reason: Some("upload failed".into()),
                })
                .unwrap(),
            )
            .await?;

        // Assert the c8y_UploadConfigFile operation status mapping to FAILED(502)
        mqtt_tests::assert_received_all_expected(
            &mut smartrest_messages,
            TEST_TIMEOUT_MS,
            &[r#"502,c8y_UploadConfigFile,"upload failed""#],
        )
        .await;

        Ok(())
    }

    // Test tedge config_snapshot command successful response mapping to SmartREST
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[serial_test::serial]
    async fn test_handle_config_upload_successful_response_child_device() -> anyhow::Result<()> {
        let tedge_device_id = "tedge-device";
        let child_device_id = "child-device";
        let config_type = "config_type";
        let test_config_path = "/some/test/config";

        let ttd = TempTedgeDir::new();
        ttd.dir("c8y")
            .dir(child_device_id)
            .file("c8y-configuration-plugin.toml")
            .with_toml_content(toml::toml! {
                files = [
                    { path = test_config_path, type = config_type }
                ]
            });

        let broker = mqtt_tests::test_mqtt_broker();

        let local_http_host = mockito::server_address().to_string();

        //Mock the config file upload to Cumulocity
        let mut c8y_http_client = MockC8YHttpProxy::new();
        c8y_http_client
            .expect_upload_config_file()
            .with(
                predicate::always(),
                predicate::eq(config_type),
                predicate::eq(Some(child_device_id.to_string())),
            )
            .return_once(|_path, _type, _child_id| Ok("http://server/config/file/url".to_string()));

        // Run the plugin's runtime logic in an async task
        tokio::spawn(async move {
            let _ = run(
                tedge_device_id.into(),
                broker.port,
                &mut c8y_http_client,
                &local_http_host,
                ttd.path().to_path_buf(),
                ttd.path(),
            )
            .await;
        });

        let mut smartrest_messages = broker
            .messages_published_on(format!("c8y/s/us/{child_device_id}").as_str())
            .await;

        // Mock the config file url, to be downloaded by this plugin, from the file transfer service as if child device uploaded the file
        let config_url_path =
            format!("/tedge/file-transfer/{child_device_id}/config_snapshot/{config_type}");
        let _config_snapshot_url_mock = mockito::mock("GET", config_url_path.as_str())
            .with_body("v1")
            .with_status(200)
            .create();

        // Fake child device sending config_snapshot successful status TODO
        broker
            .publish(
                &format!("tedge/{child_device_id}/commands/res/config_snapshot"),
                &serde_json::to_string(&ChildDeviceResponsePayload {
                    status: Some(OperationStatus::Successful),
                    path: test_config_path.into(),
                    config_type: config_type.into(),
                    reason: None,
                })
                .unwrap(),
            )
            .await?;

        // Assert the c8y_UploadConfigFile operation status mapping to SUCCESSFUL(503)
        mqtt_tests::assert_received_all_expected(
            &mut smartrest_messages,
            TEST_TIMEOUT_MS,
            &["503,c8y_UploadConfigFile,http://server/config/file/url"],
        )
        .await;

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[serial_test::serial]
    async fn test_handle_config_update_request_child_device() -> anyhow::Result<()> {
        let tedge_device_id = "tedge-device";
        let child_device_id = "child-device";
        let config_type = "file_a";
        let test_config_path = "/some/test/config";
        let tmp_dir = TempTedgeDir::new();
        tmp_dir
            .dir("c8y")
            .dir(child_device_id)
            .file("c8y-configuration-plugin.toml")
            .with_toml_content(toml::toml! {
                files = [
                    { path = test_config_path, type = "file_a" }
                ]
            });

        let broker = mqtt_tests::test_mqtt_broker();
        let local_http_host = mockito::server_address().to_string();
        let mut c8y_http_client = MockC8YHttpProxy::new();
        c8y_http_client
            .expect_url_is_in_my_tenant_domain()
            .with(predicate::always())
            .return_once(|_path| false);

        // Run the plugin's runtime logic in an async task
        tokio::spawn(async move {
            let _ = run(
                tedge_device_id.into(),
                broker.port,
                &mut c8y_http_client,
                local_http_host.as_str(),
                tmp_dir.path().to_path_buf(),
                tmp_dir.path(),
            )
            .await;
        });

        let mut tedge_command_messages = broker
            .messages_published_on(&format!(
                "tedge/{child_device_id}/commands/req/config_update"
            ))
            .await;

        // Mock download endpoint for the plugin to download config file update from the cloud
        let config_update_download_url_path = "/tede/file-transfer/config_update/file_a";
        let _download_config_url_mock = mockito::mock("GET", config_update_download_url_path)
            .with_body_from_fn(|w| w.write_all(b"v2"))
            .with_status(200)
            .create();
        let local_http_host = mockito::server_url();
        let config_update_download_url =
            format!("{local_http_host}{config_update_download_url_path}");
        dbg!(&config_update_download_url);
        dbg!(&config_type);

        // Mock upload endpoint for the plugin to upload the config file update to the file transfer service
        let config_update_upload_url_path =
            format!("/tedge/file-transfer/{child_device_id}/config_update/{config_type}");
        //let config_update_upload_url_path =
        //    format!("/tedge/file-transfer/{config_update_download_url_path}");
        dbg!(&config_update_upload_url_path);
        let _upload_config_url_mock = mockito::mock("PUT", config_update_upload_url_path.as_str())
            .with_status(201)
            .create();

        // Send a c8y_DownloadConfigFile request to the plugin
        broker
            .publish(
                "c8y/s/ds",
                format!("524,{child_device_id},{config_update_download_url},{config_type}")
                    .as_str(),
            )
            .await?;

        let request = ChildDeviceRequestPayload {
            url: format!(
                "{local_http_host}/tedge/file-transfer/{child_device_id}/config_update/{config_type}"
            ),
            path: test_config_path.into(),
            config_type: Some(config_type.into()),
        };
        let expected_request = serde_json::to_string(&request)?;

        // Assert the mapping from c8y_DownloadConfigFile request to tedge command
        mqtt_tests::assert_received_all_expected(
            &mut tedge_command_messages,
            TEST_TIMEOUT_MS,
            &[expected_request],
        )
        .await;

        Ok(())
    }

    // Test tedge config_update command successful response mapping to SmartREST
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[serial_test::serial]
    async fn test_handle_config_update_successful_response_child_device() -> anyhow::Result<()> {
        let tedge_device_id = "tedge-device";
        let child_device_id = "child-device";
        let config_type = "config_type";
        let test_config_path = "/some/test/config";

        let ttd = TempTedgeDir::new();
        ttd.dir("c8y")
            .file("c8y-configuration-plugin.toml")
            .with_toml_content(toml::toml! {
                files = [
                    { path = test_config_path, type = config_type }
                ]
            });

        let broker = mqtt_tests::test_mqtt_broker();
        let local_http_host = mockito::server_address().to_string();
        let mut c8y_http_client = MockC8YHttpProxy::new();

        // Run the plugin's runtime logic in an async task
        tokio::spawn(async move {
            let _ = run(
                tedge_device_id.into(),
                broker.port,
                &mut c8y_http_client,
                &local_http_host,
                ttd.path().to_path_buf(),
                ttd.path(),
            )
            .await;
        });

        // Mock the config file url, to be deleted by this plugin from the file transfer service
        let config_url_path = format!("/tedge/{child_device_id}/config_update/{config_type}");
        let _config_snapshot_url_mock = mockito::mock("DELETE", config_url_path.as_str())
            .with_status(200)
            .create();

        let mut smartrest_messages = broker
            .messages_published_on(format!("c8y/s/us/{child_device_id}").as_str())
            .await;

        // Fake child device sending config_update successful status
        broker
            .publish(
                &format!("tedge/{child_device_id}/commands/res/config_update"),
                &serde_json::to_string(&ChildDeviceResponsePayload {
                    status: Some(OperationStatus::Successful),
                    path: test_config_path.into(),
                    config_type: config_type.into(),
                    reason: None,
                })
                .unwrap(),
            )
            .await?;

        // Assert the c8y_DownloadConfigFile operation status mapping to SUCCESSFUL(503)
        mqtt_tests::assert_received_all_expected(
            &mut smartrest_messages,
            TEST_TIMEOUT_MS,
            &["503,c8y_DownloadConfigFile"],
        )
        .await;

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[serial_test::serial]
    async fn test_handle_multiline_config_upload_requests() -> anyhow::Result<()> {
        let tedge_device_id = "tedge-device";
        let test_config_type = "c8y-configuration-plugin";
        let ttd = TempTedgeDir::new();
        ttd.dir("c8y").file("c8y-configuration-plugin.toml");

        let broker = mqtt_tests::test_mqtt_broker();

        let mut messages = broker.messages_published_on("c8y/s/us").await;

        let mut http_client = MockC8YHttpProxy::new();
        http_client
            .expect_upload_config_file()
            .with(
                predicate::always(),
                predicate::eq(test_config_type),
                predicate::eq(None),
            )
            .returning(|_path, _type, _child_id| {
                Ok("http://server/some/test/config/url".to_string())
            });

        // Run the plugin's runtime logic in an async task
        tokio::spawn(async move {
            let _ = run(
                tedge_device_id.into(),
                broker.port,
                &mut http_client,
                "localhost",
                ttd.path().to_path_buf(),
                ttd.path(),
            )
            .await;
        });

        // Assert supported config types message(119) on plugin startup
        mqtt_tests::assert_received_all_expected(
            &mut messages,
            TEST_TIMEOUT_MS,
            &[format!("119,{test_config_type}")],
        )
        .await;

        // Send a config upload request to the plugin
        broker
            .publish(
                "c8y/s/ds",
                format!("526,{tedge_device_id},{test_config_type}\n526,{tedge_device_id},{test_config_type}").as_str(),
            )
            .await?;

        // Assert the c8y_UploadConfigFile operation transitioning from EXECUTING(501) to SUCCESSFUL(503) with the uploaded config URL
        mqtt_tests::assert_received_all_expected(
            &mut messages,
            TEST_TIMEOUT_MS,
            &[
                "501,c8y_UploadConfigFile",
                "503,c8y_UploadConfigFile,http://server/some/test/config/url",
                "501,c8y_UploadConfigFile",
                "503,c8y_UploadConfigFile,http://server/some/test/config/url",
            ],
        )
        .await;

        Ok(())
    }
}
