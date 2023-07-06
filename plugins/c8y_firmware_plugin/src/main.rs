use c8y_firmware_manager::FirmwareManagerBuilder;
use c8y_firmware_manager::FirmwareManagerConfig;
use c8y_http_proxy::credentials::C8YJwtRetriever;
use clap::Parser;
use std::path::PathBuf;
use tedge_actors::Runtime;
use tedge_config::new::TEdgeConfig;
use tedge_config::system_services::get_log_level;
use tedge_config::system_services::set_log_level;
use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;
use tedge_downloader_ext::DownloaderActor;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_signal_ext::SignalActor;
use tedge_timer_ext::TimerActor;
use tracing::log::warn;

const PLUGIN_NAME: &str = "c8y-firmware-plugin";

const AFTER_HELP_TEXT: &str = r#"`c8y-firmware-plugin` subscribes to `c8y/s/ds` listening for firmware operation requests (message `515`).
Notifying the Cumulocity tenant of their progress (messages `501`, `502` and `503`).
During a successful operation, `c8y-firmware-plugin` updates the installed firmware info in Cumulocity tenant with SmartREST message `115`.

The thin-edge `CONFIG_DIR` is used to find where:
  * to store temporary files on download: `tedge config get tmp.path`,
  * to log operation errors and progress: `tedge config get log.path`,
  * to connect the MQTT bus: `tedge config get mqtt.bind.port`,
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
    let tedge_config = config_repository.load_new()?;

    if firmware_plugin_opt.init {
        warn!("This --init option has been deprecated and will be removed in a future release");
        Ok(())
    } else {
        run(tedge_config).await
    }
}

async fn run(tedge_config: TEdgeConfig) -> Result<(), anyhow::Error> {
    let runtime_events_logger = None;
    let mut runtime = Runtime::try_new(runtime_events_logger).await?;

    // Create actor instances
    let mqtt_config = tedge_config.mqtt_config()?;
    let mut jwt_actor = C8YJwtRetriever::builder(mqtt_config.clone());
    let mut timer_actor = TimerActor::builder();
    let mut downloader_actor = DownloaderActor::new().builder();
    let mut mqtt_actor = MqttActorBuilder::new(mqtt_config.clone().with_session_name(PLUGIN_NAME));

    //Instantiate health monitor actor
    let health_actor = HealthMonitorBuilder::new(PLUGIN_NAME, &mut mqtt_actor);

    // Instantiate firmware manager actor
    let firmware_manager_config = FirmwareManagerConfig::from_tedge_config(&tedge_config)?;
    let firmware_actor = FirmwareManagerBuilder::try_new(
        firmware_manager_config,
        &mut mqtt_actor,
        &mut jwt_actor,
        &mut timer_actor,
        &mut downloader_actor,
    )?;

    // Shutdown on SIGINT
    let signal_actor = SignalActor::builder(&runtime.get_handle());

    // Run the actors
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
