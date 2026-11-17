use super::*;

#[tokio::test]
async fn test_sync_alarms() {
    let tmp_dir = TempTedgeDir::new();
    let mut converter = MeaConverter::<AlarmConverter>::new(&tmp_dir);

    let alarm_topic = "te/device/main///a/temperature_alarm";
    let alarm_payload = r#"{ "severity": "critical", "text": "Temperature very high" }"#;
    let alarm_message = MqttMessage::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

    // During the sync phase, alarms are not converted immediately, but only cached to be synced later
    assert!(converter.convert(&alarm_message).await.is_empty());

    let internal_alarm_topic = "c8y-internal/alarms/te/device/main///a/pressure_alarm";
    let internal_alarm_payload = r#"{ "severity": "major", "text": "Temperature very high" }"#;
    let internal_alarm_message = MqttMessage::new(
        &Topic::new_unchecked(internal_alarm_topic),
        internal_alarm_payload,
    );

    // During the sync phase, internal alarms are not converted, but only cached to be synced later
    assert!(converter.convert(&internal_alarm_message).await.is_empty());

    // When sync phase is complete, all pending alarms are returned
    let sync_messages = converter.sync_messages();
    // "c8y/s/us" "306,pressure_alarm"
    // "c8y-internal/alarms/te/device/main///a/pressure_alarm" ""
    // "c8y/s/us" "301,temperature_alarm,Temperature very high,2026-01-28T08:49:52.508528106Z"
    // "c8y-internal/alarms/te/device/main///a/temperature_alarm" { "severity": "critical", "text": "Temperature very high" }
    assert_eq!(sync_messages.len(), 4);

    // The first message will be clear alarm message for pressure_alarm
    let alarm_message = sync_messages.get(0).unwrap();
    assert_eq!(alarm_message.topic.name, "c8y/s/us");
    assert_eq!(alarm_message.payload_str().unwrap(), "306,pressure_alarm");

    // The second message will be the temperature_alarm
    let alarm_message = sync_messages.get(2).unwrap();
    assert_eq!(alarm_message.topic.name, "c8y/s/us");
    assert!(alarm_message
        .payload_str()
        .unwrap()
        .starts_with("301,temperature_alarm,Temperature very high"));

    // After the sync phase, already known alarms are not forwarded to c8y
    assert!(converter.convert(alarm_message).await.is_empty());

    // But, even after the sync phase, internal alarms are not converted and just ignored, as they are purely internal
    assert!(converter.convert(&internal_alarm_message).await.is_empty());
}

#[tokio::test]
async fn test_sync_child_alarms() {
    let tmp_dir = TempTedgeDir::new();
    let mut converter = MeaConverter::<AlarmConverter>::new(&tmp_dir);

    let alarm_topic = "te/device/external_sensor///a/temperature_alarm";
    let alarm_payload = r#"{ "severity": "critical", "text": "Temperature very high" }"#;
    let alarm_message = MqttMessage::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

    converter.register_source_entities(alarm_topic).await;

    // During the sync phase, alarms are not converted immediately, but only cached to be synced later
    assert!(converter.convert(&alarm_message).await.is_empty());

    let internal_alarm_topic = "c8y-internal/alarms/te/device/external_sensor///a/pressure_alarm";
    let internal_alarm_payload = r#"{ "severity": "major", "text": "Temperature very high" }"#;
    let internal_alarm_message = MqttMessage::new(
        &Topic::new_unchecked(internal_alarm_topic),
        internal_alarm_payload,
    );

    // During the sync phase, internal alarms are not converted, but only cached to be synced later
    assert!(converter.convert(&internal_alarm_message).await.is_empty());

    // When sync phase is complete, all pending alarms are returned
    let sync_messages = converter.sync_messages();
    // "c8y/s/us/test-device:device:external_sensor" "306,pressure_alarm"
    // "c8y-internal/alarms/te/device/external_sensor///a/pressure_alarm"  ""
    // "c8y/s/us/test-device:device:external_sensor" "301,temperature_alarm,Temperature very high,2026-01-28T08:58:04.34493434Z"
    // "c8y-internal/alarms/te/device/external_sensor///a/temperature_alarm" { "severity": "critical", "text": "Temperature very high" }
    assert_eq!(sync_messages.len(), 4);

    // The first message will be clear alarm message for pressure_alarm
    let alarm_message = sync_messages.get(0).unwrap();
    assert_eq!(
        alarm_message.topic.name,
        "c8y/s/us/test-device:device:external_sensor"
    );
    assert_eq!(alarm_message.payload_str().unwrap(), "306,pressure_alarm");

    // The second message will be the temperature_alarm
    let alarm_message = sync_messages.get(2).unwrap();
    assert_eq!(
        alarm_message.topic.name,
        "c8y/s/us/test-device:device:external_sensor"
    );
    assert!(alarm_message
        .payload_str()
        .unwrap()
        .starts_with("301,temperature_alarm,Temperature very high"));

    // After the sync phase, already known alarms are not forwarded to c8y
    assert!(converter.convert(alarm_message).await.is_empty());

    // But, even after the sync phase, internal alarms are not converted and just ignored, as they are purely internal
    assert!(converter.convert(&internal_alarm_message).await.is_empty());
}
