use anyhow::Context;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tedge_actors::Runtime;
use tedge_api::mqtt_topics::DeviceTopicId;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::Service;
use tedge_config::system_services::get_log_level;
use tedge_config::system_services::set_log_level;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigLocation;
use tedge_config::TEdgeConfigRepository;
use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;
use tedge_config_manager::ConfigManagerBuilder;
use tedge_config_manager::ConfigManagerConfig;
use tedge_config_manager::ConfigManagerOptions;
use tedge_downloader_ext::DownloaderActor;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_signal_ext::SignalActor;
use tedge_uploader_ext::UploaderActor;

const AFTER_HELP_TEXT: &str = r#"On start, `tedge-configuration-plugin` notifies of its 
managed configuration files and sends this list via MQTT.
`tedge-configuration-plugin` subscribes to the topics for the commands `config_snapshot`
and `config_update`.

The thin-edge `CONFIG_DIR` is used:
* to find the `tedge.toml` where the following configs are defined:
  ** `mqtt.bind.address` and `mqtt.bind.port` to connect to the tedge MQTT broker
  ** `mqtt.topic_root` and `mqtt.device_topic_id`: for the MQTT topics to publish to and subscribe from
* to find/store the `tedge-configuration-plugin.toml`: the plugin configuration file"#;

const PLUGIN_NAME: &str = "tedge-configuration-plugin";

#[derive(Debug, Parser, Clone)]
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
    /// If on also reports DEBUG
    #[clap(long)]
    pub debug: bool,

    #[clap(long = "config-dir", default_value = DEFAULT_TEDGE_CONFIG_PATH)]
    pub config_dir: PathBuf,

    #[clap(long)]
    mqtt_topic_root: Option<Arc<str>>,

    #[clap(long)]
    mqtt_device_topic_id: Option<Arc<str>>,
}

pub async fn run(config_opt: ConfigPluginOpt) -> Result<(), anyhow::Error> {
    // Load tedge config from the provided location
    let tedge_config_location = TEdgeConfigLocation::from_custom_root(&config_opt.config_dir);

    let log_level = if config_opt.debug {
        tracing::Level::DEBUG
    } else {
        get_log_level(
            "tedge-configuration-plugin",
            &tedge_config_location.tedge_config_root_path,
        )?
    };
    set_log_level(log_level);

    let tedge_config = TEdgeConfigRepository::new(tedge_config_location).load()?;

    run_with(tedge_config, config_opt).await
}

async fn run_with(
    tedge_config: TEdgeConfig,
    cliopts: ConfigPluginOpt,
) -> Result<(), anyhow::Error> {
    let runtime_events_logger = None;
    let mut runtime = Runtime::try_new(runtime_events_logger).await?;

    let mqtt_topic_root = cliopts
        .mqtt_topic_root
        .unwrap_or(tedge_config.mqtt.topic_root.clone().into());

    let mqtt_device_topic_id = cliopts
        .mqtt_device_topic_id
        .unwrap_or(tedge_config.mqtt.device_topic_id.clone().into());

    let mqtt_config = tedge_config.mqtt_config()?;
    let mut mqtt_actor = MqttActorBuilder::new(mqtt_config.clone().with_session_name(format!(
        "{PLUGIN_NAME}#{mqtt_topic_root}/{mqtt_device_topic_id}",
    )));

    let mut fs_watch_actor = FsWatchActorBuilder::new();

    // TODO: take a user-configurable service topic id
    let mqtt_device_topic_id = mqtt_device_topic_id.parse::<EntityTopicId>().unwrap();

    let service_topic_id = mqtt_device_topic_id
        .to_default_service_topic_id(PLUGIN_NAME)
        .with_context(|| {
            format!(
                "Device topic id {mqtt_device_topic_id} currently needs default scheme, e.g: 'device/DEVICE_NAME//'",
            )
        })?;
    let service = Service {
        service_topic_id,
        device_topic_id: DeviceTopicId::new(mqtt_device_topic_id.clone()),
    };
    let mqtt_schema = MqttSchema::with_root(mqtt_topic_root.to_string());
    let health_actor = HealthMonitorBuilder::from_service_topic_id(
        service,
        &mut mqtt_actor,
        &mqtt_schema,
        tedge_config.service.ty.clone(),
    );
    let identity = tedge_config.http.client.auth.identity()?;

    let mut downloader_actor = DownloaderActor::new(identity.clone()).builder();

    let mut uploader_actor = UploaderActor::new(identity).builder();

    // Instantiate config manager actor
    let manager_config = ConfigManagerConfig::from_options(ConfigManagerOptions {
        config_dir: cliopts.config_dir,
        mqtt_topic_root: MqttSchema::with_root(mqtt_topic_root.to_string()),
        mqtt_device_topic_id: mqtt_device_topic_id.to_string().parse()?,
        tmp_path: Arc::from(tedge_config.tmp.path.as_path()),
        is_sudo_enabled: tedge_config.enable.sudo,
        config_update_enabled: true,
    })?;

    let config_actor = ConfigManagerBuilder::try_new(
        manager_config,
        &mut mqtt_actor,
        &mut fs_watch_actor,
        &mut downloader_actor,
        &mut uploader_actor,
    )
    .await?;

    // Shutdown on SIGINT
    let signal_actor = SignalActor::builder(&runtime.get_handle());

    // Run the actors
    runtime.spawn(mqtt_actor).await?;
    runtime.spawn(downloader_actor).await?;
    runtime.spawn(uploader_actor).await?;
    runtime.spawn(fs_watch_actor).await?;
    runtime.spawn(config_actor).await?;
    runtime.spawn(signal_actor).await?;
    runtime.spawn(health_actor).await?;

    runtime.run_to_completion().await?;
    Ok(())
}
