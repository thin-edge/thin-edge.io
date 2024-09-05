use crate::device_profile_manager::builder::DeviceProfileManagerBuilder;
use crate::file_transfer_server::actor::FileTransferServerBuilder;
use crate::file_transfer_server::actor::FileTransferServerConfig;
use crate::operation_file_cache::FileCacheActorBuilder;
use crate::operation_workflows::load_operation_workflows;
use crate::operation_workflows::OperationConfig;
use crate::operation_workflows::WorkflowActorBuilder;
use crate::restart_manager::builder::RestartManagerBuilder;
use crate::restart_manager::config::RestartManagerConfig;
use crate::software_manager::builder::SoftwareManagerBuilder;
use crate::software_manager::config::SoftwareManagerConfig;
use crate::state_repository::state::agent_state_dir;
use crate::tedge_to_te_converter::converter::TedgetoTeConverter;
use crate::AgentOpt;
use crate::Capabilities;
use anyhow::Context;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use certificate::CloudRootCerts;
use flockfile::check_another_instance_is_not_running;
use flockfile::Flockfile;
use flockfile::FlockfileError;
use reqwest::Identity;
use std::fmt::Debug;
use std::net::SocketAddr;
use std::sync::Arc;
use tedge_actors::Concurrent;
use tedge_actors::ConvertingActor;
use tedge_actors::ConvertingActorBuilder;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::Runtime;
use tedge_actors::ServerActorBuilder;
use tedge_api::mqtt_topics::DeviceTopicId;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::Service;
use tedge_api::path::DataDir;
use tedge_config::TEdgeConfigReaderService;
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
use tracing::warn;

pub const TEDGE_AGENT: &str = "tedge-agent";

#[derive(Debug, Clone)]
pub(crate) struct AgentConfig {
    pub mqtt_config: MqttConfig,
    pub http_config: FileTransferServerConfig,
    pub restart_config: RestartManagerConfig,
    pub sw_update_config: SoftwareManagerConfig,
    pub operation_config: OperationConfig,
    pub config_dir: Utf8PathBuf,
    pub tmp_dir: Arc<Utf8Path>,
    pub run_dir: Utf8PathBuf,
    pub use_lock: bool,
    pub log_dir: Utf8PathBuf,
    pub agent_log_dir: Utf8PathBuf,
    pub data_dir: DataDir,
    pub operations_dir: Utf8PathBuf,
    pub mqtt_device_topic_id: EntityTopicId,
    pub mqtt_topic_root: Arc<str>,
    pub tedge_http_host: Arc<str>,
    pub service: TEdgeConfigReaderService,
    pub identity: Option<Identity>,
    pub cloud_root_certs: CloudRootCerts,
    pub fts_url: Arc<str>,
    pub is_sudo_enabled: bool,
    pub capabilities: Capabilities,
}

impl AgentConfig {
    pub fn from_config_and_cliopts(
        tedge_config_location: &tedge_config::TEdgeConfigLocation,
        cliopts: AgentOpt,
    ) -> Result<Self, anyhow::Error> {
        let tedge_config = tedge_config::TEdgeConfig::try_new(tedge_config_location.clone())?;

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

        // Tedge HTTP config
        let tedge_http_address = tedge_config.http.client.host.clone();
        let tedge_http_port = tedge_config.http.client.port;
        let tedge_http_host = format!("{}:{}", tedge_http_address, tedge_http_port).into();

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

        // Operation Workflow config
        let operation_config = OperationConfig::from_tedge_config(
            mqtt_topic_root.to_string(),
            &mqtt_device_topic_id,
            tedge_config_location,
        )?;

        // For flockfile
        let run_dir = tedge_config.run.path.clone();
        let use_lock = tedge_config.run.lock_files;

        // For agent specific
        let log_dir = tedge_config.logs.path.clone();
        let agent_log_dir = log_dir.join("agent");
        let operations_dir = config_dir.join("operations");

        let identity = tedge_config.http.client.auth.identity()?;
        let cloud_root_certs = tedge_config.cloud_root_certs();

        let is_sudo_enabled = tedge_config.sudo.enable;

        let capabilities = Capabilities {
            config_update: tedge_config.agent.enable.config_update,
            config_snapshot: tedge_config.agent.enable.config_snapshot,
            log_upload: tedge_config.agent.enable.log_upload,
        };
        let fts_url = format!(
            "{}:{}",
            tedge_config.http.client.host, tedge_config.http.client.port
        )
        .into();

        Ok(Self {
            mqtt_config,
            http_config,
            restart_config,
            sw_update_config,
            operation_config,
            config_dir,
            run_dir,
            tmp_dir,
            use_lock,
            data_dir,
            log_dir,
            agent_log_dir,
            operations_dir,
            mqtt_topic_root,
            mqtt_device_topic_id,
            tedge_http_host,
            identity,
            cloud_root_certs,
            fts_url,
            is_sudo_enabled,
            service: tedge_config.service.clone(),
            capabilities,
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
        create_directory_with_defaults(&self.config.agent_log_dir)?;
        create_directory_with_defaults(&self.config.data_dir)?;
        create_directory_with_defaults(&self.config.http_config.file_transfer_dir)?;
        create_directory_with_defaults(self.config.data_dir.cache_dir())?;
        create_directory_with_defaults(self.config.operations_dir.clone())?;

        Ok(())
    }

    #[instrument(skip(self), name = "sm-agent")]
    pub async fn start(self) -> Result<(), anyhow::Error> {
        let version = env!("CARGO_PKG_VERSION");
        info!("Starting tedge-agent v{}", version);
        self.init()?;

        // Runtime
        let mut runtime = Runtime::new();

        let mqtt_schema = MqttSchema::with_root(self.config.mqtt_topic_root.to_string());

        // Operation workflows
        let workflows = load_operation_workflows(&self.config.operations_dir).await?;
        let mut script_runner: ServerActorBuilder<ScriptActor, Concurrent> = ScriptActor::builder();

        // Restart actor
        let mut restart_actor_builder = RestartManagerBuilder::new(self.config.restart_config);

        // Mqtt actor
        let mut mqtt_actor_builder = MqttActorBuilder::new(self.config.mqtt_config);

        // device_profile_actor_builder.connect_sink(NoConfig, &mqtt_actor_builder);

        // Software update actor
        let mut software_update_builder = SoftwareManagerBuilder::new(self.config.sw_update_config);

        // Load device profile manager before the workflow actor
        // as it will create the device_profile workflow if it does not already exist
        let mut device_profile_actor_builder = DeviceProfileManagerBuilder::try_new(
            mqtt_schema.clone(),
            &self.config.operations_dir,
            &mut mqtt_actor_builder,
        )?;

        // Converter actor
        let mut converter_actor_builder = WorkflowActorBuilder::new(
            self.config.operation_config,
            workflows,
            &mut mqtt_actor_builder,
            &mut script_runner,
        );
        converter_actor_builder.register_builtin_operation(&mut restart_actor_builder);
        converter_actor_builder.register_builtin_operation(&mut software_update_builder);
        converter_actor_builder.register_builtin_operation(&mut device_profile_actor_builder);

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
        let health_actor = HealthMonitorBuilder::from_service_topic_id(
            service,
            &mut mqtt_actor_builder,
            &mqtt_schema,
            &self.config.service,
        );

        let mut fs_watch_actor_builder = FsWatchActorBuilder::new();
        let mut downloader_actor_builder = DownloaderActor::new(
            self.config.identity.clone(),
            self.config.cloud_root_certs.clone(),
        )
        .builder();
        let mut uploader_actor_builder =
            UploaderActor::new(self.config.identity, self.config.cloud_root_certs).builder();

        // Instantiate config manager actor if config_snapshot or both operations are enabled
        let config_actor_builder: Option<ConfigManagerBuilder> =
            if self.config.capabilities.config_snapshot {
                let manager_config = ConfigManagerConfig::from_options(ConfigManagerOptions {
                    config_dir: self.config.config_dir.clone().into(),
                    mqtt_topic_root: mqtt_schema.clone(),
                    mqtt_device_topic_id: self.config.mqtt_device_topic_id.clone(),
                    tedge_http_host: self.config.tedge_http_host,
                    tmp_path: self.config.tmp_dir.clone(),
                    is_sudo_enabled: self.config.is_sudo_enabled,
                    config_update_enabled: self.config.capabilities.config_update,
                })?;
                let mut config_manager = ConfigManagerBuilder::try_new(
                    manager_config,
                    &mut fs_watch_actor_builder,
                    &mut downloader_actor_builder,
                    &mut uploader_actor_builder,
                )
                .await?;
                converter_actor_builder.register_builtin_operation(&mut config_manager);
                Some(config_manager)
            } else if self.config.capabilities.config_update {
                warn!("Config_snapshot operation must be enabled to run config_update!");
                None
            } else {
                None
            };

        // Instantiate log manager actor if the operation is enabled
        let log_actor_builder = if self.config.capabilities.log_upload {
            let log_manager_config = LogManagerConfig::from_options(LogManagerOptions {
                config_dir: self.config.config_dir.clone().into(),
                tmp_dir: self.config.tmp_dir.to_path_buf().into(),
                log_dir: self.config.log_dir,
                mqtt_schema: mqtt_schema.clone(),
                mqtt_device_topic_id: self.config.mqtt_device_topic_id.clone(),
            })?;
            let mut log_actor = LogManagerBuilder::try_new(
                log_manager_config,
                &mut fs_watch_actor_builder,
                &mut uploader_actor_builder,
            )
            .await?;
            converter_actor_builder.register_builtin_operation(&mut log_actor);
            Some(log_actor)
        } else {
            None
        };

        // TODO: replace with a call to entity store when we stop assuming default MQTT schema
        let is_main_device =
            self.config.mqtt_device_topic_id == EntityTopicId::default_main_device();
        if is_main_device {
            info!("Running as a main device, starting tedge_to_te_converter and File Transfer Service");

            // Tedge to Te topic converter
            let tedge_to_te_converter = create_tedge_to_te_converter(&mut mqtt_actor_builder)?;
            runtime.spawn(tedge_to_te_converter).await?;

            let file_transfer_server_builder =
                FileTransferServerBuilder::try_bind(self.config.http_config).await?;
            runtime.spawn(file_transfer_server_builder).await?;

            let operation_file_cache_builder = FileCacheActorBuilder::new(
                mqtt_schema,
                self.config.fts_url.clone(),
                self.config.data_dir,
                &mut downloader_actor_builder,
                &mut mqtt_actor_builder,
            );
            runtime.spawn(operation_file_cache_builder).await?;
        } else {
            info!("Running as a child device, tedge_to_te_converter and File Transfer Service disabled");
        }

        // Spawn all
        runtime.spawn(signal_actor_builder).await?;
        runtime.spawn(mqtt_actor_builder).await?;
        runtime.spawn(fs_watch_actor_builder).await?;
        runtime.spawn(downloader_actor_builder).await?;
        runtime.spawn(uploader_actor_builder).await?;
        if let Some(config_actor_builder) = config_actor_builder {
            runtime.spawn(config_actor_builder).await?;
        }
        if let Some(log_actor_builder) = log_actor_builder {
            runtime.spawn(log_actor_builder).await?;
        }
        runtime.spawn(restart_actor_builder).await?;
        runtime.spawn(software_update_builder).await?;
        runtime.spawn(device_profile_actor_builder).await?;
        runtime.spawn(script_runner).await?;
        runtime.spawn(converter_actor_builder).await?;
        runtime.spawn(health_actor).await?;

        runtime.run_to_completion().await?;

        Ok(())
    }
}

pub fn create_tedge_to_te_converter(
    mqtt_actor_builder: &mut MqttActorBuilder,
) -> Result<ConvertingActorBuilder<TedgetoTeConverter>, anyhow::Error> {
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
        ConvertingActor::builder("TedgetoTeConverter", tedge_to_te_converter);

    tedge_converter_actor.connect_source(subscriptions, mqtt_actor_builder);
    tedge_converter_actor.connect_sink(NoConfig, mqtt_actor_builder);

    Ok(tedge_converter_actor)
}
