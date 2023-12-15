use crate::HealthMonitorBuilder;
use crate::TopicFilter;
use std::time::Duration;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::MessageReceiver;
use tedge_actors::ServiceProvider;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::Service;
use tedge_config::TEdgeConfigRepository;
use tedge_mqtt_ext::MqttConfig;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tokio::time::timeout;

const TEST_TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::test]
async fn send_health_check_message_to_generic_topic() -> Result<(), anyhow::Error> {
    let mut mqtt_config = MqttConfig::default();
    let mut mqtt_message_box =
        spawn_a_health_check_actor("health-check-service-1", &mut mqtt_config).await;
    // skip registration message

    mqtt_message_box.skip(1).await;

    if let Some(message) = timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await? {
        assert!(
            message.payload_str()?.contains("up"),
            "{} doesn't contain \"up\"",
            message.payload_str().unwrap()
        )
    }

    Ok(())
}

#[tokio::test]
async fn send_health_check_message_to_service_specific_topic() -> Result<(), anyhow::Error> {
    let mut mqtt_config = MqttConfig::default();
    let mut mqtt_message_box =
        spawn_a_health_check_actor("health-check-service-2", &mut mqtt_config).await;

    // skip registration message
    mqtt_message_box.skip(1).await;

    if let Some(message) = timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await? {
        assert!(
            message.payload_str()?.contains("up"),
            "{} doesn't contain \"up\"",
            message.payload_str().unwrap()
        )
    }

    Ok(())
}

#[tokio::test]
async fn health_check_set_init_and_last_will_message() -> Result<(), anyhow::Error> {
    let mut mqtt_config = MqttConfig::default();
    let _ = spawn_a_health_check_actor("test", &mut mqtt_config).await;

    let expected_last_will = MqttMessage::new(
        &Topic::new_unchecked("te/device/main/service/test/status/health"),
        format!(r#"{{"pid":{},"status":"down"}}"#, std::process::id()),
    );
    let expected_last_will = expected_last_will.with_retain();
    assert_eq!(mqtt_config.last_will_message, Some(expected_last_will));

    Ok(())
}

async fn spawn_a_health_check_actor(
    service_to_be_monitored: &str,
    mqtt_config: &mut MqttConfig,
) -> SimpleMessageBox<MqttMessage, MqttMessage> {
    let mut health_mqtt_builder = MqttActorBuilder::new(mqtt_config);

    let mqtt_schema = MqttSchema::new();
    let config = TEdgeConfigRepository::load_toml_str("service.ty = \"service\"");
    let service = Service {
        service_topic_id: EntityTopicId::default_main_service(service_to_be_monitored)
            .unwrap()
            .into(),
        device_topic_id: EntityTopicId::default_main_device().into(),
    };

    let health_actor = HealthMonitorBuilder::from_service_topic_id(
        service,
        &mut health_mqtt_builder,
        &mqtt_schema,
        &config.service,
    );

    let actor = health_actor.build();
    tokio::spawn(async move { actor.run().await });

    health_mqtt_builder.build()
}

struct MqttActorBuilder<'a> {
    mqtt_config: &'a mut MqttConfig,
    message_box: SimpleMessageBoxBuilder<MqttMessage, MqttMessage>,
}

impl<'a> MqttActorBuilder<'a> {
    pub fn new(mqtt_config: &'a mut MqttConfig) -> Self {
        let message_box = SimpleMessageBoxBuilder::new("MQTT", 5);
        MqttActorBuilder {
            mqtt_config,
            message_box,
        }
    }

    pub fn build(self) -> SimpleMessageBox<MqttMessage, MqttMessage> {
        self.message_box.build()
    }
}

impl<'a> AsMut<MqttConfig> for MqttActorBuilder<'a> {
    fn as_mut(&mut self) -> &mut MqttConfig {
        self.mqtt_config
    }
}

impl<'a> ServiceProvider<MqttMessage, MqttMessage, TopicFilter> for MqttActorBuilder<'a> {
    fn connect_consumer(
        &mut self,
        config: TopicFilter,
        response_sender: DynSender<MqttMessage>,
    ) -> DynSender<MqttMessage> {
        self.message_box.connect_consumer(config, response_sender)
    }
}
