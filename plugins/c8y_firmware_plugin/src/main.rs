use c8y_firmware_manager::create_directories;
use c8y_firmware_manager::FirmwareManagerBuilder;
use c8y_firmware_manager::FirmwareManagerConfig;
use c8y_http_proxy::credentials::C8YJwtRetriever;
use clap::Parser;
use std::path::PathBuf;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::Runtime;
use tedge_config::system_services::get_log_level;
use tedge_config::system_services::set_log_level;
use tedge_config::ConfigRepository;
use tedge_config::ConfigSettingAccessor;
use tedge_config::DataPathSetting;
use tedge_config::MqttClientHostSetting;
use tedge_config::MqttClientPortSetting;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigError;
use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;
use tedge_downloader_ext::DownloaderActor;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_mqtt_ext::MqttConfig;
use tedge_signal_ext::SignalActor;
use tedge_timer_ext::TimerActor;

const PLUGIN_NAME: &str = "c8y-firmware-plugin";

const AFTER_HELP_TEXT: &str = r#"`c8y-firmware-plugin` subscribes to `c8y/s/ds` listening for firmware operation requests (message `515`).
Notifying the Cumulocity tenant of their progress (messages `501`, `502` and `503`).
During a successful operation, `c8y-firmware-plugin` updates the installed firmware info in Cumulocity tenant with SmartREST message `115`.

The thin-edge `CONFIG_DIR` is used to find where:
  * to store temporary files on download: `tedge config get tmp.path`,
  * to log operation errors and progress: `tedge config get log.path`,
  * to connect the MQTT bus: `tedge config get mqtt.port`,
  * to timeout pending operations: `tedge config get firmware.child.update.timeout"#;

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

    /// Create required directories
    #[clap(short, long)]
    pub init: bool,

    #[clap(long = "config-dir", default_value = DEFAULT_TEDGE_CONFIG_PATH)]
    pub config_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let firmware_plugin_opt = FirmwarePluginOpt::parse();

    // Load tedge config from the provided location
    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(&firmware_plugin_opt.config_dir);
    let log_level = if firmware_plugin_opt.debug {
        tracing::Level::TRACE
    } else {
        get_log_level(PLUGIN_NAME, &tedge_config_location.tedge_config_root_path)?
    };

    set_log_level(log_level);

    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
    let tedge_config = config_repository.load()?;

    if firmware_plugin_opt.init {
        init(&tedge_config)
    } else {
        run(tedge_config).await
    }
}

async fn run(tedge_config: TEdgeConfig) -> Result<(), anyhow::Error> {
    let runtime_events_logger = None;
    let mut runtime = Runtime::try_new(runtime_events_logger).await?;

    // Create actor instances
    let mqtt_config = mqtt_config(&tedge_config)?;
    let mut jwt_actor = C8YJwtRetriever::builder(mqtt_config.clone());

    let mut signal_actor = SignalActor::builder();
    let mut timer_actor = TimerActor::builder();
    let mut downloader_actor = DownloaderActor::new().builder();
    let mut mqtt_actor = MqttActorBuilder::new(mqtt_config.clone().with_session_name(PLUGIN_NAME));

    //Instantiate health monitor actor
    let health_actor = HealthMonitorBuilder::new(PLUGIN_NAME, &mut mqtt_actor);

    // Instantiate firmware manager actor
    let firmware_manager_config = FirmwareManagerConfig::from_tedge_config(&tedge_config)?;
    let firmware_actor = FirmwareManagerBuilder::new(
        firmware_manager_config,
        &mut mqtt_actor,
        &mut jwt_actor,
        &mut timer_actor,
        &mut downloader_actor,
    );

    // Shutdown on SIGINT
    signal_actor.register_peer(NoConfig, runtime.get_handle().get_sender());

    // Run the actors
    // FIXME: having to list all the actors is error prone
    runtime.spawn(signal_actor).await?;
    runtime.spawn(mqtt_actor).await?;
    runtime.spawn(jwt_actor).await?;
    runtime.spawn(downloader_actor).await?;
    runtime.spawn(firmware_actor).await?;
    runtime.spawn(timer_actor).await?;
    runtime.spawn(health_actor).await?;

    runtime.run_to_completion().await?;

    Ok(())
}

fn init(tedge_config: &TEdgeConfig) -> Result<(), anyhow::Error> {
    let data_dir: PathBuf = tedge_config.query(DataPathSetting)?.into();
    create_directories(data_dir)?;
    Ok(())
}

fn mqtt_config(tedge_config: &TEdgeConfig) -> Result<MqttConfig, TEdgeConfigError> {
    let mqtt_port = tedge_config.query(MqttClientPortSetting)?.into();
    let mqtt_host = tedge_config.query(MqttClientHostSetting)?;
    let config = MqttConfig::default()
        .with_host(mqtt_host)
        .with_port(mqtt_port);
    Ok(config)
}
