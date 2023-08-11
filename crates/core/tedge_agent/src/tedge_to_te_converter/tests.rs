use super::converter::TedgetoTeConverter;
use std::time::Duration;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::test_helpers::TimedMessageBox;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ConvertingActor;
use tedge_actors::DynError;
use tedge_actors::Sender;
use tedge_actors::ServiceConsumer;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;

#[tokio::test]
async fn convert_incoming_main_device_measurement_topic() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate SoftwareList MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/measurements"),
        r#"{"temperature": 2500 }"#,
    );
    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///m/"),
        r#"{"temperature": 2500 }"#,
    );
    mqtt_box.send(mqtt_message).await?;

    // Assert SoftwareListRequest
    mqtt_box.assert_received([expected_mqtt_message]).await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_child_device_measurement_topic() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate SoftwareList MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/measurements/child1"),
        r#"{"temperature": 2500 }"#,
    );
    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1///m/"),
        r#"{"temperature": 2500 }"#,
    );
    mqtt_box.send(mqtt_message).await?;

    // Assert SoftwareListRequest
    mqtt_box.assert_received([expected_mqtt_message]).await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_main_device_alarm_topic() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate SoftwareList MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/alarms/critical/MyCustomAlarm"),
        r#"{
            "text": "I raised it",
            "time": "2021-04-23T19:00:00+05:00"
        }"#,
    );

    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///a/MyCustomAlarm"),
        r#"{"text":"I raised it","time":"2021-04-23T19:00:00+05:00","severity":"critical"}"#,
    );

    mqtt_box.send(mqtt_message).await?;

    // Assert SoftwareListRequest
    mqtt_box
        .assert_received_matching(same_json_over_mqtt_msg, [expected_mqtt_message])
        .await;
    Ok(())
}

fn same_json_over_mqtt_msg(left: &MqttMessage, right: &MqttMessage) -> bool {
    let left_msg: serde_json::Value = serde_json::from_slice(left.payload.as_bytes()).unwrap();
    let right_msg: serde_json::Value = serde_json::from_slice(right.payload.as_bytes()).unwrap();

    (left.topic == right.topic) && (left_msg == right_msg)
}

#[tokio::test]
async fn convert_incoming_custom_main_device_alarm_topic() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate SoftwareList MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/alarms/critical/MyCustomAlarm"),
        r#"{
            "text": "I raised it",
            "someOtherCustomFragment": {"nested":{"value": "extra info"}}
        }"#,
    );

    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///a/MyCustomAlarm"),
        r#"{"text":"I raised it","severity":"critical","someOtherCustomFragment":{"nested":{"value":"extra info"}}}"#,
    );

    mqtt_box.send(mqtt_message).await?;

    // Assert SoftwareListRequest
    mqtt_box
        .assert_received_matching(same_json_over_mqtt_msg, [expected_mqtt_message])
        .await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_child_device_alarm_topic() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate SoftwareList MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/alarms/critical/MyCustomAlarm/child"),
        r#"{
           "text": "I raised it",
           "time": "2021-04-23T19:00:00+05:00"
       }"#,
    );

    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child///a/MyCustomAlarm"),
        r#"{"text":"I raised it","time":"2021-04-23T19:00:00+05:00","severity":"critical"}"#,
    );

    mqtt_box.send(mqtt_message).await?;

    // Assert SoftwareListRequest
    mqtt_box
        .assert_received_matching(same_json_over_mqtt_msg, [expected_mqtt_message])
        .await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_main_device_event_topic() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate SoftwareList MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/events/MyEvent"),
        r#"{"text":"Some test event","time":"2021-04-23T19:00:00+05:00"}"#,
    );

    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///e/MyEvent"),
        r#"{"text":"Some test event","time":"2021-04-23T19:00:00+05:00"}"#,
    );

    mqtt_box.send(mqtt_message).await?;

    // Assert SoftwareListRequest
    mqtt_box.assert_received([expected_mqtt_message]).await;
    Ok(())
}

#[tokio::test]
async fn convert_custom_incoming_main_device_event_topic() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate SoftwareList MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/events/MyEvent"),
        r#"{"text":"Some test event","time":"2021-04-23T19:00:00+05:00","someOtherCustomFragment":{"nested":{"value":"extra info"}}}"#,
    );

    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///e/MyEvent"),
        r#"{"text":"Some test event","time":"2021-04-23T19:00:00+05:00","someOtherCustomFragment":{"nested":{"value":"extra info"}}}"#,
    );

    mqtt_box.send(mqtt_message).await?;

    // Assert SoftwareListRequest
    mqtt_box.assert_received([expected_mqtt_message]).await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_child_device_event_topic() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate SoftwareList MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/events/MyEvent/child"),
        r#"{"text":"Some test event","time":"2021-04-23T19:00:00+05:00"}"#,
    );

    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child///e/MyEvent"),
        r#"{"text":"Some test event","time":"2021-04-23T19:00:00+05:00"}"#,
    );

    mqtt_box.send(mqtt_message).await?;

    // Assert SoftwareListRequest
    mqtt_box.assert_received([expected_mqtt_message]).await;
    Ok(())
}

// tedge/health/service-name -> te/device/main/service/<service-name>/status/health
// tedge/health/child/service-name -> te/device/child/service/<service-name>/status/health
#[tokio::test]
async fn convert_incoming_main_device_service_health_status() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate SoftwareList MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/health/myservice"),
        r#"{""pid":1234,"status":"up","time":1674739912}"#,
    )
    .with_retain();

    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main/service/myservice/status/health"),
        r#"{""pid":1234,"status":"up","time":1674739912}"#,
    )
    .with_retain();

    mqtt_box.send(mqtt_message).await?;

    // Assert SoftwareListRequest
    mqtt_box.assert_received([expected_mqtt_message]).await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_child_device_service_health_status() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate SoftwareList MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/health/child/myservice"),
        r#"{""pid":1234,"status":"up","time":1674739912}"#,
    )
    .with_retain();

    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child/service/myservice/status/health"),
        r#"{""pid":1234,"status":"up","time":1674739912}"#,
    )
    .with_retain();

    mqtt_box.send(mqtt_message).await?;

    // Assert SoftwareListRequest
    mqtt_box.assert_received([expected_mqtt_message]).await;
    Ok(())
}

async fn spawn_tedge_to_te_converter(
) -> Result<TimedMessageBox<SimpleMessageBox<MqttMessage, MqttMessage>>, DynError> {
    // Tedge to Te topic converter
    let tedge_to_te_converter = TedgetoTeConverter::new();
    let subscriptions: TopicFilter = vec![
        "tedge/measurements",
        "tedge/measurements/+",
        "tedge/events/+",
        "tedge/events/+/+",
        "tedge/alarms/+/+",
        "tedge/alarms/+/+/+",
        "tedge/health/+",
        "tedge/health/+/+",
    ]
    .try_into()?;

    // Tedge to Te converter
    let mut tedge_converter_actor =
        ConvertingActor::builder("TedgetoTeConverter", tedge_to_te_converter, subscriptions);

    let mqtt_box = SimpleMessageBoxBuilder::new("MQTT", 5)
        .with_connection(&mut tedge_converter_actor)
        .build()
        .with_timeout(Duration::from_millis(100));

    tokio::spawn(async move { tedge_converter_actor.build().run().await });

    Ok(mqtt_box)
}
