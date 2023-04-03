use c8y_config_manager::ConfigManagerBuilder;
use c8y_config_manager::ConfigManagerConfig;
use c8y_http_proxy::credentials::C8YJwtRetriever;
use c8y_http_proxy::C8YHttpProxyBuilder;
use clap::Parser;
use std::path::PathBuf;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::Runtime;
use tedge_actors::ServiceConsumer;
use tedge_config::system_services::get_log_level;
use tedge_config::system_services::set_log_level;
use tedge_config::ConfigRepository;
use tedge_config::ConfigSettingAccessor;
use tedge_config::MqttClientHostSetting;
use tedge_config::MqttClientPortSetting;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigError;
use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_http_ext::HttpActor;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_mqtt_ext::MqttConfig;
use tedge_signal_ext::SignalActor;
use tedge_timer_ext::TimerActor;
use tracing::info;

const PLUGIN_NAME: &str = "c8y-configuration-plugin";

const AFTER_HELP_TEXT: &str = r#"On start, `c8y-configuration-plugin` notifies the cloud tenant of the managed configuration files, listed in the `CONFIG_FILE`, sending this list with a `119` on `c8y/s/us`.
`c8y-configuration-plugin` subscribes then to `c8y/s/ds` listening for configuration operation requests (messages `524` and `526`).
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

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let config_plugin_opt = ConfigPluginOpt::parse();

    // Load tedge config from the provided location
    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(&config_plugin_opt.config_dir);
    let log_level = if config_plugin_opt.debug {
        tracing::Level::TRACE
    } else {
        get_log_level(PLUGIN_NAME, &tedge_config_location.tedge_config_root_path)?
    };

    set_log_level(log_level);

    let config_repository = tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
    let tedge_config = config_repository.load()?;

    if config_plugin_opt.init {
        init(config_plugin_opt.config_dir)
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
    let mut http_actor = HttpActor::new().builder();
    let c8y_http_config = (&tedge_config).try_into()?;
    let mut c8y_http_proxy_actor =
        C8YHttpProxyBuilder::new(c8y_http_config, &mut http_actor, &mut jwt_actor);

    let mut fs_watch_actor = FsWatchActorBuilder::new();
    let mut signal_actor = SignalActor::builder();
    let mut timer_actor = TimerActor::builder();

    //Instantiate health monitor actor
    let mut health_actor = HealthMonitorBuilder::new(PLUGIN_NAME);
    let mqtt_config = health_actor.set_init_and_last_will(mqtt_config);
    let mut mqtt_actor = MqttActorBuilder::new(mqtt_config.clone().with_session_name(PLUGIN_NAME));

    health_actor.set_connection(&mut mqtt_actor);

    //Instantiate config manager actor
    let config_manager_config =
        ConfigManagerConfig::from_tedge_config(DEFAULT_TEDGE_CONFIG_PATH, &tedge_config)?;
    let mut config_actor = ConfigManagerBuilder::new(config_manager_config);

    // Connect other actor instances to config manager actor
    config_actor.with_fs_connection(&mut fs_watch_actor)?;
    config_actor.with_c8y_http_proxy(&mut c8y_http_proxy_actor)?;
    config_actor.set_connection(&mut mqtt_actor);
    config_actor.set_connection(&mut timer_actor);

    // Shutdown on SIGINT
    signal_actor.register_peer(NoConfig, runtime.get_handle().get_sender());

    // Run the actors
    // FIXME: having to list all the actors is error prone
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

fn init(cfg_dir: PathBuf) -> Result<(), anyhow::Error> {
    info!("Creating supported operation files");
    c8y_config_manager::init(&cfg_dir)?;
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
