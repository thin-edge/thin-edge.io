use crate::HealthMonitorBuilder;
use crate::TopicFilter;
use std::time::Duration;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::Service;
use tedge_api::service_command::ServiceActions;
use tedge_config::TEdgeConfig;
use tedge_mqtt_ext::MqttConfig;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tokio::time::timeout;

const TEST_TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::test]
async fn send_health_check_message_to_generic_topic() -> Result<(), anyhow::Error> {
    let mut mqtt_config = MqttConfig::default();
    let mut mqtt_message_box = spawn_a_health_check_actor(
        "health-check-service-1",
        &mut mqtt_config,
        ServiceActions::NONE,
    )
    .await;
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
    let mut mqtt_message_box = spawn_a_health_check_actor(
        "health-check-service-2",
        &mut mqtt_config,
        ServiceActions::NONE,
    )
    .await;

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
    let mut mqtt_box =
        spawn_a_health_check_actor("test", &mut mqtt_config, ServiceActions::NONE).await;

    let expected_last_will = MqttMessage::new(
        &Topic::new_unchecked("te/device/main/service/test/status/health"),
        format!(r#"{{"pid":{},"status":"down"}}"#, std::process::id()),
    );
    let expected_last_will = expected_last_will.with_retain();
    assert_eq!(mqtt_config.last_will_message, Some(expected_last_will));

    mqtt_box
        .assert_received([MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/test"),
            r#"{"@parent":"device/main//","@type":"service","name":"test","type":"service"}"#,
        )
        .with_retain()])
        .await;

    Ok(())
}

#[tokio::test]
async fn a_service_declaring_actions_publishes_their_capability() -> Result<(), anyhow::Error> {
    let mut mqtt_config = MqttConfig::default();
    let mut mqtt_box = spawn_a_health_check_actor(
        "test",
        &mut mqtt_config,
        ServiceActions::declaring(&["restart", "enable"]),
    )
    .await;

    // Skip the service registration message
    mqtt_box.skip(1).await;

    mqtt_box
        .assert_received([
            MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/test/cmd/restart"),
                "{}",
            )
            .with_retain(),
            MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/test/cmd/enable"),
                "{}",
            )
            .with_retain(),
        ])
        .await;

    Ok(())
}

#[tokio::test]
async fn a_service_declaring_no_action_publishes_its_health_next() -> Result<(), anyhow::Error> {
    let mut mqtt_config = MqttConfig::default();
    let mut mqtt_box =
        spawn_a_health_check_actor("test", &mut mqtt_config, ServiceActions::NONE).await;

    // skip the registration message
    mqtt_box.skip(1).await;

    let message = timeout(TEST_TIMEOUT, mqtt_box.recv()).await?.unwrap();
    assert_eq!(
        message.topic,
        Topic::new_unchecked("te/device/main/service/test/status/health")
    );

    Ok(())
}

#[tokio::test]
async fn a_service_withdrawing_actions_clears_their_capability() -> Result<(), anyhow::Error> {
    let mut mqtt_config = MqttConfig::default();
    let actions = ServiceActions {
        declared: &["restart"],
        withdrawn: &["start", "stop"],
    };
    let mut mqtt_box = spawn_a_health_check_actor("test", &mut mqtt_config, actions).await;

    // skip the registration message
    mqtt_box.skip(1).await;

    mqtt_box
        .assert_received([
            MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/test/cmd/restart"),
                "{}",
            )
            .with_retain(),
            // An empty payload withdraws a capability, and clears the retained message with it
            MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/test/cmd/start"),
                "",
            )
            .with_retain(),
            MqttMessage::new(
                &Topic::new_unchecked("te/device/main/service/test/cmd/stop"),
                "",
            )
            .with_retain(),
        ])
        .await;

    Ok(())
}

async fn spawn_a_health_check_actor(
    service_to_be_monitored: &str,
    mqtt_config: &mut MqttConfig,
    actions: ServiceActions,
) -> SimpleMessageBox<MqttMessage, MqttMessage> {
    let mut health_mqtt_builder = MqttActorBuilder::new(mqtt_config);

    let mqtt_schema = MqttSchema::new();
    let config = TEdgeConfig::load_toml_str("service.ty = \"service\"");
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
    )
    .with_actions(actions);

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

impl AsMut<MqttConfig> for MqttActorBuilder<'_> {
    fn as_mut(&mut self) -> &mut MqttConfig {
        self.mqtt_config
    }
}

impl MessageSource<MqttMessage, TopicFilter> for MqttActorBuilder<'_> {
    fn connect_sink(&mut self, config: TopicFilter, peer: &impl MessageSink<MqttMessage>) {
        self.message_box.connect_sink(config, peer)
    }
}

impl MessageSink<MqttMessage> for MqttActorBuilder<'_> {
    fn get_sender(&self) -> DynSender<MqttMessage> {
        self.message_box.get_sender()
    }
}
