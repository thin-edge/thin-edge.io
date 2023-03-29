use std::path::Path;

use crate::converter::AwsConverter;
use crate::AwsMapperBuilder;
use async_trait::async_trait;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::Runtime;
use tedge_actors::ServiceConsumer;
use tedge_config::AwsMapperTimestamp;
use tedge_config::ConfigSettingAccessor;
use tedge_config::MqttClientHostSetting;
use tedge_config::MqttClientPortSetting;
use tedge_config::TEdgeConfig;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_mapper_core::component::TEdgeComponent;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_mqtt_ext::MqttConfig;
use tedge_signal_ext::SignalActor;
use tracing::info;

const AWS_MAPPER_NAME: &str = "tedge-mapper-aws";

pub struct AwsMapper;

impl AwsMapper {
    pub fn new() -> AwsMapper {
        AwsMapper {}
    }
}

impl Default for AwsMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TEdgeComponent for AwsMapper {
    fn session_name(&self) -> &str {
        AWS_MAPPER_NAME
    }

    async fn init(&self, _config_dir: &Path) -> Result<(), anyhow::Error> {
        info!("Initialize tedge mapper aws");
        self.init_session(AwsConverter::in_topic_filter()).await?;

        Ok(())
    }

    async fn start(
        &self,
        tedge_config: TEdgeConfig,
        _config_dir: &Path,
    ) -> Result<(), anyhow::Error> {
        let runtime_events_logger = None;
        let mut runtime = Runtime::try_new(runtime_events_logger).await?;

        let add_timestamp = tedge_config.query(AwsMapperTimestamp)?.is_set();
        let mqtt_port = tedge_config.query(MqttClientPortSetting)?.into();
        let mqtt_host = tedge_config.query(MqttClientHostSetting)?;

        let mqtt_config = MqttConfig::default()
            .with_host(mqtt_host.clone())
            .with_port(mqtt_port);
        let mut mqtt_actor =
            MqttActorBuilder::new(mqtt_config.clone().with_session_name(self.session_name()));

        let aws_actor = AwsMapperBuilder::new(self.session_name(), add_timestamp);
        let aws_actor = aws_actor.with_connection(&mut mqtt_actor);

        //Instantiate health monitor actor
        let health_actor = HealthMonitorBuilder::new(self.session_name());
        mqtt_actor.mqtt_config = health_actor.set_init_and_last_will(mqtt_actor.mqtt_config);
        let health_actor = health_actor.with_connection(&mut mqtt_actor);

        let mut signal_actor = SignalActor::builder();
        
        // Shutdown on SIGINT
        signal_actor.register_peer(NoConfig, runtime.get_handle().get_sender());

        runtime.spawn(signal_actor).await?;
        runtime.spawn(mqtt_actor).await?;
        runtime.spawn(aws_actor).await?;
        runtime.spawn(health_actor).await?;

        runtime.run_to_completion().await?;
        Ok(())
    }
}
