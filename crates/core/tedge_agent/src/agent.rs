use crate::file_transfer_server::actor::FileTransferServerBuilder;
use crate::file_transfer_server::http_rest::HttpConfig;
use crate::restart_manager::builder::RestartManagerBuilder;
use crate::restart_manager::config::RestartManagerConfig;
use crate::software_manager::builder::SoftwareManagerBuilder;
use crate::software_manager::config::SoftwareManagerConfig;
use crate::tedge_operation_converter::builder::TedgeOperationConverterBuilder;
use crate::tedge_to_te_converter::converter::TedgetoTeConverter;
use crate::AgentOpt;
use anyhow::Context;
use camino::Utf8PathBuf;
use flockfile::check_another_instance_is_not_running;
use flockfile::Flockfile;
use flockfile::FlockfileError;
use std::fmt::Debug;
use std::sync::Arc;
use tedge_actors::ConvertingActor;
use tedge_actors::ConvertingActorBuilder;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::Runtime;
use tedge_api::mqtt_topics::DeviceTopicId;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::Service;
use tedge_api::path::DataDir;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_mqtt_ext::MqttConfig;
use tedge_mqtt_ext::TopicFilter;
use tedge_signal_ext::SignalActor;
use tedge_utils::file::create_directory_with_defaults;
use tracing::info;
use tracing::instrument;

const TEDGE_AGENT: &str = "tedge-agent";

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub mqtt_config: MqttConfig,
    pub http_config: HttpConfig,
    pub restart_config: RestartManagerConfig,
    pub sw_update_config: SoftwareManagerConfig,
    pub config_dir: Utf8PathBuf,
    pub run_dir: Utf8PathBuf,
    pub use_lock: bool,
    pub log_dir: Utf8PathBuf,
    pub data_dir: DataDir,
    pub mqtt_device_topic_id: EntityTopicId,
    pub mqtt_topic_root: Arc<str>,
    pub service_type: String,
}

impl AgentConfig {
    pub fn from_config_and_cliopts(
        tedge_config_location: &tedge_config::TEdgeConfigLocation,
        cliopts: AgentOpt,
    ) -> Result<Self, anyhow::Error> {
        let config_repository =
            tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
        let tedge_config = config_repository.load()?;

        let config_dir = tedge_config_location.tedge_config_root_path.clone();

        let mqtt_topic_root = cliopts
            .mqtt_topic_root
            .unwrap_or(tedge_config.mqtt.topic_root.clone().into());

        let mqtt_device_topic_id = cliopts
            .mqtt_device_topic_id
            .unwrap_or(tedge_config.mqtt.device_topic_id.clone().into())
            .parse()
            .context("Could not parse the device MQTT topic")?;

        let mqtt_session_name = format!("{TEDGE_AGENT}#{mqtt_topic_root}/{mqtt_device_topic_id}");

        let mqtt_config = tedge_config
            .mqtt_config()?
            .with_max_packet_size(10 * 1024 * 1024)
            .with_session_name(mqtt_session_name);

        // HTTP config
        let data_dir: DataDir = tedge_config.data.path.clone().into();
        let http_bind_address = tedge_config.http.bind.address;
        let http_port = tedge_config.http.bind.port;

        let http_config = HttpConfig::default()
            .with_data_dir(data_dir.clone())
            .with_port(http_port)
            .with_ip_address(http_bind_address);

        // Restart config
        let restart_config =
            RestartManagerConfig::from_tedge_config(&mqtt_device_topic_id, tedge_config_location)?;

        // Software update config
        let sw_update_config = SoftwareManagerConfig::from_tedge_config(tedge_config_location)?;

        // For flockfile
        let run_dir = tedge_config.run.path.clone();
        let use_lock = tedge_config.run.lock_files;

        // For agent specific
        let log_dir = tedge_config.logs.path.join("tedge").join("agent");

        Ok(Self {
            mqtt_config,
            http_config,
            restart_config,
            sw_update_config,
            config_dir,
            run_dir,
            use_lock,
            data_dir,
            log_dir,
            mqtt_topic_root,
            mqtt_device_topic_id,
            service_type: tedge_config.service.ty.clone(),
        })
    }
}

#[derive(Debug)]
pub struct Agent {
    config: AgentConfig,
    _flock: Option<Flockfile>,
}

impl Agent {
    pub fn try_new(name: &str, config: AgentConfig) -> Result<Self, FlockfileError> {
        let mut flock = None;
        if config.use_lock {
            flock = check_another_instance_is_not_running(name, config.run_dir.as_std_path())?;
        }
        info!("{} starting", &name);

        Ok(Self {
            config,
            _flock: flock,
        })
    }

    #[instrument(skip(self), name = "sm-agent")]
    pub fn init(&self) -> Result<(), anyhow::Error> {
        // `config_dir` by default is `/etc/tedge` (or whatever the user sets with --config-dir)
        create_directory_with_defaults(self.config.config_dir.join(".agent"))?;
        create_directory_with_defaults(self.config.log_dir.clone())?;
        create_directory_with_defaults(self.config.data_dir.clone())?;
        create_directory_with_defaults(self.config.http_config.data_dir.file_transfer_dir())?;
        create_directory_with_defaults(self.config.http_config.data_dir.cache_dir())?;

        Ok(())
    }

    #[instrument(skip(self), name = "sm-agent")]
    pub async fn start(&mut self) -> Result<(), anyhow::Error> {
        info!("Starting tedge agent");
        self.init()?;

        // Runtime
        let runtime_events_logger = None;
        let mut runtime = Runtime::try_new(runtime_events_logger).await?;

        // File transfer server actor
        let file_transfer_server_builder =
            FileTransferServerBuilder::new(self.config.http_config.clone());

        // Restart actor
        let mut restart_actor_builder =
            RestartManagerBuilder::new(self.config.restart_config.clone());

        // Mqtt actor
        let mut mqtt_actor_builder = MqttActorBuilder::new(self.config.mqtt_config.clone());

        // Software update actor
        let mut software_update_builder =
            SoftwareManagerBuilder::new(self.config.sw_update_config.clone());

        // Converter actor
        let converter_actor_builder = TedgeOperationConverterBuilder::new(
            self.config.mqtt_topic_root.as_ref(),
            self.config.mqtt_device_topic_id.clone(),
            &mut software_update_builder,
            &mut restart_actor_builder,
            &mut mqtt_actor_builder,
        );

        // Shutdown on SIGINT
        let signal_actor_builder = SignalActor::builder(&runtime.get_handle());

        // Health actor
        // TODO: take a user-configurable service topic id
        let service_topic_id = self.config.mqtt_device_topic_id.to_default_service_topic_id("tedge-agent")
            .with_context(|| format!("Device topic id {} currently needs default scheme, e.g: 'device/DEVICE_NAME//'", self.config.mqtt_device_topic_id))?;
        let service = Service {
            service_topic_id,
            device_topic_id: DeviceTopicId::new(self.config.mqtt_device_topic_id.clone()),
        };
        let mqtt_schema = MqttSchema::with_root(self.config.mqtt_topic_root.to_string());
        let health_actor = HealthMonitorBuilder::from_service_topic_id(
            service,
            &mut mqtt_actor_builder,
            &mqtt_schema,
            self.config.service_type.clone(),
        );

        // Tedge to Te topic converter
        let tedge_to_te_converter = create_tedge_to_te_converter(&mut mqtt_actor_builder)?;

        // Spawn all
        runtime.spawn(signal_actor_builder).await?;
        runtime.spawn(mqtt_actor_builder).await?;
        runtime.spawn(restart_actor_builder).await?;
        runtime.spawn(software_update_builder).await?;
        runtime.spawn(converter_actor_builder).await?;
        runtime.spawn(health_actor).await?;

        // TODO: replace with a call to entity store when we stop assuming default MQTT schema
        let is_main_device =
            self.config.mqtt_device_topic_id == EntityTopicId::default_main_device();
        if is_main_device {
            info!(
                "Running as a main device, starting tedge_to_te_converter and file transfer actors"
            );
            runtime.spawn(tedge_to_te_converter).await?;
            runtime.spawn(file_transfer_server_builder).await?;
        } else {
            info!("Running as a child device, tedge_to_te_converter and file transfer actors disabled");
        }

        runtime.run_to_completion().await?;

        Ok(())
    }
}

pub fn create_tedge_to_te_converter(
    mqtt_actor_builder: &mut MqttActorBuilder,
) -> Result<ConvertingActorBuilder<TedgetoTeConverter, TopicFilter>, anyhow::Error> {
    let tedge_to_te_converter = TedgetoTeConverter::new();

    let subscriptions: TopicFilter = vec![
        "tedge/measurements",
        "tedge/measurements/+",
        "tedge/events/+",
        "tedge/events/+/+",
        "tedge/alarms/+/+",
        "tedge/alarms/+/+/+",
    ]
    .try_into()?;

    // Tedge to Te converter
    let mut tedge_converter_actor =
        ConvertingActor::builder("TedgetoTeConverter", tedge_to_te_converter, subscriptions);

    tedge_converter_actor.add_input(mqtt_actor_builder);
    tedge_converter_actor.add_sink(mqtt_actor_builder);

    Ok(tedge_converter_actor)
}
