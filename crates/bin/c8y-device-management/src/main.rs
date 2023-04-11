use c8y_config_manager::ConfigManagerBuilder;
use c8y_config_manager::ConfigManagerConfig;
use c8y_firmware_manager::FirmwareManagerBuilder;
use c8y_firmware_manager::FirmwareManagerConfig;
use c8y_http_proxy::credentials::C8YJwtRetriever;
use c8y_http_proxy::C8YHttpProxyBuilder;
use c8y_log_manager::LogManagerBuilder;
use c8y_log_manager::LogManagerConfig;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::Runtime;
use tedge_actors::ServiceConsumer;
use tedge_config::get_tedge_config;
use tedge_config::ConfigSettingAccessor;
use tedge_config::MqttClientHostSetting;
use tedge_config::MqttClientPortSetting;
use tedge_config::TEdgeConfig;
use tedge_config::TEdgeConfigError;
use tedge_config::DEFAULT_TEDGE_CONFIG_PATH;
use tedge_downloader_ext::DownloaderActor;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_http_ext::HttpActor;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_mqtt_ext::MqttConfig;
use tedge_signal_ext::SignalActor;
use tedge_timer_ext::TimerActor;

pub const PLUGIN_NAME: &str = "c8y-device-management";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let runtime_events_logger = None;
    let mut runtime = Runtime::try_new(runtime_events_logger).await?;

    let tedge_config = get_tedge_config()?;
    let c8y_http_config = (&tedge_config).try_into()?;

    // Create actor instances
    let mqtt_config = mqtt_config(&tedge_config)?;

    let mut mqtt_actor = MqttActorBuilder::new(mqtt_config.clone().with_session_name(PLUGIN_NAME));

    let mut jwt_actor = C8YJwtRetriever::builder(mqtt_config);
    let mut http_actor = HttpActor::new().builder();

    let mut c8y_http_proxy_actor =
        C8YHttpProxyBuilder::new(c8y_http_config, &mut http_actor, &mut jwt_actor);

    let mut fs_watch_actor = FsWatchActorBuilder::new();
    let mut signal_actor = SignalActor::builder();
    let mut timer_actor = TimerActor::builder();
    let mut downloader_actor = DownloaderActor::new().builder();

    //Instantiate config manager actor
    let config_manager_config =
        ConfigManagerConfig::from_tedge_config(DEFAULT_TEDGE_CONFIG_PATH, &tedge_config)?;
    let mut config_actor = ConfigManagerBuilder::new(config_manager_config);

    // Connect other actor instances to config manager actor
    config_actor.with_fs_connection(&mut fs_watch_actor)?;
    config_actor.with_c8y_http_proxy(&mut c8y_http_proxy_actor)?;
    config_actor.set_connection(&mut mqtt_actor);
    config_actor.set_connection(&mut timer_actor);

    //Instantiate log manager actor
    let log_manager_config =
        LogManagerConfig::from_tedge_config(DEFAULT_TEDGE_CONFIG_PATH, &tedge_config)?;
    let mut log_actor = LogManagerBuilder::new(log_manager_config);

    // Connect other actor instances to log manager actor
    log_actor.with_fs_connection(&mut fs_watch_actor)?;
    log_actor.with_c8y_http_proxy(&mut c8y_http_proxy_actor)?;
    log_actor.with_mqtt_connection(&mut mqtt_actor)?;

    // Instantiate firmware manager actor
    let firmware_manager_config = FirmwareManagerConfig::from_tedge_config(&tedge_config)?;
    let firmware_actor = FirmwareManagerBuilder::new(
        firmware_manager_config,
        &mut mqtt_actor,
        &mut jwt_actor,
        &mut timer_actor,
        &mut downloader_actor,
    );

    //Instantiate health monitor actor
    let health_actor = HealthMonitorBuilder::new(PLUGIN_NAME);
    mqtt_actor.mqtt_config = health_actor.set_init_and_last_will(mqtt_actor.mqtt_config);
    let health_actor = health_actor.with_connection(&mut mqtt_actor);

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
    runtime.spawn(log_actor).await?;
    runtime.spawn(firmware_actor).await?;
    runtime.spawn(timer_actor).await?;
    runtime.spawn(health_actor).await?;
    runtime.spawn(downloader_actor).await?;

    runtime.run_to_completion().await?;

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
