use crate::file_transfer_server::actor::FileTransferServerBuilder;
use crate::file_transfer_server::http_rest::HttpConfig;
use crate::restart_manager::builder::RestartManagerBuilder;
use crate::restart_manager::config::RestartManagerConfig;
use crate::software_manager::builder::SoftwareManagerBuilder;
use crate::software_manager::config::SoftwareManagerConfig;
use crate::tedge_operation_converter::builder::TedgeOperationConverterBuilder;
use camino::Utf8PathBuf;
use flockfile::check_another_instance_is_not_running;
use flockfile::Flockfile;
use flockfile::FlockfileError;
use std::fmt::Debug;
use tedge_actors::Runtime;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_mqtt_ext::MqttConfig;
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
    pub data_dir: Utf8PathBuf,
}

impl AgentConfig {
    pub fn from_tedge_config(
        tedge_config_location: &tedge_config::TEdgeConfigLocation,
    ) -> Result<Self, anyhow::Error> {
        let config_repository =
            tedge_config::TEdgeConfigRepository::new(tedge_config_location.clone());
        let tedge_config = config_repository.load_new()?;

        let config_dir = tedge_config_location.tedge_config_root_path.clone();

        let mqtt_config = tedge_config
            .mqtt_config()?
            .with_max_packet_size(10 * 1024 * 1024)
            .with_session_name(TEDGE_AGENT);

        // HTTP config
        let data_dir = tedge_config.data.path.clone();
        let http_bind_address = tedge_config.http.bind.address;
        let http_port = tedge_config.http.bind.port;

        let http_config = HttpConfig::default()
            .with_data_dir(data_dir.clone())
            .with_port(http_port)
            .with_ip_address(http_bind_address);

        // Restart config
        let restart_config = RestartManagerConfig::from_tedge_config(tedge_config_location)?;

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
            flock = Some(check_another_instance_is_not_running(
                name,
                config.run_dir.as_std_path(),
            )?);
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
        create_directory_with_defaults(self.config.http_config.file_transfer_dir_as_string())?;

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
        let mut mqtt_actor_builder = MqttActorBuilder::new(
            self.config
                .mqtt_config
                .clone()
                .with_session_name(TEDGE_AGENT),
        );

        // Software update actor
        let mut software_update_builder =
            SoftwareManagerBuilder::new(self.config.sw_update_config.clone());

        // Converter actor
        let converter_actor_builder = TedgeOperationConverterBuilder::new(
            &mut software_update_builder,
            &mut restart_actor_builder,
            &mut mqtt_actor_builder,
        );

        // Shutdown on SIGINT
        let signal_actor_builder = SignalActor::builder(&runtime.get_handle());

        // Health actor
        let health_actor = HealthMonitorBuilder::new(TEDGE_AGENT, &mut mqtt_actor_builder);

        // Spawn all
        runtime.spawn(signal_actor_builder).await?;
        runtime.spawn(file_transfer_server_builder).await?;
        runtime.spawn(mqtt_actor_builder).await?;
        runtime.spawn(restart_actor_builder).await?;
        runtime.spawn(software_update_builder).await?;
        runtime.spawn(converter_actor_builder).await?;
        runtime.spawn(health_actor).await?;

        runtime.run_to_completion().await?;

        Ok(())
    }
}
