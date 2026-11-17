use super::*;
use rstest::rstest;

#[tokio::test]
async fn convert_measurement_with_child_id() {
    let tmp_dir = TempTedgeDir::new();
    let mut converter = MeaConverter::<MeasurementConverter>::new(&tmp_dir);

    let in_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1///m/"),
        json!({
            "temp": 1,
            "time": "2021-11-16T17:45:40.571760714+01:00"
        })
        .to_string(),
    );

    converter
        .register_source_entities(&in_message.topic.name)
        .await;

    let messages = converter.convert(&in_message).await;

    assert_messages_matching(
        &messages,
        [(
            "c8y/measurement/measurements/create",
            json!({
                "externalSource":{
                    "externalId":"test-device:device:child1",
                    "type":"c8y_Serial"
                },
                "temp":{
                    "temp":{
                        "value":1.0
                    }
                },
                "time":"2021-11-16T17:45:40.571760714+01:00",
                "type":"ThinEdgeMeasurement"
            })
            .into(),
        )],
    );
}

#[tokio::test]
async fn convert_measurement_with_nested_child_device() {
    let tmp_dir = TempTedgeDir::new();
    let mut converter = MeaConverter::<MeasurementConverter>::new(&tmp_dir);
    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/immediate_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/main//",
            "@id":"immediate_child"
        })
        .to_string(),
    );
    let _ = converter
        .process_entity_metadata_message(&reg_message)
        .await
        .unwrap();

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/immediate_child//",
            "@id":"nested_child"
        })
        .to_string(),
    );
    let _ = converter
        .process_entity_metadata_message(&reg_message)
        .await
        .unwrap();

    let in_topic = "te/device/nested_child///m/";
    let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
    let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

    let expected_c8y_json_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/measurement/measurements/create"),
        json!({
            "externalSource":{"externalId":"nested_child","type":"c8y_Serial"},
            "temp":{"temp":{"value":1.0}},
            "time":"2021-11-16T17:45:40.571760714+01:00",
            "type":"ThinEdgeMeasurement"
        })
        .to_string(),
    );

    // Test the first output messages contains SmartREST and C8Y JSON.
    let out_first_messages = converter.convert(&in_message).await;
    assert_eq!(out_first_messages, vec![expected_c8y_json_message.clone()]);
}

#[tokio::test]
async fn convert_measurement_with_nested_child_service() {
    let tmp_dir = TempTedgeDir::new();
    let mut converter = MeaConverter::<MeasurementConverter>::new(&tmp_dir);
    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/immediate_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/main//",
            "@id":"immediate_child"
        })
        .to_string(),
    );
    let _ = converter
        .process_entity_metadata_message(&reg_message)
        .await
        .unwrap();

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/immediate_child//",
            "@id":"nested_child"
        })
        .to_string(),
    );
    let _ = converter
        .process_entity_metadata_message(&reg_message)
        .await
        .unwrap();

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child/service/nested_service"),
        json!({
            "@type":"service",
            "@parent":"device/nested_child//",
            "@id":"nested_service"
        })
        .to_string(),
    );
    let _ = converter
        .process_entity_metadata_message(&reg_message)
        .await
        .unwrap();

    let in_topic = "te/device/nested_child/service/nested_service/m/";
    let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
    let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

    let expected_c8y_json_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/measurement/measurements/create"),
        json!({
            "externalSource":{"externalId":"nested_service","type":"c8y_Serial"},
            "temp":{"temp":{"value":1.0}},
            "time":"2021-11-16T17:45:40.571760714+01:00",
            "type":"ThinEdgeMeasurement"
        })
        .to_string(),
    );

    // Test the first output messages contains SmartREST and C8Y JSON.
    let out_first_messages = converter.convert(&in_message).await;
    assert_eq!(out_first_messages, vec![expected_c8y_json_message.clone()]);
}

#[tokio::test]
async fn convert_measurement_for_child_device_service() {
    let tmp_dir = TempTedgeDir::new();
    let mut converter = MeaConverter::<MeasurementConverter>::new(&tmp_dir);

    let in_topic = "te/device/child1/service/app1/m/m_type";
    let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
    let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

    converter.register_source_entities(in_topic).await;

    let expected_c8y_json_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/measurement/measurements/create"),
        json!({
                "externalSource":{
                    "externalId":"test-device:device:child1:service:app1",
                    "type":"c8y_Serial"
                },
                "temp":{"temp":{"value":1.0}},
                "time":"2021-11-16T17:45:40.571760714+01:00",
                "type":"m_type"})
        .to_string(),
    );

    let out_first_messages = converter.convert(&in_message).await;
    assert_eq!(out_first_messages, vec![expected_c8y_json_message.clone(),]);
}

#[tokio::test]
async fn convert_measurement_for_main_device_service() {
    let tmp_dir = TempTedgeDir::new();
    let mut converter = MeaConverter::<MeasurementConverter>::new(&tmp_dir);

    let in_topic = "te/device/main/service/appm/m/m_type";
    let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
    let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

    converter.register_source_entities(in_topic).await;

    let expected_c8y_json_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/measurement/measurements/create"),
        json!({
                "externalSource":{
                    "externalId":"test-device:device:main:service:appm",
                    "type":"c8y_Serial"
                },
                "temp":{"temp":{"value":1.0}},
                "time":"2021-11-16T17:45:40.571760714+01:00",
                "type":"m_type"})
        .to_string(),
    );

    let out_first_messages = converter.convert(&in_message).await;
    assert_eq!(out_first_messages, vec![expected_c8y_json_message.clone(),]);
}

#[tokio::test]
#[ignore = "FIXME: the registration is currently done even if the message is ill-formed"]
async fn convert_first_measurement_invalid_then_valid_with_child_id() {
    let tmp_dir = TempTedgeDir::new();
    let mut converter = MeaConverter::<MeasurementConverter>::new(&tmp_dir);

    let in_topic = "te/device/child1///m/";
    let in_invalid_payload = r#"{"temp": invalid}"#;
    let in_valid_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
    let in_first_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_invalid_payload);
    let in_second_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_valid_payload);

    // First convert invalid Thin Edge JSON message.
    let out_first_messages = converter.convert(&in_first_message).await;
    let expected_error_message = MqttMessage::new(
        &Topic::new_unchecked("te/errors"),
        "Invalid JSON: expected value at line 1 column 10: `invalid}\n`",
    );
    assert_eq!(out_first_messages, vec![expected_error_message]);

    // Second convert valid Thin Edge JSON message.
    let out_second_messages: Vec<_> = converter
        .convert(&in_second_message)
        .await
        .into_iter()
        .filter(|m| m.topic.name.starts_with("c8y"))
        .collect();
    let expected_smart_rest_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/s/us"),
        "101,test-device:device:child1,child1,thin-edge.io-child,false",
    );
    let expected_c8y_json_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/measurement/measurements/create"),
        r#"{"externalSource":{"externalId":"test-device:device:child1","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"ThinEdgeMeasurement"}"#,
    );
    assert_eq!(
        out_second_messages,
        vec![expected_smart_rest_message, expected_c8y_json_message]
    );
}

#[tokio::test]
async fn convert_two_measurement_messages_given_different_child_id() {
    let tmp_dir = TempTedgeDir::new();
    let mut converter = MeaConverter::<MeasurementConverter>::new(&tmp_dir);
    let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;

    // First message from "child1"
    let in_first_message =
        MqttMessage::new(&Topic::new_unchecked("te/device/child1///m/"), in_payload);
    converter
        .register_source_entities(&in_first_message.topic.name)
        .await;

    let out_first_messages: Vec<_> = converter
        .convert(&in_first_message)
        .await
        .into_iter()
        .filter(|m| m.topic.name.starts_with("c8y"))
        .collect();
    let expected_first_c8y_json_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/measurement/measurements/create"),
        r#"{"externalSource":{"externalId":"test-device:device:child1","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"ThinEdgeMeasurement"}"#,
    );
    assert_eq!(out_first_messages, vec![expected_first_c8y_json_message,]);

    // Second message from "child2"
    let in_second_message =
        MqttMessage::new(&Topic::new_unchecked("te/device/child2///m/"), in_payload);
    converter
        .register_source_entities(&in_second_message.topic.name)
        .await;

    let out_second_messages: Vec<_> = converter
        .convert(&in_second_message)
        .await
        .into_iter()
        .filter(|m| m.topic.name.starts_with("c8y"))
        .collect();
    let expected_second_c8y_json_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/measurement/measurements/create"),
        r#"{"externalSource":{"externalId":"test-device:device:child2","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"ThinEdgeMeasurement"}"#,
    );
    assert_eq!(out_second_messages, vec![expected_second_c8y_json_message,]);
}

#[rstest]
#[case::main_device_with_type_from_topic(
    "te/device/main///m/test_type",
    r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#,
    r#"{"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"test_type"}"#
)]
#[case::main_device_with_type_from_payload(
    "te/device/main///m/test_type",
    r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00","type":"type_in_payload"}"#,
    r#"{"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"type_in_payload"}"#
)]
#[case::child_device_with_type_from_topic(
    "te/device/child///m/test_type",
    r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#,
    r#"{"externalSource":{"externalId":"test-device:device:child","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"test_type"}"#
)]
#[case::child_device_with_type_from_payload(
    "te/device/child2///m/test_type",
    r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00","type":"type_in_payload"}"#,
    r#"{"externalSource":{"externalId":"test-device:device:child2","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00","type":"type_in_payload"}"#
)]
#[tokio::test]
async fn convert_measurement_with_measurement_type(
    #[case] in_topic: &str,
    #[case] in_payload: &str,
    #[case] expected_payload: &str,
) {
    let tmp_dir = TempTedgeDir::new();
    let mut converter = MeaConverter::<MeasurementConverter>::new(&tmp_dir);
    let in_message = MqttMessage::new(&Topic::new_unchecked(in_topic), in_payload);

    converter.register_source_entities(in_topic).await;

    let expected_c8y_json_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/measurement/measurements/create"),
        expected_payload,
    );
    let out_messages: Vec<_> = converter
        .convert(&in_message)
        .await
        .into_iter()
        .filter(|m| m.topic.name.starts_with("c8y"))
        .collect();
    assert_eq!(out_messages, vec![expected_c8y_json_message]);
}

#[rstest]
#[case::default_type_without_a_type(
    "te/device/main///e/",
    r#"{ "text": "Someone clicked", "time": "2020-02-02T01:02:03+05:30" }"#,
    r#"400,ThinEdgeEvent,"Someone clicked",2020-02-02T01:02:03+05:30"#
)]
#[case::type_from_payload(
    "te/device/main///e/topic_event",
    r#"{ "type": "payload event", "text": "Someone clicked", "time": "2020-02-02T01:02:03+05:30" }"#,
    r#"400,payload event,"Someone clicked",2020-02-02T01:02:03+05:30"#
)]
#[case::type_from_topic(
    "te/device/main///e/click_event",
    r#"{ "text": "Someone clicked", "time": "2020-02-02T01:02:03+05:30" }"#,
    r#"400,click_event,"Someone clicked",2020-02-02T01:02:03+05:30"#
)]
#[tokio::test]
async fn convert_event_with_known_fields_to_c8y_smartrest(
    #[case] event_topic: &str,
    #[case] event_payload: &str,
    #[case] expected_payload: &str,
) {
    let tmp_dir = TempTedgeDir::new();
    let mut converter = MeaConverter::<EventConverter>::new(&tmp_dir);
    let event_message = MqttMessage::new(&Topic::new_unchecked(event_topic), event_payload);

    let converted_events = converter.convert(&event_message).await;
    assert_eq!(converted_events.len(), 1);
    let converted_event = converted_events.get(0).unwrap();
    assert_eq!(converted_event.topic.name, "c8y/s/us");
    assert_eq!(converted_event.payload_str().unwrap(), expected_payload);
}

#[rstest]
#[case::type_from_payload(
    "te/device/main///e/click_event",
    r#"{ "type": "payload event", "text": "tick", "foo": "bar" }"#,
    json!({"type": "payload event", "text": "tick", "foo": "bar"})
)]
#[case::type_from_topic(
    "te/device/main///e/click_event",
    r#"{ "text": "tick", "foo": "bar" }"#,
    json!({"type": "click_event", "text": "tick", "foo": "bar"})
)]
#[tokio::test]
async fn convert_event_with_extra_fields_to_c8y_json(
    #[case] event_topic: &str,
    #[case] event_payload: &str,
    #[case] expected: serde_json::Value,
) {
    let tmp_dir = TempTedgeDir::new();
    let mut converter = MeaConverter::<EventConverter>::new(&tmp_dir);
    let event_message = MqttMessage::new(&Topic::new_unchecked(event_topic), event_payload);

    let converted_events = converter.convert(&event_message).await;
    assert_eq!(converted_events.len(), 1);
    let converted_event = converted_events.get(0).unwrap();
    assert_eq!(converted_event.topic.name, "c8y/event/events/create");
    assert_json_include!(
        actual: serde_json::from_str::<serde_json::Value>(converted_event.payload_str().unwrap()).unwrap(),
        expected: expected
    );
}

#[tokio::test]
async fn convert_event_with_custom_c8y_topic_prefix() {
    let tmp_dir = TempTedgeDir::new();
    let mut converter = MeaConverter::<EventConverter>::new(&tmp_dir);
    converter
        .set_config(json!({ "c8y_prefix": "custom-topic"}))
        .unwrap();

    let event_topic = "te/device/main///e/click_event";
    let event_payload = r#"{ "text": "Someone clicked", "time": "2020-02-02T01:02:03+05:30" }"#;
    let event_message = MqttMessage::new(&Topic::new_unchecked(event_topic), event_payload);

    let converted_events = converter.convert(&event_message).await;
    assert_eq!(converted_events.len(), 1);
    let converted_event = converted_events.get(0).unwrap();
    assert_eq!(converted_event.topic.name, "custom-topic/s/us");

    assert_eq!(
        converted_event.payload_str().unwrap(),
        r#"400,click_event,"Someone clicked",2020-02-02T01:02:03+05:30"#
    );
}

#[tokio::test]
async fn test_convert_big_event() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, http_proxy) = create_c8y_converter(&tmp_dir);
    spawn_dummy_c8y_http_proxy(http_proxy);

    let event_topic = "te/device/main///e/click_event";
    let big_event_text = create_packet((16 + 1) * 1024); // Event payload > size_threshold
    let big_event_payload = json!({ "text": big_event_text }).to_string();
    let big_event_message = MqttMessage::new(&Topic::new_unchecked(event_topic), big_event_payload);

    assert!(converter.convert(&big_event_message).await.is_empty());
}

#[tokio::test]
async fn test_convert_big_event_for_child_device() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, http_proxy) = create_c8y_converter(&tmp_dir);
    spawn_dummy_c8y_http_proxy(http_proxy);

    let event_topic = "te/device/child1///e/click_event";
    let big_event_text = create_packet((16 + 1) * 1024); // Event payload > size_threshold
    let big_event_payload = json!({ "text": big_event_text }).to_string();
    let big_event_message = MqttMessage::new(&Topic::new_unchecked(event_topic), big_event_payload);

    let child_registration_messages = converter.convert(&big_event_message).await;

    for message in child_registration_messages {
        // Event creation message should be handled via HTTP
        assert_ne!(message.topic.name, "c8y/event/events/create")
    }
}

#[tokio::test]
async fn test_convert_big_measurement() {
    let tmp_dir = TempTedgeDir::new();
    let mut converter = MeaConverter::<MeasurementConverter>::new(&tmp_dir);
    let measurement_topic = "te/device/main///m/";
    let big_measurement_payload = create_thin_edge_measurement(10 * 1024); // Measurement payload > size_threshold after converting to c8y json

    let big_measurement_message = MqttMessage::new(
        &Topic::new_unchecked(measurement_topic),
        big_measurement_payload,
    );
    let result = converter.convert(&big_measurement_message).await;

    let payload = result[0].payload_str().unwrap();
    assert!(payload.contains(r#""temperature0":{"temperature0":{"value":0.0}}"#));
}

#[tokio::test]
async fn test_convert_small_measurement() {
    let tmp_dir = TempTedgeDir::new();
    let mut converter = MeaConverter::<MeasurementConverter>::new(&tmp_dir);
    let measurement_topic = "te/device/main///m/";
    let big_measurement_payload = create_thin_edge_measurement(20); // Measurement payload size is 20 bytes

    let big_measurement_message = MqttMessage::new(
        &Topic::new_unchecked(measurement_topic),
        big_measurement_payload,
    );

    let result = converter.convert(&big_measurement_message).await;

    assert!(result[0]
        .payload_str()
        .unwrap()
        .contains(r#"{"temperature0":{"temperature0":{"value":0.0}}"#));
    assert!(result[0]
        .payload_str()
        .unwrap()
        .contains(r#""type":"ThinEdgeMeasurement""#));
}

#[tokio::test]
async fn test_convert_big_measurement_for_child_device() {
    let tmp_dir = TempTedgeDir::new();
    let mut converter = MeaConverter::<MeasurementConverter>::new(&tmp_dir);
    let measurement_topic = "te/device/child1///m/";
    let big_measurement_payload = create_thin_edge_measurement(10 * 1024); // Measurement payload > size_threshold after converting to c8y json

    let big_measurement_message = MqttMessage::new(
        &Topic::new_unchecked(measurement_topic),
        big_measurement_payload,
    );

    converter.register_source_entities(measurement_topic).await;

    let result = converter.convert(&big_measurement_message).await;

    // Skipping the first two auto-registration messages and validating the third mapped message
    let payload = result[0].payload_str().unwrap();
    assert!(payload.contains(r#""temperature0":{"temperature0":{"value":0.0}}"#));
}

fn create_packet(size: usize) -> String {
    let data: String = "Some data!".into();
    let loops = size / data.len();
    let mut buffer = String::with_capacity(size);
    for _ in 0..loops {
        buffer.push_str("Some data!");
    }
    buffer
}

fn create_thin_edge_measurement(size: usize) -> String {
    let mut map = serde_json::Map::new();
    let data = r#""temperature":25"#;
    let loops = size / data.len();
    for i in 0..loops {
        map.insert(format!("temperature{i}"), json!(i));
    }
    let obj = serde_json::Value::Object(map);
    serde_json::to_string(&obj).unwrap()
}
