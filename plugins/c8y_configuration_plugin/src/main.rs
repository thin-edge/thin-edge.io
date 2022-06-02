mod config;
mod download;
mod error;
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
use mqtt_channel::{Message, SinkExt, StreamExt, Topic};
use std::fs;
use std::path::{Path, PathBuf};
use tedge_config::{
    ConfigRepository, ConfigSettingAccessor, MqttPortSetting, TEdgeConfig, TmpPathSetting,
    DEFAULT_TEDGE_CONFIG_PATH,
};
use tedge_utils::file::{create_directory_with_user_group, create_file_with_user_group};
use tracing::{debug, error, info};

pub const DEFAULT_PLUGIN_CONFIG_FILE_PATH: &str = "/etc/tedge/c8y/c8y-configuration-plugin.toml";
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

    #[clap(long = "config-file", default_value = DEFAULT_PLUGIN_CONFIG_FILE_PATH)]
    pub config_file: PathBuf,
}

async fn create_mqtt_client(mqtt_port: u16) -> Result<mqtt_channel::Connection, anyhow::Error> {
    let mut topic_filter =
        mqtt_channel::TopicFilter::new_unchecked(C8yTopic::SmartRestRequest.as_str());
    let _ = topic_filter
        .add_unchecked(format!("{CONFIG_CHANGE_TOPIC}/{DEFAULT_PLUGIN_CONFIG_TYPE}").as_str());

    let mqtt_config = mqtt_channel::Config::default()
        .with_port(mqtt_port)
        .with_subscriptions(topic_filter);

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

    let mqtt_port = tedge_config.query(MqttPortSetting)?.into();
    let mut http_client = create_http_client(&tedge_config).await?;
    let tmp_dir = tedge_config.query(TmpPathSetting)?.into();

    run(
        mqtt_port,
        &mut http_client,
        tmp_dir,
        &config_plugin_opt.config_file,
    )
    .await
}

async fn run(
    mqtt_port: u16,
    http_client: &mut impl C8YHttpProxy,
    tmp_dir: PathBuf,
    config_file_path: &Path,
) -> Result<(), anyhow::Error> {
    let mut plugin_config = PluginConfig::new(config_file_path);

    let mut mqtt_client = create_mqtt_client(mqtt_port).await?;

    // Publish supported configuration types
    let msg = plugin_config.to_supported_config_types_message()?;
    debug!("Plugin init message: {:?}", msg);
    let () = mqtt_client.published.send(msg).await?;

    // Get pending operations
    let msg = Message::new(
        &Topic::new_unchecked(C8yTopic::SmartRestResponse.as_str()),
        "500",
    );
    let () = mqtt_client.published.send(msg).await?;

    // Mqtt message loop
    while let Some(message) = mqtt_client.received.next().await {
        debug!("Received {:?}", message);
        if let Ok(payload) = message.payload_str() {
            let result = if let "tedge/configuration_change/c8y-configuration-plugin" =
                message.topic.name.as_str()
            {
                // Reload the plugin config file
                plugin_config = PluginConfig::new(config_file_path);
                // Resend the supported config types
                let msg = plugin_config.to_supported_config_types_message()?;
                mqtt_client.published.send(msg).await?;
                Ok(())
            } else {
                match payload.split(',').next().unwrap_or_default() {
                    "524" => {
                        let config_download_request =
                            SmartRestConfigDownloadRequest::from_smartrest(payload)?;

                        handle_config_download_request(
                            &plugin_config,
                            config_download_request,
                            tmp_dir.clone(),
                            &mut mqtt_client,
                            http_client,
                        )
                        .await
                    }
                    "526" => {
                        // retrieve config file upload smartrest request from payload
                        let config_upload_request =
                            SmartRestConfigUploadRequest::from_smartrest(payload)?;

                        // handle the config file upload request
                        handle_config_upload_request(
                            &plugin_config,
                            config_upload_request,
                            &mut mqtt_client,
                            http_client,
                        )
                        .await
                    }
                    _ => {
                        // Ignore operation messages not meant for this plugin
                        Ok(())
                    }
                }
            };

            if let Err(err) = result {
                error!("Handling of operation: '{payload}' failed with {err}");
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
    create_directory_with_user_group(&format!("{config_dir}/c8y"), "root", "root", 0o775)?;
    create_file_with_user_group(
        &format!("{config_dir}/c8y/c8y-configuration-plugin.toml"),
        "root",
        "root",
        0o644,
    )?;
    let example_config = r#"# Add the configurations to be managed by c8y-configuration-plugin

files = [
#    { path = '/etc/tedge/tedge.toml' },
#    { path = '/etc/tedge/mosquitto-conf/c8y-bridge.conf', type = 'c8y-bridge.conf' },
#    { path = '/etc/tedge/mosquitto-conf/tedge-mosquitto.conf', type = 'tedge-mosquitto.conf' },
#    { path = '/etc/mosquitto/mosquitto.conf', type = 'mosquitto.conf' },
#    { path = '/etc/tedge/c8y/example.txt', type = 'example', user = 'tedge', group = 'tedge', mode = 0o444 }
]"#;
    fs::write(
        &format!("{config_dir}/c8y/c8y-configuration-plugin.toml"),
        example_config,
    )?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use c8y_api::http_proxy::MockC8YHttpProxy;
    use mockall::predicate;
    use std::{path::Path, time::Duration};
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[serial_test::serial]
    async fn test_message_dispatch() -> anyhow::Result<()> {
        let test_config_path = "/some/test/config";
        let test_config_type = "c8y-configuration-plugin";

        let broker = mqtt_tests::test_mqtt_broker();

        let mut messages = broker.messages_published_on("c8y/s/us").await;

        let mut http_client = MockC8YHttpProxy::new();
        http_client
            .expect_upload_config_file()
            .with(
                predicate::eq(Path::new(test_config_path)),
                predicate::eq(test_config_type),
            )
            .return_once(|_path, _type| Ok("http://server/some/test/config/url".to_string()));

        let tmp_dir = TempTedgeDir::new();

        // Run the plugin's runtime logic in an async task
        tokio::spawn(async move {
            let _ = run(
                broker.port,
                &mut http_client,
                tmp_dir.path().to_path_buf(),
                PathBuf::from(test_config_path).as_path(),
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

        // Send a software upload request to the plugin
        let _ = broker
            .publish(
                "c8y/s/ds",
                format!("526,tedge-device,{test_config_type}").as_str(),
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
}
