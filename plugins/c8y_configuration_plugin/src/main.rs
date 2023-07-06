use c8y_config_manager::ConfigManagerBuilder;
use c8y_config_manager::ConfigManagerConfig;
use c8y_http_proxy::credentials::C8YJwtRetriever;
use c8y_http_proxy::C8YHttpProxyBuilder;
use clap::Parser;
use std::path::Path;
use std::path::PathBuf;
use tedge_actors::Runtime;
use tedge_config::new::TEdgeConfig;
use tedge_config::system_services::get_log_level;
use tedge_config::system_services::set_log_level;
use tedge_config::CertificateError;
use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_http_ext::HttpActor;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_mqtt_ext::MqttConfig;
use tedge_signal_ext::SignalActor;
use tedge_timer_ext::TimerActor;
use tracing::log::warn;

const PLUGIN_NAME: &str = "c8y-configuration-plugin";

const AFTER_HELP_TEXT: &str = r#"On start, `c8y-configuration-plugin` notifies the cloud tenant of the managed configuration files, listed in the `CONFIG_FILE`, sending this list with a `119` on `c8y/s/us`.
`c8y-configuration-plugin` subscribes then to `c8y/s/ds` listening for configuration operation requests (messages `524` and `526`).
notifying the Cumulocity tenant of their progress (messages `501`, `502` and `503`).

The thin-edge `CONFIG_DIR` is used to find where:
  * to store temporary files on download: `tedge config get tmp.path`,
  * to log operation errors and progress: `tedge config get log.path`,
  * to connect the MQTT bus: `tedge config get mqtt.client.port`."#;

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

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config_plugin_opt = ConfigPluginOpt::parse();
    let config_dir = config_plugin_opt.config_dir;

    // Load tedge config from the provided location
    let tedge_config_location = tedge_config::TEdgeConfigLocation::from_custom_root(&config_dir);
    let log_level = if config_plugin_opt.debug {
        tracing::Level::TRACE
    } else {
        get_log_level(PLUGIN_NAME, &tedge_config_location.tedge_config_root_path)?
    };

    set_log_level(log_level);

    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
    let tedge_config = config_repository.load_new()?;

    if config_plugin_opt.init {
        warn!("This --init option has been deprecated and will be removed in a future release");
        Ok(())
    } else {
        run(config_dir, tedge_config).await
    }
}

async fn run(config_dir: impl AsRef<Path>, tedge_config: TEdgeConfig) -> Result<(), anyhow::Error> {
    let runtime_events_logger = None;
    let mut runtime = Runtime::try_new(runtime_events_logger).await?;

    // Create actor instances
    let mqtt_config = mqtt_config(&tedge_config)?;
    let mut jwt_actor = C8YJwtRetriever::builder(mqtt_config.clone());
    let mut http_actor = HttpActor::new().builder();
    let c8y_http_config = (&tedge_config).try_into()?;
    let mut c8y_http_proxy_actor =
        C8YHttpProxyBuilder::new(c8y_http_config, &mut http_actor, &mut jwt_actor);

    let mut fs_watch_actor = FsWatchActorBuilder::new();
    let mut timer_actor = TimerActor::builder();
    let mut mqtt_actor = MqttActorBuilder::new(mqtt_config.clone().with_session_name(PLUGIN_NAME));

    // Instantiate health monitor actor
    let health_actor = HealthMonitorBuilder::new(PLUGIN_NAME, &mut mqtt_actor);

    // Instantiate config manager actor
    let config_manager_config = ConfigManagerConfig::from_tedge_config(config_dir, &tedge_config)?;
    let config_actor = ConfigManagerBuilder::try_new(
        config_manager_config,
        &mut mqtt_actor,
        &mut c8y_http_proxy_actor,
        &mut timer_actor,
        &mut fs_watch_actor,
    )?;

    // Shutdown on SIGINT
    let signal_actor = SignalActor::builder(&runtime.get_handle());

    // Run the actors
    runtime.spawn(signal_actor).await?;
    runtime.spawn(mqtt_actor).await?;
    runtime.spawn(jwt_actor).await?;
    runtime.spawn(http_actor).await?;
    runtime.spawn(c8y_http_proxy_actor).await?;
    runtime.spawn(fs_watch_actor).await?;
    runtime.spawn(config_actor).await?;
    runtime.spawn(timer_actor).await?;
    runtime.spawn(health_actor).await?;

    runtime.run_to_completion().await?;

    Ok(())
}

fn mqtt_config(tedge_config: &TEdgeConfig) -> Result<MqttConfig, CertificateError> {
    tedge_config.mqtt_config()
}
