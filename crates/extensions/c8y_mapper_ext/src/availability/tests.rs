use crate::availability::actor::TimerPayload;
use crate::availability::AvailabilityBuilder;
use crate::availability::AvailabilityConfig;
use crate::availability::TimerComplete;
use crate::availability::TimerStart;
use serde_json::json;
use std::time::Duration;
use tedge_actors::test_helpers::FakeServerBox;
use tedge_actors::test_helpers::FakeServerBoxBuilder;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::test_helpers::WithTimeout;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::MessageReceiver;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
use tedge_mqtt_ext::test_helpers::assert_received_includes_json;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_timer_ext::Timeout;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

#[tokio::test]
async fn main_device_init() {
    let config = get_availability_config(10);
    let handlers = spawn_availability_actor(config).await;
    let mut mqtt = handlers.mqtt_box.with_timeout(TEST_TIMEOUT_MS);
    let mut timer = handlers.timer_box;

    // SmartREST
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "117,10")]).await;

    // Timer request
    let timer_start = timer_recv(&mut timer).await;
    assert_eq!(timer_start.duration, Duration::from_secs(10 * 60));
    assert_eq!(
        timer_start.event,
        TimerPayload {
            topic_id: EntityTopicId::default_main_device()
        }
    );
}

#[tokio::test]
async fn main_device_sends_heartbeat() {
    let config = get_availability_config(10);
    let handlers = spawn_availability_actor(config).await;
    let mut mqtt = handlers.mqtt_box.with_timeout(TEST_TIMEOUT_MS);
    let mut timer = handlers.timer_box;

    mqtt.skip(1).await; // SmartREST 117
    timer_recv(&mut timer).await; // First timer request for main device

    // tedge-agent service is up
    let health_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main/service/tedge-agent/status/health"),
        json!({"status": "up", "pid": 1234}).to_string(),
    );
    mqtt.send(health_message).await.unwrap();

    // timer fired
    timer_send(
        &mut timer,
        TimerPayload {
            topic_id: EntityTopicId::default_main_device(),
        },
    )
    .await;

    // JSON over MQTT message
    assert_received_includes_json(
        &mut mqtt,
        [("c8y/inventory/managedObjects/update/test-device", json!({}))],
    )
    .await;

    // New timer request
    let timer_start = timer_recv(&mut timer).await;
    assert_eq!(timer_start.duration, Duration::from_secs(10 * 60));
    assert_eq!(
        timer_start.event,
        TimerPayload {
            topic_id: EntityTopicId::default_main_device()
        }
    );
}

#[tokio::test]
async fn main_device_does_not_send_heartbeat_when_service_status_is_not_up() {
    let config = get_availability_config(10);
    let handlers = spawn_availability_actor(config).await;
    let mut mqtt = handlers.mqtt_box.with_timeout(TEST_TIMEOUT_MS);
    let mut timer = handlers.timer_box;

    mqtt.skip(1).await; // SmartREST 117
    timer_recv(&mut timer).await; // First timer request for main device

    // tedge-agent service is DOWN
    let health_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main/service/tedge-agent/status/health"),
        json!({"status": "down"}).to_string(),
    );
    mqtt.send(health_message).await.unwrap();

    // timer fired
    timer_send(
        &mut timer,
        TimerPayload {
            topic_id: EntityTopicId::default_main_device(),
        },
    )
    .await;

    // No MQTT message is sent
    assert!(mqtt.recv().await.is_none());

    // New timer request
    let timer_start = timer_recv(&mut timer).await;
    assert_eq!(timer_start.duration, Duration::from_secs(10 * 60));
    assert_eq!(
        timer_start.event,
        TimerPayload {
            topic_id: EntityTopicId::default_main_device()
        }
    );
}

#[tokio::test]
async fn main_device_sends_heartbeat_based_on_custom_endpoint() {
    let config = get_availability_config(10);
    let handlers = spawn_availability_actor(config).await;
    let mut mqtt = handlers.mqtt_box.with_timeout(TEST_TIMEOUT_MS);
    let mut timer = handlers.timer_box;

    mqtt.skip(1).await; // SmartREST 117
    timer_recv(&mut timer).await; // First timer request for main device

    // registration message
    let registration_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main//"),
        json!({"@id": "test-device", "@type": "device", "@health": "device/main/service/foo"})
            .to_string(),
    );
    mqtt.send(registration_message).await.unwrap();

    // custom "foo" service is up
    let health_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main/service/foo/status/health"),
        json!({"status": "up"}).to_string(),
    );
    mqtt.send(health_message).await.unwrap();

    // timer fired
    timer_send(
        &mut timer,
        TimerPayload {
            topic_id: EntityTopicId::default_main_device(),
        },
    )
    .await;

    // JSON over MQTT message
    assert_received_includes_json(
        &mut mqtt,
        [("c8y/inventory/managedObjects/update/test-device", json!({}))],
    )
    .await;
}

#[tokio::test]
async fn child_device_sends_heartbeat() {
    let config = get_availability_config(10);
    let handlers = spawn_availability_actor(config).await;
    let mut mqtt = handlers.mqtt_box.with_timeout(TEST_TIMEOUT_MS);
    let mut timer = handlers.timer_box;

    mqtt.skip(1).await; // SmartREST 117 for the main device
    timer_recv(&mut timer).await; // First timer request for the main device

    // registration message
    let registration_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1//"),
        json!({"@id": "test-device:device:child1", "@type": "child-device"}).to_string(),
    );
    mqtt.send(registration_message).await.unwrap();

    // SmartREST 117 for the child device
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us/test-device:device:child1", "117,10")],
    )
    .await;

    // tedge-agent of the child device is up
    let health_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1/service/tedge-agent/status/health"),
        json!({"status": "up"}).to_string(),
    );
    mqtt.send(health_message).await.unwrap();

    // timer fired
    timer_send(
        &mut timer,
        TimerPayload {
            topic_id: EntityTopicId::default_child_device("child1").unwrap(),
        },
    )
    .await;

    // JSON over MQTT message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/inventory/managedObjects/update/test-device:device:child1",
            json!({}),
        )],
    )
    .await;

    // New timer request
    let timer_start = timer_recv(&mut timer).await;
    assert_eq!(timer_start.duration, Duration::from_secs(10 * 60));
    assert_eq!(
        timer_start.event,
        TimerPayload {
            topic_id: EntityTopicId::default_child_device("child1").unwrap()
        }
    );
}

#[tokio::test]
async fn child_device_does_not_send_heartbeat_when_service_status_is_not_up() {
    let config = get_availability_config(10);
    let handlers = spawn_availability_actor(config).await;
    let mut mqtt = handlers.mqtt_box.with_timeout(TEST_TIMEOUT_MS);
    let mut timer = handlers.timer_box;

    mqtt.skip(1).await; // SmartREST 117 for the main device
    timer_recv(&mut timer).await; // First timer request for the main device

    // registration message
    let registration_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1//"),
        json!({"@id": "test-device:device:child1", "@type": "child-device"}).to_string(),
    );
    mqtt.send(registration_message).await.unwrap();

    mqtt.skip(1).await; // SmartREST 117 for the child device

    // tedge-agent of the child device is UNKNOWN
    let health_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1/service/tedge-agent/status/health"),
        json!({"status": "unknown"}).to_string(),
    );
    mqtt.send(health_message).await.unwrap();

    // timer fired
    timer_send(
        &mut timer,
        TimerPayload {
            topic_id: EntityTopicId::default_child_device("child1").unwrap(),
        },
    )
    .await;

    // No MQTT message is sent
    assert!(mqtt.recv().await.is_none());

    // New timer request
    let timer_start = timer_recv(&mut timer).await;
    assert_eq!(timer_start.duration, Duration::from_secs(10 * 60));
    assert_eq!(
        timer_start.event,
        TimerPayload {
            topic_id: EntityTopicId::default_child_device("child1").unwrap()
        }
    );
}

#[tokio::test]
async fn child_device_sends_heartbeat_based_on_custom_endpoint() {
    let config = get_availability_config(10);
    let handlers = spawn_availability_actor(config).await;
    let mut mqtt = handlers.mqtt_box.with_timeout(TEST_TIMEOUT_MS);
    let mut timer = handlers.timer_box;

    mqtt.skip(1).await; // SmartREST 117 for the main device
    timer_recv(&mut timer).await; // First timer request for the main device

    // registration message
    let registration_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1//"),
        json!({"@id": "test-device:device:child1", "@type": "child-device", "@health": "device/child1/service/foo"}).to_string(),
    );
    mqtt.send(registration_message).await.unwrap();

    // SmartREST 117 for the child device
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us/test-device:device:child1", "117,10")],
    )
    .await;

    // Custom service "foo" is up
    let health_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1/service/foo/status/health"),
        json!({"status": "up"}).to_string(),
    );
    mqtt.send(health_message).await.unwrap();

    // timer fired
    timer_send(
        &mut timer,
        TimerPayload {
            topic_id: EntityTopicId::default_child_device("child1").unwrap(),
        },
    )
    .await;

    // JSON over MQTT message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/inventory/managedObjects/update/test-device:device:child1",
            json!({}),
        )],
    )
    .await;
}

#[tokio::test]
async fn interval_is_zero_value() {
    let config = get_availability_config(0);
    let handlers = spawn_availability_actor(config).await;
    let mut mqtt = handlers.mqtt_box.with_timeout(TEST_TIMEOUT_MS);
    let mut timer = handlers.timer_box;

    // SmartREST 117 for the main device
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "117,0")]).await;

    // No timer request created
    assert!(timer.recv().with_timeout(TEST_TIMEOUT_MS).await.is_err());

    // Child registration message
    let registration_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1//"),
        json!({"@id": "test-device:device:child1", "@type": "child-device"}).to_string(),
    );
    mqtt.send(registration_message).await.unwrap();

    // SmartREST 117 for the child device
    assert_received_contains_str(&mut mqtt, [("c8y/s/us/test-device:device:child1", "117,0")])
        .await;

    // No timer request created
    assert!(timer.recv().with_timeout(TEST_TIMEOUT_MS).await.is_err());
}

async fn timer_recv(timer: &mut FakeServerBox<TimerStart, TimerComplete>) -> TimerStart {
    timer
        .recv()
        .with_timeout(TEST_TIMEOUT_MS)
        .await
        .unwrap()
        .unwrap()
}

async fn timer_send(timer: &mut FakeServerBox<TimerStart, TimerComplete>, event: TimerPayload) {
    timer
        .send(Timeout::new(event))
        .with_timeout(TEST_TIMEOUT_MS)
        .await
        .unwrap()
        .unwrap()
}

struct TestHandler {
    pub mqtt_box: SimpleMessageBox<MqttMessage, MqttMessage>,
    pub timer_box: FakeServerBox<TimerStart, TimerComplete>,
}

async fn spawn_availability_actor(config: AvailabilityConfig) -> TestHandler {
    let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 10);
    let mut timer_builder: FakeServerBoxBuilder<TimerStart, TimerComplete> =
        FakeServerBoxBuilder::default();

    let availability_builder =
        AvailabilityBuilder::new(config, &mut mqtt_builder, &mut timer_builder);

    let actor = availability_builder.build();
    tokio::spawn(async move { actor.run().await });

    TestHandler {
        mqtt_box: mqtt_builder.build(),
        timer_box: timer_builder.build(),
    }
}

fn get_availability_config(interval_in_minutes: u64) -> AvailabilityConfig {
    AvailabilityConfig {
        main_device_id: "test-device".into(),
        mqtt_schema: MqttSchema::default(),
        c8y_prefix: "c8y".try_into().unwrap(),
        enable: true,
        interval: Duration::from_secs(interval_in_minutes * 60),
    }
}
