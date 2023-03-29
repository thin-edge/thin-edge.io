use std::path::Path;

use crate::converter::AzureConverter;
use crate::AzureMapperBuilder;
use async_trait::async_trait;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::Runtime;
use tedge_actors::ServiceConsumer;
use tedge_config::AzureMapperTimestamp;
use tedge_config::ConfigSettingAccessor;
use tedge_config::MqttClientHostSetting;
use tedge_config::MqttClientPortSetting;
use tedge_config::TEdgeConfig;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_mapper_core::component::TEdgeComponent;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_mqtt_ext::MqttConfig;
use tedge_signal_ext::SignalActor;
use tedge_utils::file::create_directory_with_user_group;
use tracing::info;

const AZURE_MAPPER_NAME: &str = "tedge-mapper-az";

pub struct AzureMapper {}

impl AzureMapper {
    pub fn new() -> AzureMapper {
        AzureMapper {}
    }
}

impl Default for AzureMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TEdgeComponent for AzureMapper {
    fn session_name(&self) -> &str {
        AZURE_MAPPER_NAME
    }

    async fn init(&self, config_dir: &Path) -> Result<(), anyhow::Error> {
        info!("Initialize tedge mapper az");
        create_directory_with_user_group(
            format!("{}/operations/az", config_dir.display()),
            "tedge",
            "tedge",
            0o775,
        )?;

        self.init_session(AzureConverter::in_topic_filter()).await?;
        Ok(())
    }

    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        _config_dir: &Path,
    ) -> Result<(), anyhow::Error> {
        let runtime_events_logger = None;
        let mut runtime = Runtime::try_new(runtime_events_logger).await?;

        let add_timestamp = tedge_config.query(AzureMapperTimestamp)?.is_set();
        let mqtt_port = tedge_config.query(MqttClientPortSetting)?.into();
        let mqtt_host = tedge_config.query(MqttClientHostSetting)?;

        let mqtt_config = MqttConfig::default()
            .with_host(mqtt_host.clone())
            .with_port(mqtt_port);
        let mut mqtt_actor =
            MqttActorBuilder::new(mqtt_config.clone().with_session_name(self.session_name()));

        let azure_actor = AzureMapperBuilder::new(self.session_name(), add_timestamp);
        let azure_actor = azure_actor.with_connection(&mut mqtt_actor);

        //Instantiate health monitor actor
        let health_actor = HealthMonitorBuilder::new(self.session_name());
        mqtt_actor.mqtt_config = health_actor.set_init_and_last_will(mqtt_actor.mqtt_config);
        let health_actor = health_actor.with_connection(&mut mqtt_actor);

        let mut signal_actor = SignalActor::builder();

        // Shutdown on SIGINT
        signal_actor.register_peer(NoConfig, runtime.get_handle().get_sender());

        runtime.spawn(signal_actor).await?;
        runtime.spawn(mqtt_actor).await?;
        runtime.spawn(azure_actor).await?;
        runtime.spawn(health_actor).await?;

        runtime.run_to_completion().await?;
        Ok(())
    }
}
