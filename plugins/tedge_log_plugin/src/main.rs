use clap::Parser;
use std::path::Path;
use std::path::PathBuf;
use tedge_actors::Runtime;
use tedge_config::system_services::get_log_level;
use tedge_config::system_services::set_log_level;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigLocation;
use tedge_config::TEdgeConfigRepository;
use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_http_ext::HttpActor;
use tedge_log_manager::LogManagerBuilder;
use tedge_log_manager::LogManagerConfig;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_signal_ext::SignalActor;
use tracing::info;

const AFTER_HELP_TEXT: &str = r#"The thin-edge `CONFIG_DIR` is used:
* to find the `tedge.toml` where the following configs are defined:
   ** `mqtt.bind.address` and `mqtt.bind.port`: to connect to the tedge MQTT broker
   ** `root.topic` and `device.topic`: for the MQTT topics to publish to and subscribe from
* to find/store the `tedge-log-plugin.toml`: the configuration file that specifies which logs to be retrieved"#;

const TEDGE_LOG_PLUGIN: &str = "tedge-log-plugin";

#[derive(Debug, Parser, Clone)]
#[clap(
name = clap::crate_name!(),
version = clap::crate_version!(),
about = clap::crate_description!(),
after_help = AFTER_HELP_TEXT
)]
pub struct LogfilePluginOpt {
    /// Turn-on the debug log level.
    ///
    /// If off only reports ERROR, WARN, and INFO
    /// If on also reports DEBUG and TRACE
    #[clap(long)]
    pub debug: bool,

    #[clap(long = "config-dir", default_value = DEFAULT_TEDGE_CONFIG_PATH)]
    pub config_dir: PathBuf,

    #[clap(long, default_value = "te")]
    root: String,

    #[clap(long, default_value = "device/main//")]
    device: String,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let logfile_opt = LogfilePluginOpt::parse();

    // Load tedge config from the provided location
    let tedge_config_location = TEdgeConfigLocation::from_custom_root(&logfile_opt.config_dir);

    let log_level = if logfile_opt.debug {
        tracing::Level::TRACE
    } else {
        get_log_level(
            "tedge-log-plugin",
            &tedge_config_location.tedge_config_root_path,
        )?
    };
    set_log_level(log_level);

    let tedge_config = TEdgeConfigRepository::new(tedge_config_location).load()?;

    run(
        logfile_opt.config_dir,
        tedge_config,
        logfile_opt.root,
        logfile_opt.device,
    )
    .await
}

async fn run(
    config_dir: impl AsRef<Path>,
    tedge_config: TEdgeConfig,
    topic_root: String,
    topic_identifier: String,
) -> Result<(), anyhow::Error> {
    let runtime_events_logger = None;
    let mut runtime = Runtime::try_new(runtime_events_logger).await?;

    let mqtt_config = tedge_config
        .mqtt_internal_config()
        .with_session_name(TEDGE_LOG_PLUGIN);
    let mut mqtt_actor = MqttActorBuilder::new(mqtt_config);

    let mut fs_watch_actor = FsWatchActorBuilder::new();

    let health_actor = HealthMonitorBuilder::new(TEDGE_LOG_PLUGIN, &mut mqtt_actor);

    let mut http_actor = HttpActor::new().builder();

    // Instantiate log manager actor
    let log_manager_config = LogManagerConfig::from_tedge_config(
        config_dir,
        &tedge_config,
        topic_root,
        topic_identifier,
    )?;
    let log_actor = LogManagerBuilder::try_new(
        log_manager_config,
        &mut mqtt_actor,
        &mut http_actor,
        &mut fs_watch_actor,
    )?;

    // Shutdown on SIGINT
    let signal_actor = SignalActor::builder(&runtime.get_handle());

    // Run the actors
    runtime.spawn(mqtt_actor).await?;
    runtime.spawn(http_actor).await?;
    runtime.spawn(fs_watch_actor).await?;
    runtime.spawn(log_actor).await?;
    runtime.spawn(signal_actor).await?;
    runtime.spawn(health_actor).await?;

    info!("Ready to serve log requests");
    runtime.run_to_completion().await?;
    Ok(())
}
