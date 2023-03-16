use crate::actor::CollectdActorBuilder;
use batcher::BatchingActorBuilder;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::Runtime;
use tedge_actors::RuntimeError;
use tedge_actors::ServiceConsumer;
use tedge_health_ext::HealthMonitorBuilder;
use tedge_mqtt_ext::MqttActorBuilder;
use tedge_mqtt_ext::MqttConfig;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;
use tedge_signal_ext::SignalActor;

#[derive(Debug)]
pub struct DeviceMonitorConfig {
    mapper_name: &'static str,
    host: String,
    port: u16,
    mqtt_client_id: &'static str,
    pub mqtt_source_topic: &'static str,
    mqtt_target_topic: &'static str,
    batching_window: u32,
    maximum_message_delay: u32,
    message_leap_limit: u32,
}

impl Default for DeviceMonitorConfig {
    fn default() -> Self {
        Self {
            mapper_name: "tedge-mapper-collectd",
            host: "localhost".to_string(),
            port: 1883,
            mqtt_client_id: "collectd-mapper",
            mqtt_source_topic: "collectd/#",
            mqtt_target_topic: "tedge/measurements",
            batching_window: 500,
            maximum_message_delay: 400, // Heuristic delay that should work out well on an Rpi
            message_leap_limit: 0,
        }
    }
}

impl DeviceMonitorConfig {
    pub fn with_port(self, port: u16) -> Self {
        Self { port, ..self }
    }

    pub fn with_host(self, host: String) -> Self {
        Self { host, ..self }
    }
}

#[derive(Debug)]
pub struct DeviceMonitor {
    config: DeviceMonitorConfig,
}

impl DeviceMonitor {
    pub fn new(config: DeviceMonitorConfig) -> Self {
        Self { config }
    }

    pub async fn run(&self) -> Result<(), RuntimeError> {
        let mut runtime = Runtime::try_new(None).await?;

        let input_topic =
            TopicFilter::new_unchecked(self.config.mqtt_source_topic).with_qos(QoS::AtMostOnce);
        let output_topic = Topic::new_unchecked(self.config.mqtt_target_topic);

        let mut health_actor = HealthMonitorBuilder::new(self.config.mapper_name);
        let mqtt_config = health_actor.set_init_and_last_will(
            MqttConfig::new(self.config.host.to_string(), self.config.port)
                .with_session_name(self.config.mqtt_client_id),
        );

        let mut mqtt_actor = MqttActorBuilder::new(mqtt_config);
        health_actor.set_connection(&mut mqtt_actor);

        let mut batching_actor = BatchingActorBuilder::default()
            .with_batching_window(self.config.batching_window)
            .with_maximum_message_delay(self.config.maximum_message_delay)
            .with_message_leap_limit(self.config.message_leap_limit);

        let mut collectd_actor = CollectdActorBuilder::new(input_topic);

        collectd_actor.add_input(&mut mqtt_actor);
        batching_actor.add_input(&mut collectd_actor);
        mqtt_actor.add_mapped_input(&mut batching_actor, move |batch| {
            crate::converter::batch_into_mqtt_messages(&output_topic, batch).into_iter()
        });

        // Shutdown on SIGINT
        let mut signal_actor = SignalActor::builder();
        signal_actor.register_peer(NoConfig, runtime.get_handle().get_sender());

        runtime.spawn(signal_actor).await?;
        runtime.spawn(health_actor).await?;
        runtime.spawn(mqtt_actor).await?;
        runtime.spawn(collectd_actor).await?;
        runtime.spawn(batching_actor).await?;

        runtime.run_to_completion().await?;
        Ok(())
    }
}
