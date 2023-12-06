use crate::file_transfer_server::actor::FileTransferServerBuilder;
use crate::file_transfer_server::actor::FileTransferServerConfig;
use crate::restart_manager::builder::RestartManagerBuilder;
use crate::restart_manager::config::RestartManagerConfig;
use crate::software_manager::builder::SoftwareManagerBuilder;
use crate::software_manager::config::SoftwareManagerConfig;
use crate::state_repository::state::agent_state_dir;
use crate::tedge_operation_converter::builder::TedgeOperationConverterBuilder;
use crate::tedge_to_te_converter::converter::TedgetoTeConverter;
use crate::AgentOpt;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use flockfile::check_another_instance_is_not_running;
use flockfile::Flockfile;
use flockfile::FlockfileError;
use log::error;
use reqwest::Identity;
use std::ffi::OsStr;
use std::fmt::Debug;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tedge_actors::Concurrent;
use tedge_actors::ConvertingActor;
use tedge_actors::ConvertingActorBuilder;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::Runtime;
use tedge_actors::ServerActorBuilder;
use tedge_api::mqtt_topics::DeviceTopicId;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::Service;
use tedge_api::path::DataDir;
use tedge_api::workflow::toml_config::TomlOperationWorkflow;
use tedge_api::workflow::OperationWorkflow;
use tedge_api::workflow::WorkflowSupervisor;
use tedge_config_manager::ConfigManagerBuilder;
use tedge_config_manager::ConfigManagerConfig;
use tedge_config_manager::ConfigManagerOptions;
use tedge_downloader_ext::DownloaderActor;
use tedge_file_system_ext::FsWatchActorBuilder;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_log_manager::LogManagerBuilder;
use tedge_log_manager::LogManagerConfig;
use tedge_log_manager::LogManagerOptions;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_mqtt_ext::MqttConfig;
use tedge_mqtt_ext::TopicFilter;
use tedge_script_ext::ScriptActor;
use tedge_signal_ext::SignalActor;
use tedge_uploader_ext::UploaderActor;
use tedge_utils::file::create_directory_with_defaults;
use tracing::info;
use tracing::instrument;

const TEDGE_AGENT: &str = "tedge-agent";

#[derive(Debug, Clone)]
pub(crate) struct AgentConfig {
    pub mqtt_config: MqttConfig,
    pub http_config: FileTransferServerConfig,
    pub restart_config: RestartManagerConfig,
    pub sw_update_config: SoftwareManagerConfig,
    pub config_dir: Utf8PathBuf,
    pub tmp_dir: Arc<Utf8Path>,
    pub run_dir: Utf8PathBuf,
    pub use_lock: bool,
    pub log_dir: Utf8PathBuf,
    pub data_dir: DataDir,
    pub operations_dir: Utf8PathBuf,
    pub mqtt_device_topic_id: EntityTopicId,
    pub mqtt_topic_root: Arc<str>,
    pub service_type: String,
    pub identity: Option<Identity>,
    pub is_sudo_enabled: bool,
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
        let tmp_dir = Arc::from(tedge_config.tmp.path.as_path());

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

        let http_config = FileTransferServerConfig {
            file_transfer_dir: data_dir.file_transfer_dir(),
            cert_path: tedge_config.http.cert_path.clone(),
            key_path: tedge_config.http.key_path.clone(),
            ca_path: tedge_config.http.ca_path.clone(),
            bind_addr: SocketAddr::from((http_bind_address, http_port)),
        };

        // Restart config
        let restart_config =
            RestartManagerConfig::from_tedge_config(&mqtt_device_topic_id, tedge_config_location)?;

        // Software update config
        let sw_update_config = SoftwareManagerConfig::from_tedge_config(tedge_config_location)?;

        // For flockfile
        let run_dir = tedge_config.run.path.clone();
        let use_lock = tedge_config.run.lock_files;

        // For agent specific
        let log_dir = tedge_config.logs.path.join("agent");
        let operations_dir = config_dir.join("operations");

        let identity = tedge_config.http.client.auth.identity()?;

        let is_sudo_enabled = tedge_config.enable.sudo;

        Ok(Self {
            mqtt_config,
            http_config,
            restart_config,
            sw_update_config,
            config_dir,
            run_dir,
            tmp_dir,
            use_lock,
            data_dir,
            log_dir,
            operations_dir,
            mqtt_topic_root,
            mqtt_device_topic_id,
            service_type: tedge_config.service.ty.clone(),
            identity,
            is_sudo_enabled,
        })
    }
}

#[derive(Debug)]
pub struct Agent {
    config: AgentConfig,
    _flock: Option<Flockfile>,
}

impl Agent {
    pub(crate) fn try_new(name: &str, config: AgentConfig) -> Result<Self, FlockfileError> {
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
        create_directory_with_defaults(agent_state_dir(self.config.config_dir.clone()))?;
        create_directory_with_defaults(&self.config.log_dir)?;
        create_directory_with_defaults(&self.config.data_dir)?;
        create_directory_with_defaults(&self.config.http_config.file_transfer_dir)?;
        create_directory_with_defaults(self.config.data_dir.cache_dir())?;
        create_directory_with_defaults(self.config.operations_dir.clone())?;

        Ok(())
    }

    #[instrument(skip(self), name = "sm-agent")]
    pub async fn start(self) -> Result<(), anyhow::Error> {
        info!("Starting tedge agent");
        self.init()?;

        // Runtime
        let runtime_events_logger = None;
        let mut runtime = Runtime::try_new(runtime_events_logger).await?;

        // Operation workflows
        let workflows = self.load_operation_workflows().await?;
        let mut script_runner: ServerActorBuilder<ScriptActor, Concurrent> = ScriptActor::builder();

        // Restart actor
        let mut restart_actor_builder = RestartManagerBuilder::new(self.config.restart_config);

        // Mqtt actor
        let mut mqtt_actor_builder = MqttActorBuilder::new(self.config.mqtt_config);

        // Software update actor
        let mut software_update_builder = SoftwareManagerBuilder::new(self.config.sw_update_config);

        // Converter actor
        let converter_actor_builder = TedgeOperationConverterBuilder::new(
            self.config.mqtt_topic_root.as_ref(),
            self.config.mqtt_device_topic_id.clone(),
            workflows,
            self.config.log_dir.clone(),
            &mut software_update_builder,
            &mut restart_actor_builder,
            &mut mqtt_actor_builder,
            &mut script_runner,
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

        let mut fs_watch_actor_builder = FsWatchActorBuilder::new();
        let mut downloader_actor_builder =
            DownloaderActor::new(self.config.identity.clone()).builder();
        let mut uploader_actor_builder = UploaderActor::new(self.config.identity).builder();

        // Instantiate config manager actor
        let manager_config = ConfigManagerConfig::from_options(ConfigManagerOptions {
            config_dir: self.config.config_dir.clone().into(),
            mqtt_topic_root: mqtt_schema.clone(),
            mqtt_device_topic_id: self.config.mqtt_device_topic_id.clone(),
            tmp_path: self.config.tmp_dir.clone(),
            is_sudo_enabled: self.config.is_sudo_enabled,
        })?;
        let config_actor_builder = ConfigManagerBuilder::try_new(
            manager_config,
            &mut mqtt_actor_builder,
            &mut fs_watch_actor_builder,
            &mut downloader_actor_builder,
            &mut uploader_actor_builder,
        )
        .await?;

        // Instantiate log manager actor
        let log_manager_config = LogManagerConfig::from_options(LogManagerOptions {
            config_dir: self.config.config_dir.clone().into(),
            tmp_dir: self.config.config_dir.into(),
            mqtt_schema,
            mqtt_device_topic_id: self.config.mqtt_device_topic_id.clone(),
        })?;
        let log_actor_builder = LogManagerBuilder::try_new(
            log_manager_config,
            &mut mqtt_actor_builder,
            &mut fs_watch_actor_builder,
            &mut uploader_actor_builder,
        )
        .await?;

        // Spawn all
        runtime.spawn(signal_actor_builder).await?;
        runtime.spawn(mqtt_actor_builder).await?;
        runtime.spawn(fs_watch_actor_builder).await?;
        runtime.spawn(downloader_actor_builder).await?;
        runtime.spawn(uploader_actor_builder).await?;
        runtime.spawn(config_actor_builder).await?;
        runtime.spawn(log_actor_builder).await?;
        runtime.spawn(restart_actor_builder).await?;
        runtime.spawn(software_update_builder).await?;
        runtime.spawn(script_runner).await?;
        runtime.spawn(converter_actor_builder).await?;
        runtime.spawn(health_actor).await?;

        // TODO: replace with a call to entity store when we stop assuming default MQTT schema
        let is_main_device =
            self.config.mqtt_device_topic_id == EntityTopicId::default_main_device();
        if is_main_device {
            info!(
                "Running as a main device, starting tedge_to_te_converter and file transfer actors"
            );

            let file_transfer_server_builder =
                FileTransferServerBuilder::try_bind(self.config.http_config).await?;
            runtime.spawn(tedge_to_te_converter).await?;
            runtime.spawn(file_transfer_server_builder).await?;
        } else {
            info!("Running as a child device, tedge_to_te_converter and file transfer actors disabled");
        }

        runtime.run_to_completion().await?;

        Ok(())
    }

    async fn load_operation_workflows(&self) -> Result<WorkflowSupervisor, anyhow::Error> {
        let dir_path = &self.config.operations_dir;
        let mut workflows = WorkflowSupervisor::default();
        for entry in std::fs::read_dir(dir_path)?.flatten() {
            let file = entry.path();
            if file.extension() == Some(OsStr::new("toml")) {
                match read_operation_workflow(&file)
                    .await
                    .and_then(|workflow| load_operation_workflow(&mut workflows, workflow))
                {
                    Ok(cmd) => {
                        info!("Using operation workflow definition from {file:?} for '{cmd}' operation");
                    }
                    Err(err) => {
                        error!("Ignoring operation workflow definition from {file:?}: {err}")
                    }
                };
            }
        }
        Ok(workflows)
    }
}

async fn read_operation_workflow(path: &Path) -> Result<OperationWorkflow, anyhow::Error> {
    let context = || format!("Reading operation workflow from {path:?}");
    let bytes = tokio::fs::read(path).await.with_context(context)?;
    let input = std::str::from_utf8(&bytes).with_context(context)?;
    let toml = toml::from_str::<TomlOperationWorkflow>(input)?; //.with_context(context)?;
    let workflow = TryInto::<OperationWorkflow>::try_into(toml).with_context(context)?;
    Ok(workflow)
}

fn load_operation_workflow(
    workflows: &mut WorkflowSupervisor,
    workflow: OperationWorkflow,
) -> Result<String, anyhow::Error> {
    let name = workflow.operation.to_string();
    workflows.register_custom_workflow(workflow)?;
    Ok(name)
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
