use anyhow::Context;
use c8y_firmware_manager::FirmwareManagerBuilder;
use c8y_firmware_manager::FirmwareManagerConfig;
use std::path::PathBuf;
use tedge_actors::Runtime;
use tedge_api::mqtt_topics::DeviceTopicId;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::Service;
use tedge_config::get_config_dir;
use tedge_config::system_services::log_init;
use tedge_config::system_services::LogConfigArgs;
use tedge_config::ProfileName;
use tedge_config::TEdgeConfig;
use tedge_downloader_ext::DownloaderActor;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_signal_ext::SignalActor;
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
    #[command(flatten)]
    pub log_args: LogConfigArgs,

    /// Create required directories
    #[clap(short, long)]
    pub init: bool,

    /// [env: TEDGE_CONFIG_DIR, default: /etc/tedge]
    #[clap(
        long = "config-dir",
        default_value = get_config_dir().into_os_string(),
        hide_env_values = true,
        hide_default_value = true,
    )]
    pub config_dir: PathBuf,

    #[clap(long, env = "C8Y_PROFILE", hide = true)]
    pub profile: Option<ProfileName>,
}

pub async fn run(firmware_plugin_opt: FirmwarePluginOpt) -> Result<(), anyhow::Error> {
    // Load tedge config from the provided location
    let tedge_config_location =
        tedge_config::TEdgeConfigLocation::from_custom_root(&firmware_plugin_opt.config_dir);

    log_init(
        "c8y-firmware-plugin",
        &firmware_plugin_opt.log_args,
        &tedge_config_location.tedge_config_root_path,
    )?;

    let tedge_config = tedge_config::TEdgeConfig::try_new(tedge_config_location)?;
    let c8y_profile = firmware_plugin_opt.profile.as_deref();

    if firmware_plugin_opt.init {
        warn!("This --init option has been deprecated and will be removed in a future release");
        Ok(())
    } else {
        run_with(tedge_config, c8y_profile).await
    }
}

async fn run_with(
    tedge_config: TEdgeConfig,
    c8y_profile: Option<&str>,
) -> Result<(), anyhow::Error> {
    let mut runtime = Runtime::new();

    // Create actor instances
    let mqtt_config = tedge_config.mqtt_config()?;
    let identity = tedge_config.http.client.auth.identity()?;
    let cloud_root_certs = tedge_config.cloud_root_certs();
    let mut downloader_actor = DownloaderActor::new(identity, cloud_root_certs).builder();
    let mut mqtt_actor = MqttActorBuilder::new(mqtt_config.clone().with_session_name(PLUGIN_NAME));

    //Instantiate health monitor actor
    // TODO: take a user-configurable service topic id
    let mqtt_device_topic_id = &tedge_config
        .mqtt
        .device_topic_id
        .parse::<EntityTopicId>()
        .unwrap();

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
    let mqtt_schema = MqttSchema::with_root(tedge_config.mqtt.topic_root.to_string());
    let health_actor = HealthMonitorBuilder::from_service_topic_id(
        service,
        &mut mqtt_actor,
        &mqtt_schema,
        &tedge_config.service,
    );

    // Instantiate firmware manager actor
    let firmware_manager_config =
        FirmwareManagerConfig::from_tedge_config(&tedge_config, c8y_profile)?;
    let firmware_actor = FirmwareManagerBuilder::try_new(
        firmware_manager_config,
        &mut mqtt_actor,
        &mut downloader_actor,
    )?;

    // Shutdown on SIGINT
    let signal_actor = SignalActor::builder(&runtime.get_handle());

    // Run the actors
    runtime.spawn(signal_actor).await?;
    runtime.spawn(mqtt_actor).await?;
    runtime.spawn(downloader_actor).await?;
    runtime.spawn(firmware_actor).await?;
    runtime.spawn(health_actor).await?;

    runtime.run_to_completion().await?;

    Ok(())
}
