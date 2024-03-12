use super::converter::TedgetoTeConverter;
use std::time::Duration;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::test_helpers::TimedMessageBox;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ConvertingActor;
use tedge_actors::DynError;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;

#[tokio::test]
async fn convert_incoming_main_device_measurement_topic() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate measurement MQTT message received.
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

    // Simulate child measurement MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/measurements/child1"),
        r#"{"temperature": 2500 }"#,
    );
    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1///m/"),
        r#"{"temperature": 2500 }"#,
    );
    mqtt_box.send(mqtt_message).await?;

    // Assert measurement message
    mqtt_box.assert_received([expected_mqtt_message]).await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_main_device_alarm_topic() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate alarm MQTT message received.
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
    let left_msg: Option<serde_json::Value> = serde_json::from_slice(left.payload.as_bytes()).ok();
    let right_msg: Option<serde_json::Value> =
        serde_json::from_slice(right.payload.as_bytes()).ok();

    (left.topic == right.topic) && (left_msg == right_msg)
}

#[tokio::test]
async fn convert_incoming_custom_main_device_alarm_topic() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate custom alarm MQTT message received.
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

    // Assert alarm message
    mqtt_box
        .assert_received_matching(same_json_over_mqtt_msg, [expected_mqtt_message])
        .await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_clear_alarm_message() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate clear alarm MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/alarms/critical/MyCustomAlarm"),
        "",
    );

    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///a/MyCustomAlarm"),
        "",
    );

    mqtt_box.send(mqtt_message).await?;

    // Assert mqtt message
    mqtt_box
        .assert_received_matching(same_json_over_mqtt_msg, [expected_mqtt_message])
        .await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_empty_alarm_message() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate empty alarm MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/alarms/critical/MyCustomAlarm"),
        r#"{}"#,
    );

    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///a/MyCustomAlarm"),
        r#"{"severity":"critical"}"#,
    );

    mqtt_box.send(mqtt_message).await?;

    // Assert empty alarm mqtt message
    mqtt_box
        .assert_received_matching(same_json_over_mqtt_msg, [expected_mqtt_message])
        .await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_empty_alarm_type() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate empty alarm type MQTT message received.
    let mqtt_message = MqttMessage::new(&Topic::new_unchecked("tedge/alarms/critical/"), r#"{}"#);

    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///a/"),
        r#"{"severity":"critical"}"#,
    );

    mqtt_box.send(mqtt_message).await?;

    // Assert empty alarm type message
    mqtt_box
        .assert_received_matching(same_json_over_mqtt_msg, [expected_mqtt_message])
        .await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_empty_severity() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate empty severity MQTT message received.
    let mqtt_message = MqttMessage::new(&Topic::new_unchecked("tedge/alarms//test_type"), r#"{}"#);

    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///a/test_type"),
        r#"{"severity":""}"#,
    );

    mqtt_box.send(mqtt_message).await?;

    // Assert empty severity mqtt message
    mqtt_box
        .assert_received_matching(same_json_over_mqtt_msg, [expected_mqtt_message])
        .await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_child_device_alarm_topic() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate child device alarm MQTT message received.
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

    // Assert child device alarm message
    mqtt_box
        .assert_received_matching(same_json_over_mqtt_msg, [expected_mqtt_message])
        .await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_main_device_event_topic() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate main device event MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/events/MyEvent"),
        r#"{"text":"Some test event","time":"2021-04-23T19:00:00+05:00"}"#,
    );

    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///e/MyEvent"),
        r#"{"text":"Some test event","time":"2021-04-23T19:00:00+05:00"}"#,
    );

    mqtt_box.send(mqtt_message).await?;

    // Assert event message
    mqtt_box.assert_received([expected_mqtt_message]).await;
    Ok(())
}

#[tokio::test]
async fn convert_custom_incoming_main_device_event_topic() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate main device custom MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/events/MyEvent"),
        r#"{"text":"Some test event","time":"2021-04-23T19:00:00+05:00","someOtherCustomFragment":{"nested":{"value":"extra info"}}}"#,
    );

    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///e/MyEvent"),
        r#"{"text":"Some test event","time":"2021-04-23T19:00:00+05:00","someOtherCustomFragment":{"nested":{"value":"extra info"}}}"#,
    );

    mqtt_box.send(mqtt_message).await?;

    // Assert event mqtt message
    mqtt_box.assert_received([expected_mqtt_message]).await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_child_device_event_topic() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let mut mqtt_box = spawn_tedge_to_te_converter().await?;

    // Simulate child event MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("tedge/events/MyEvent/child"),
        r#"{"text":"Some test event","time":"2021-04-23T19:00:00+05:00"}"#,
    );

    let expected_mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child///e/MyEvent"),
        r#"{"text":"Some test event","time":"2021-04-23T19:00:00+05:00"}"#,
    );

    mqtt_box.send(mqtt_message).await?;

    // Assert event mqtt message
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
    ]
    .try_into()?;

    // Tedge to Te converter
    let mut tedge_converter_actor =
        ConvertingActor::builder("TedgetoTeConverter", tedge_to_te_converter);

    let mut mqtt_box = SimpleMessageBoxBuilder::new("MQTT", 5);
    mqtt_box.register_peer(subscriptions, tedge_converter_actor.get_input_sender());
    tedge_converter_actor.register_peer(NoConfig, mqtt_box.get_sender());
    let mqtt_box = mqtt_box.build().with_timeout(Duration::from_millis(100));

    tokio::spawn(async move { tedge_converter_actor.build().run().await });

    Ok(mqtt_box)
}
