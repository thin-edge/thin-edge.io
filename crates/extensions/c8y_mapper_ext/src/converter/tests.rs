use super::*;
use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::actor::IdUploadRequest;
use crate::actor::IdUploadResult;
use crate::config::BridgeConfig;
use crate::config::C8yMapperConfig;
use crate::entity_cache::InvalidExternalIdError;
use crate::mea::alarms::AlarmConverter;
use crate::mea::health::HealthStatusConverter;
use crate::mea::measurements::MeasurementConverter;
use crate::supported_operations::operation::Operation;
use crate::supported_operations::SupportedOperations;
use crate::tests::spawn_dummy_c8y_http_proxy;
use crate::Capabilities;
use anyhow::Result;
use assert_json_diff::assert_json_include;
use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
use c8y_api::proxy_url::Protocol;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::handle::C8YHttpProxy;
use rstest::rstest;
use serde_json::json;
use serde_json::Value;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use std::time::SystemTime;
use tedge_actors::test_helpers::FakeServerBox;
use tedge_actors::test_helpers::FakeServerBoxBuilder;
use tedge_actors::Builder;
use tedge_actors::ClientMessageBox;
use tedge_actors::CloneSender;
use tedge_actors::LoggingSender;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::http_url::FileTransferUrls;
use tedge_api::mqtt_topics::default_topic_schema;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::ChannelFilter;
use tedge_api::mqtt_topics::EntityFilter;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::pending_entity_store::RegisteredEntityData;
use tedge_api::script::ShellScript;
use tedge_api::workflow::log::log_dir::OperationLogs;
use tedge_api::CommandStatus;
use tedge_api::SoftwareListCommand;
use tedge_api::SoftwareUpdateCommand;
use tedge_config::models::AutoLogUpload;
use tedge_config::models::SoftwareManagementApiFlag;
use tedge_config::models::TopicPrefix;
use tedge_config::TEdgeConfig;
use tedge_flows::ConfigError;
use tedge_flows::Message;
use tedge_flows::Transformer;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpResult;
use tedge_mqtt_ext::test_helpers::assert_messages_matching;
use tedge_mqtt_ext::test_helpers::assert_messages_present;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::QoS;
use tedge_mqtt_ext::Topic;
use tedge_test_utils::fs::TempTedgeDir;
use tedge_utils::paths::TedgePaths;
use test_case::test_case;
use tokio::time::Instant;

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

#[tokio::test]
async fn convert_child_device_registration() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    let in_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1//"),
        json!({
            "@type":"child-device",
            "name":"child1"
        })
        .to_string(),
    );
    let UpdateOutcome::Inserted(entities) = converter
        .process_entity_metadata_message(&in_message)
        .await
        .unwrap()
    else {
        panic!("Expected insert outcome");
    };

    assert_eq!(entities.len(), 1);

    let messages = converter.convert(&in_message).await;

    assert_messages_present(
        &messages,
        [(
            "c8y/s/us",
            "101,test-device:device:child1,child1,thin-edge.io-child,false".into(),
        )],
    );
}

#[tokio::test]
async fn convert_child_device_registration_control_is_device_fragment() {
    let tmp_dir = TempTedgeDir::new();
    let mut config = c8y_converter_config(&tmp_dir);
    config.smartrest_child_device_create_with_device_marker = true;
    let (mut converter, _) = create_c8y_converter_from_config(config);

    let in_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1//"),
        json!({
            "@type":"child-device",
            "name":"child1"
        })
        .to_string(),
    );
    let UpdateOutcome::Inserted(entities) = converter
        .process_entity_metadata_message(&in_message)
        .await
        .unwrap()
    else {
        panic!("Expected insert outcome");
    };

    assert_eq!(entities.len(), 1);

    let messages = converter.convert(&in_message).await;

    assert_messages_present(
        &messages,
        [(
            "c8y/s/us",
            "101,test-device:device:child1,child1,thin-edge.io-child,true".into(),
        )],
    );
}

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

#[tokio::test]
async fn test_execute_operation_is_not_blocked() {
    let tmp_dir = TempTedgeDir::new();
    let (converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    let operation = |name: &str, timeout: u64| {
        let mut operation = toml::from_str::<Operation>(&format!(
            "[exec]\ncommand = \"sleep 5\"\ntimeout = {timeout}\nforceful_timeout = 1"
        ))
        .unwrap();
        operation.name = name.to_owned();
        operation
    };

    let now = Instant::now();
    converter
        .execute_operation(
            ShellScript::from_str("sleep 5").unwrap(),
            &operation("sleep_ten", 10),
            None,
        )
        .await
        .unwrap();
    converter
        .execute_operation(
            ShellScript::from_str("sleep 5").unwrap(),
            &operation("sleep_twenty", 20),
            None,
        )
        .await
        .unwrap();

    // a result between now and elapsed that is not 0 probably means that the operations are
    // blocking and that you probably removed a tokio::spawn handle (;
    assert_eq!(now.elapsed().as_secs(), 0);
}

#[tokio::test]
async fn operations_are_deduplicated() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    let operation = MqttMessage::new(&Topic::new_unchecked("c8y/devicecontrol/notifications"), json!(
            {"id":"16574089","status":"PENDING","c8y_Restart":{},"description":"do something","externalSource":{"externalId":"test-device","type":"c8y_Serial"}}
        ).to_string());
    assert_eq!(
        converter.try_convert(&operation).await.unwrap(),
        vec![MqttMessage {
            topic: Topic {
                name: "te/device/main///cmd/restart/c8y-mapper-16574089".into(),
            },
            payload: json!({"status":"init"}).to_string().into(),
            qos: QoS::AtLeastOnce,
            retain: true,
        },]
    );
    assert_eq!(converter.try_convert(&operation).await.unwrap(), vec![]);
}

#[tokio::test]
async fn operations_are_deduplicated_after_completion() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    let operation = MqttMessage::new(&Topic::new_unchecked("c8y/devicecontrol/notifications"), json!(
            {"id":"16574089","status":"PENDING","c8y_Restart":{},"description":"do something","externalSource":{"externalId":"test-device","type":"c8y_Serial"}}
        ).to_string());
    assert_eq!(
        converter.try_convert(&operation).await.unwrap(),
        vec![MqttMessage {
            topic: Topic {
                name: "te/device/main///cmd/restart/c8y-mapper-16574089".into(),
            },
            payload: json!({"status":"init"}).to_string().into(),
            qos: QoS::AtLeastOnce,
            retain: true,
        },]
    );
    let local_completion = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/restart/c8y-mapper-16574089"),
        "",
    );
    assert_eq!(
        converter.try_convert(&local_completion).await.unwrap(),
        vec![]
    );

    assert_eq!(converter.try_convert(&operation).await.unwrap(), vec![]);
}

#[tokio::test]
async fn a_repeated_operation_does_not_hide_the_other_operations_of_its_message() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
    let repeated = json!({
        "id": "16574089",
        "status": "PENDING",
        "c8y_Restart": {},
        "externalSource": {"externalId": "test-device", "type": "c8y_Serial"}
    });
    let new = json!({
        "id": "16574090",
        "status": "PENDING",
        "c8y_Restart": {},
        "externalSource": {"externalId": "test-device", "type": "c8y_Serial"}
    });
    let topic = Topic::new_unchecked("c8y/devicecontrol/notifications");
    converter
        .try_convert(&MqttMessage::new(&topic, repeated.to_string()))
        .await
        .unwrap();

    let both = MqttMessage::new(&topic, format!("{repeated}\n{new}"));

    assert_eq!(
        converter.try_convert(&both).await.unwrap(),
        vec![MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/restart/c8y-mapper-16574090"),
            json!({"status":"init"}).to_string(),
        )
        .with_qos(QoS::AtLeastOnce)
        .with_retain()]
    );
}

#[tokio::test]
async fn custom_operations_are_deduplicated() {
    let tmp_dir = TempTedgeDir::new();
    let custom_op = r#"exec.topic = "my/custom/topic"
        exec.on_fragment = "my_op"
        exec.command = "/etc/tedge/operations/command ${.payload.my_op.text}""#;
    let f = tmp_dir.dir("operations").dir("c8y").file("my_op");
    f.with_raw_content(custom_op);
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    let operation = MqttMessage::new(&Topic::new_unchecked("my/custom/topic"), json!(
            {"id":"16574089","status":"PENDING","my_op":{},"description":"do something","externalSource":{"externalId":"test-device","type":"c8y_Serial"}}
        ).to_string());
    assert!(
        matches!(
            dbg!(converter.try_convert(&operation).await.unwrap().as_slice()),
            [MqttMessage { topic, .. }, ..] if topic.to_string() == "c8y/s/us"
        ),
        "Initial operation delivery produces outgoing message"
    );
    assert_eq!(
        converter.try_convert(&operation).await.unwrap(),
        vec![],
        "Operation redelivery is ignored by converter"
    );
}

#[tokio::test]
async fn custom_operations_are_not_deduplicated_before_registration() {
    // We could potentially receive a custom operation before we have
    // registered a handler for it. If we then register a suitable handler
    // and the operation is redelivered to the mapper, this should be
    // processed
    let tmp_dir = TempTedgeDir::new();
    let custom_op = r#"exec.topic = "my/custom/topic"
        exec.on_fragment = "my_op"
        exec.command = "/etc/tedge/operations/command ${.payload.my_op.text}""#;
    let f = tmp_dir.dir("operations").dir("c8y").file("my_op");
    f.with_raw_content(custom_op);
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
    let before_registration = SupportedOperations {
        device_id: converter.supported_operations.device_id.clone(),
        base_ops_dir: converter.supported_operations.base_ops_dir.clone(),
        operations_by_xid: <_>::default(),
    };
    let after_registration =
        std::mem::replace(&mut converter.supported_operations, before_registration);

    let operation = MqttMessage::new(&Topic::new_unchecked("my/custom/topic"), json!(
            {"id":"16574089","status":"PENDING","my_op":{},"description":"do something","externalSource":{"externalId":"test-device","type":"c8y_Serial"}}
        ).to_string());
    assert_eq!(
        converter.try_convert(&operation).await.unwrap(),
        vec![],
        "Operation is ignored before the operation is registered"
    );

    converter.supported_operations = after_registration;

    assert!(
        matches!(
            dbg!(converter.try_convert(&operation).await.unwrap().as_slice()),
            [MqttMessage { topic, .. }, ..] if topic.to_string() == "c8y/s/us"
        ),
        "First delivery after registration produces outgoing message"
    );
}

#[tokio::test]
async fn te_topic_operations_do_not_have_time_based_expiry() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    tokio::time::pause();

    let operation = MqttMessage::new(&Topic::new_unchecked("c8y/devicecontrol/notifications"), json!(
            {"id":"16574089","status":"PENDING","c8y_Restart":{},"description":"do something","externalSource":{"externalId":"test-device","type":"c8y_Serial"}}
        ).to_string());
    let expected_message = MqttMessage {
        topic: Topic {
            name: "te/device/main///cmd/restart/c8y-mapper-16574089".into(),
        },
        payload: json!({"status":"init"}).to_string().into(),
        qos: QoS::AtLeastOnce,
        retain: true,
    };

    assert_eq!(
        converter.try_convert(&operation).await.unwrap().first(),
        Some(&expected_message),
        "First delivery after registration produces outgoing message"
    );
    assert_eq!(converter.commands.active_count(), 1);

    // Converter should disable time-based expiry when the `te` topic
    // message is processed
    converter.try_convert(&expected_message).await.unwrap();

    tokio::time::advance(Duration::from_secs(24 * 3600)).await;

    let random_message = MqttMessage::new(&Topic::new_unchecked("c8y/s/ds"), "510,test");

    // Trigger the converter since it performs cache eviction only when it's converting c8y messages
    converter.try_convert(&random_message).await.unwrap();
    assert_eq!(converter.commands.active_count(), 1);
}

#[tokio::test]
async fn custom_operation_ids_are_not_cached_indefinitely() {
    let tmp_dir = TempTedgeDir::new();
    let custom_op = r#"exec.topic = "my/custom/topic"
        exec.on_fragment = "my_op"
        exec.command = "/etc/tedge/operations/command ${.payload.my_op.text}""#;
    let f = tmp_dir.dir("operations").dir("c8y").file("my_op");
    f.with_raw_content(custom_op);

    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    tokio::time::pause();

    let operation = MqttMessage::new(&Topic::new_unchecked("my/custom/topic"), json!(
            {"id":"16574089","status":"PENDING","my_op":{},"description":"do something","externalSource":{"externalId":"test-device","type":"c8y_Serial"}}
        ).to_string());

    assert!(
        matches!(
            dbg!(converter.try_convert(&operation).await.unwrap().as_slice()),
            [MqttMessage { topic, .. }, ..] if topic.to_string() == "c8y/s/us"
        ),
        "First delivery after registration produces outgoing message"
    );

    assert_eq!(converter.commands.active_count(), 1);
    tokio::time::advance(Duration::from_secs(24 * 3600)).await;

    let random_message = MqttMessage::new(&Topic::new_unchecked("c8y/s/ds"), "510,test");

    // Trigger the converter since it performs cache eviction only when it's converting c8y messages
    converter.try_convert(&random_message).await.unwrap();
    assert_eq!(converter.commands.active_count(), 0);
}

#[tokio::test]
async fn active_commands_is_populated_with_existing_commands_from_retain_messages() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    let existing_pending_operation = MqttMessage {
        topic: Topic {
            name: "te/device/main///cmd/restart/c8y-mapper-16574089".into(),
        },
        payload: json!({"status":"init"}).to_string().into(),
        qos: QoS::AtLeastOnce,
        retain: true,
    };
    converter
        .try_convert(&existing_pending_operation)
        .await
        .unwrap();

    let operation = MqttMessage::new(&Topic::new_unchecked("c8y/devicecontrol/notifications"), json!(
            {"id":"16574089","status":"PENDING","c8y_Restart":{},"description":"do something","externalSource":{"externalId":"test-device","type":"c8y_Serial"}}
        ).to_string());

    assert_eq!(
        converter.try_convert(&operation).await.unwrap().as_slice(),
        [],
        "Existing tedge operation should trigger de-duplication"
    );
}

#[tokio::test]
async fn software_list_is_requested_when_software_update_capability_is_received() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
    let mqtt_schema = converter.mqtt_schema.clone();
    let device = EntityTopicId::default_main_device();

    let capability = SoftwareUpdateCommand::capability_message(&mqtt_schema, &device);
    converter.try_convert(&capability).await.unwrap();
    let messages = converter.take_software_list_requests();

    let requests = software_list_requests(&converter, &device, &messages);
    assert_eq!(
        requests.len(),
        1,
        "Expected a software list request in {messages:?}"
    );
    assert_eq!(
        serde_json::from_slice::<Value>(requests[0].payload_bytes()).unwrap(),
        json!({"status": "init"})
    );
}

#[test_case(CommandStatus::Init)]
#[test_case(CommandStatus::Executing)]
#[test_case(CommandStatus::Successful)]
#[tokio::test]
async fn software_list_is_not_requested_while_a_previous_request_is_pending(status: CommandStatus) {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
    let mqtt_schema = converter.mqtt_schema.clone();
    let device = EntityTopicId::default_main_device();

    // A software list command issued by this mapper, e.g. before being restarted,
    // received as a retained message from the broker and not cleared yet
    let previous_request = SoftwareListCommand::new(&device, "c8y-mapper-1234".into())
        .with_status(status)
        .command_message(&mqtt_schema);
    converter.try_convert(&previous_request).await.unwrap();

    let capability = SoftwareUpdateCommand::capability_message(&mqtt_schema, &device);
    converter.try_convert(&capability).await.unwrap();
    let messages = converter.take_software_list_requests();

    assert!(
        software_list_requests(&converter, &device, &messages).is_empty(),
        "No software list request expected in {messages:?}"
    );
}

#[tokio::test]
async fn software_list_is_not_requested_when_a_pending_request_is_received_after_the_capability() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
    let mqtt_schema = converter.mqtt_schema.clone();
    let device = EntityTopicId::default_main_device();

    let capability = SoftwareUpdateCommand::capability_message(&mqtt_schema, &device);
    converter.try_convert(&capability).await.unwrap();
    let previous_request =
        SoftwareListCommand::new(&device, "c8y-mapper-1234".into()).command_message(&mqtt_schema);
    converter.try_convert(&previous_request).await.unwrap();
    let messages = converter.take_software_list_requests();

    assert!(
        software_list_requests(&converter, &device, &messages).is_empty(),
        "No software list request expected in {messages:?}"
    );
}

#[tokio::test]
async fn software_list_is_requested_once_when_the_capability_is_received_repeatedly() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
    let mqtt_schema = converter.mqtt_schema.clone();
    let device = EntityTopicId::default_main_device();

    let capability = SoftwareUpdateCommand::capability_message(&mqtt_schema, &device);
    converter.try_convert(&capability).await.unwrap();
    let mut messages = converter.take_software_list_requests();
    converter.try_convert(&capability).await.unwrap();
    messages.extend(converter.take_software_list_requests());

    assert_eq!(
        software_list_requests(&converter, &device, &messages).len(),
        1,
        "Expected a single software list request in {messages:?}"
    );
}

#[tokio::test]
async fn software_list_is_requested_once_the_previous_request_is_cleared() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
    let mqtt_schema = converter.mqtt_schema.clone();
    let device = EntityTopicId::default_main_device();

    let previous_request = SoftwareListCommand::new(&device, "c8y-mapper-1234".into())
        .with_status(CommandStatus::Successful);
    converter
        .try_convert(&previous_request.command_message(&mqtt_schema))
        .await
        .unwrap();
    converter
        .try_convert(&previous_request.clearing_message(&mqtt_schema))
        .await
        .unwrap();

    let capability = SoftwareUpdateCommand::capability_message(&mqtt_schema, &device);
    converter.try_convert(&capability).await.unwrap();
    let messages = converter.take_software_list_requests();

    assert_eq!(
        software_list_requests(&converter, &device, &messages).len(),
        1,
        "Expected a software list request in {messages:?}"
    );
}

#[tokio::test]
async fn software_list_is_requested_when_the_agent_restarts_with_a_request_still_pending() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
    let mqtt_schema = converter.mqtt_schema.clone();
    let device = EntityTopicId::default_main_device();

    let previous_request =
        SoftwareListCommand::new(&device, "c8y-mapper-1234".into()).command_message(&mqtt_schema);
    converter.try_convert(&previous_request).await.unwrap();

    let mut capability = SoftwareUpdateCommand::capability_message(&mqtt_schema, &device);
    capability.retain = false;
    let mut messages = converter.try_convert(&capability).await.unwrap();
    messages.extend(converter.take_software_list_requests());

    assert_eq!(
        software_list_requests(&converter, &device, &messages).len(),
        1,
        "Expected a software list request in {messages:?}"
    );
}

#[tokio::test]
async fn software_list_is_requested_when_the_agent_restarts_and_resumes_a_cleared_request() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
    let mqtt_schema = converter.mqtt_schema.clone();
    let device = EntityTopicId::default_main_device();

    let previous_request = SoftwareListCommand::new(&device, "c8y-mapper-1234".into())
        .with_status(CommandStatus::Successful);
    converter
        .try_convert(&previous_request.command_message(&mqtt_schema))
        .await
        .unwrap();
    converter
        .try_convert(&previous_request.clearing_message(&mqtt_schema))
        .await
        .unwrap();
    // Republished by an agent restarted before having seen the clearing message
    converter
        .try_convert(&previous_request.command_message(&mqtt_schema))
        .await
        .unwrap();

    let mut capability = SoftwareUpdateCommand::capability_message(&mqtt_schema, &device);
    capability.retain = false;
    let mut messages = converter.try_convert(&capability).await.unwrap();
    messages.extend(converter.take_software_list_requests());

    assert_eq!(
        software_list_requests(&converter, &device, &messages).len(),
        1,
        "Expected a software list request in {messages:?}"
    );
}

#[tokio::test]
async fn software_list_request_not_issued_by_the_mapper_does_not_prevent_a_new_request() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
    let mqtt_schema = converter.mqtt_schema.clone();
    let device = EntityTopicId::default_main_device();

    // A software list command issued locally, e.g. using `tedge mqtt pub`
    let local_request =
        SoftwareListCommand::new(&device, "local-1111".into()).command_message(&mqtt_schema);
    converter.try_convert(&local_request).await.unwrap();

    let capability = SoftwareUpdateCommand::capability_message(&mqtt_schema, &device);
    converter.try_convert(&capability).await.unwrap();
    let messages = converter.take_software_list_requests();

    assert_eq!(
        software_list_requests(&converter, &device, &messages).len(),
        1,
        "Expected a software list request in {messages:?}"
    );
}

#[tokio::test]
async fn software_list_request_pending_for_another_device_does_not_prevent_a_new_request() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
    let mqtt_schema = converter.mqtt_schema.clone();
    let device = EntityTopicId::default_main_device();
    let child = EntityTopicId::default_child_device("child1").unwrap();

    let child_request =
        SoftwareListCommand::new(&child, "c8y-mapper-1234".into()).command_message(&mqtt_schema);
    register_source_entities(&child_request.topic.name, &mut converter).await;
    converter.try_convert(&child_request).await.unwrap();

    let capability = SoftwareUpdateCommand::capability_message(&mqtt_schema, &device);
    converter.try_convert(&capability).await.unwrap();
    let messages = converter.take_software_list_requests();

    assert_eq!(
        software_list_requests(&converter, &device, &messages).len(),
        1,
        "Expected a software list request in {messages:?}"
    );
}

fn software_list_requests<'a>(
    converter: &CumulocityConverter,
    target: &EntityTopicId,
    messages: &'a [MqttMessage],
) -> Vec<&'a MqttMessage> {
    let software_list_topics = converter.mqtt_schema.topics(
        EntityFilter::Entity(target),
        ChannelFilter::Command(OperationType::SoftwareList),
    );
    messages
        .iter()
        .filter(|message| software_list_topics.accept(message))
        .collect()
}

#[tokio::test]
async fn custom_operation_ids_are_not_evicted_from_cache_prematurely() {
    let tmp_dir = TempTedgeDir::new();
    let custom_op = r#"exec.topic = "my/custom/topic"
        exec.on_fragment = "my_op"
        exec.command = "/etc/tedge/operations/command ${.payload.my_op.text}""#;
    let f = tmp_dir.dir("operations").dir("c8y").file("my_op");
    f.with_raw_content(custom_op);

    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    tokio::time::pause();

    let operation = MqttMessage::new(&Topic::new_unchecked("my/custom/topic"), json!(
            {"id":"16574089","status":"PENDING","my_op":{},"description":"do something","externalSource":{"externalId":"test-device","type":"c8y_Serial"}}
        ).to_string());

    assert!(
        matches!(
            dbg!(converter.try_convert(&operation).await.unwrap().as_slice()),
            [MqttMessage { topic, .. }, ..] if topic.to_string() == "c8y/s/us"
        ),
        "First delivery after registration produces outgoing message"
    );

    assert_eq!(converter.commands.active_count(), 1);
    // After a minute, the operation id should still exist
    tokio::time::advance(Duration::from_secs(60)).await;

    let random_message = MqttMessage::new(&Topic::new_unchecked("c8y/s/ds"), "510,test");
    converter.try_convert(&random_message).await.unwrap();
    assert_eq!(converter.commands.active_count(), 1);
}

#[tokio::test]
async fn smartrest_message_on_builtin_topic_is_not_an_unsupported_topic() {
    let (capture, _guard) = tedge_test_utils::tracing::TracingCapture::default().start();

    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    let message = MqttMessage::new(&Topic::new_unchecked("c8y/s/ds"), "510,test-device");
    converter.try_convert(&message).await.unwrap();

    capture.assert_no_errors();
}

#[tokio::test]
async fn handle_operations_for_child_device() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    // The child has first to declare its capabilities
    let mqtt_schema = MqttSchema::default();
    let child = EntityTopicId::default_child_device("childId").unwrap();
    let child_capability = SoftwareUpdateCommand::capability_message(&mqtt_schema, &child);

    register_source_entities(&child_capability.topic.name, &mut converter).await;

    // A request to a child is forwarded to that child using its registered mapping: external id <=> topic identifier
    let device_cmd_channel = mqtt_schema.topics(
        EntityFilter::Entity(&child),
        ChannelFilter::Command(OperationType::SoftwareUpdate),
    );
    let mqtt_message = MqttMessage::new(
        &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
        json!({
            "id": "123456",
            "c8y_SoftwareUpdate": [
                {
                    "name": "software_a",
                    "action": "install",
                    "version": "version_a",
                    "url": "url_a"
                }
            ],
            "externalSource": {
                "externalId": "test-device:device:childId",
                "type": "c8y_Serial"
            }
        })
        .to_string(),
    );
    let command = converter
        .convert_device_control_operations(&mqtt_message)
        .await
        .unwrap()
        .get(0)
        .unwrap()
        .clone();

    assert!(device_cmd_channel.accept(&command));
    assert_eq!(
        serde_json::from_slice::<Value>(command.payload_bytes()).unwrap(),
        json!({
            "status":"init",
            "updateList":[
                { "type":"default",
                  "modules":[
                    {"name":"software_a","version":"version_a","url":"url_a","action":"install"}
                  ]}
            ]
        })
    );
}

#[tokio::test]
async fn an_invalid_operation_of_a_child_device_is_failed_on_the_smartrest_topic_of_the_child_device(
) {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
    register_source_entities("te/device/childId//", &mut converter).await;

    assert_messages_matching(
            &converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked("c8y/devicecontrol/notifications"),
                    json!({
                        "id": "123456",
                        "c8y_SoftwareUpdate": [
                            {"name": "nodered", "action": "remove", "version": "1.0.0::debian"}
                        ],
                        "externalSource": {"externalId": "test-device:device:childId", "type": "c8y_Serial"},
                    })
                    .to_string(),
                ))
                .await,
            [
                (
                    "c8y/s/us/test-device:device:childId",
                    "501,c8y_SoftwareUpdate".into(),
                ),
                (
                    "c8y/s/us/test-device:device:childId",
                    "502,c8y_SoftwareUpdate,Parameter remove is not recognized. It must be install or delete.".into(),
                ),
            ],
        );
}

#[test_case("device/main//", "test-device")]
#[test_case(
    "device/main/service/tedge-agent",
    "test-device:device:main:service:tedge-agent"
)]
#[test_case("device/child1//", "test-device:device:child1")]
#[test_case(
    "device/child1/service/collectd",
    "test-device:device:child1:service:collectd"
)]
#[test_case("custom_name///", "test-device:custom_name")]
#[tokio::test]
async fn entity_topic_id_to_c8y_external_id_mapping(entity_topic_id: &str, c8y_external_id: &str) {
    let entity_topic_id = EntityTopicId::from_str(entity_topic_id).unwrap();
    assert_eq!(
        CumulocityConverter::map_to_c8y_external_id(&entity_topic_id, &"test-device".into()),
        c8y_external_id.into()
    );
}

#[test_case("bad+name1", '+')]
#[test_case("bad/name2", '/')]
#[test_case("bad#name3", '#')]
#[test_case("my/very#bad+name", '/')]
fn sanitize_c8y_external_id(input_id: &str, invalid_char: char) {
    assert_eq!(
        CumulocityConverter::validate_external_id(input_id),
        Err(InvalidExternalIdError {
            external_id: input_id.into(),
            invalid_char,
        })
    );
}

#[tokio::test]
async fn duplicate_registration_messages_not_mapped_2311() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    let in_topic = "te/device/main/service/my_measurement_service";
    register_source_entities(in_topic, &mut converter).await;

    let reg_message = MqttMessage::new(&Topic::new_unchecked(in_topic), r#"{"@type": "service"}"#);

    // when converting a registration message the same as the previous one, no additional registration messages should be produced
    let outcome = converter
        .process_entity_metadata_message(&reg_message)
        .await
        .unwrap();
    assert_eq!(outcome, UpdateOutcome::Unchanged);
}

#[rstest]
#[case::empty("", "service")]
#[case::set("systemd", "systemd")]
#[tokio::test]
async fn the_health_status_of_a_service_registered_without_type_has_the_default_service_type(
    #[case] service_type: &str,
    #[case] expected_type: &str,
) {
    let tmp_dir = TempTedgeDir::new();
    let mut config = c8y_converter_config(&tmp_dir);
    let tedge_config = TEdgeConfig::load_toml_str(&format!("service.type = \"{service_type}\""));
    config.service = tedge_config.service.clone();

    let mut converter = MeaConverter::<HealthStatusConverter>::from_c8y_config(config);

    let service_health_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main/service/service1/status/health"),
        serde_json::to_string(&json!({"status": "up"})).unwrap(),
    );

    converter
        .register_source_entities(&service_health_message.topic.name)
        .await;

    let output = converter.convert(&service_health_message).await;
    let service_creation_message = output
        .into_iter()
        .find(|m| m.topic == C8yTopic::upstream_topic(&"c8y".try_into().unwrap()))
        .expect("service creation message should be present");

    let mut smartrest_fields = service_creation_message.payload_str().unwrap().split(',');

    assert_eq!(smartrest_fields.next().unwrap(), "102");
    assert_eq!(
        smartrest_fields.next().unwrap(),
        format!(
            "{}:device:main:service:service1",
            converter.c8y_converter.config.device_id
        )
    );
    assert_eq!(smartrest_fields.next().unwrap(), expected_type);
    assert_eq!(smartrest_fields.next().unwrap(), "service1");
    assert_eq!(smartrest_fields.next().unwrap(), "up");
}

#[tokio::test]
async fn the_health_status_of_a_child_device_service_registered_without_type_has_the_type_service()
{
    let tmp_dir = TempTedgeDir::new();
    let mut config = c8y_converter_config(&tmp_dir);
    config.service = TEdgeConfig::load_toml_str("service.type = \"systemd\"")
        .service
        .clone();
    let mut converter = MeaConverter::<HealthStatusConverter>::from_c8y_config(config);

    let service_health_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1/service/service1/status/health"),
        json!({"status": "up"}).to_string(),
    );
    converter
        .register_source_entities(&service_health_message.topic.name)
        .await;

    let output = converter.convert(&service_health_message).await;
    let service_creation_message = output
        .iter()
        .find_map(|m| m.payload_str().ok().filter(|p| p.starts_with("102,")))
        .expect("service creation message should be present");

    let smartrest_fields: Vec<&str> = service_creation_message.split(',').collect();
    assert_eq!(smartrest_fields[2], "service");
}

#[tokio::test]
async fn early_messages_cached_and_processed_only_after_registration() {
    let tmp_dir = TempTedgeDir::new();
    let mut config = c8y_converter_config(&tmp_dir);
    config.bridge_config.c8y_prefix = "custom-c8y-prefix".try_into().unwrap();

    let (mut converter, _http_proxy) = create_c8y_converter_from_config(config);

    // Publish some measurements that are only cached and not converted
    for i in 0..3 {
        let measurement_message = MqttMessage::new(
            &Topic::new_unchecked("te/custom/child1///m/environment"),
            json!({ "temperature": i }).to_string(),
        );
        let mapped_messages = converter.convert(&measurement_message).await;
        assert!(
            mapped_messages.is_empty(),
            "Expected the early telemetry messages to be cached and not mapped"
        )
    }

    // Publish a twin message which is also cached
    let twin_message = MqttMessage::new(
        &Topic::new_unchecked("te/custom/child1///twin/foo"),
        r#"5.6789"#,
    );
    let mapped_messages = converter.convert(&twin_message).await;
    assert!(
        mapped_messages.is_empty(),
        "Expected the early twin messages to be cached and not mapped"
    );

    // Publish the registration message which will trigger the conversion of cached messages as well
    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/custom/child1//"),
        json!({"@type": "child-device", "@id": "child1", "name": "child1"}).to_string(),
    );

    let UpdateOutcome::Inserted(entities) = converter
        .process_entity_metadata_message(&reg_message)
        .await
        .unwrap()
    else {
        panic!("Expected insert outcome");
    };

    let messages = registered_entities_into_mqtt_messages(entities);

    // Assert that the registration message, the twin updates and the cached measurement messages are converted
    assert_messages_matching(
        &messages,
        [
            (
                "te/custom/child1//",
                json!({
                    "@id":"child1",
                    "@type":"child-device",
                    "name":"child1"
                })
                .into(),
            ),
            ("te/custom/child1///twin/foo", "5.6789".into()),
            (
                "te/custom/child1///m/environment",
                json!({ "temperature": 0 }).into(),
            ),
            (
                "te/custom/child1///m/environment",
                json!({ "temperature": 1 }).into(),
            ),
            (
                "te/custom/child1///m/environment",
                json!({ "temperature": 2 }).into(),
            ),
        ],
    );
}

#[tokio::test]
async fn early_child_device_registrations_processed_only_after_parent_registration() {
    let tmp_dir = TempTedgeDir::new();
    let config = c8y_converter_config(&tmp_dir);

    let (mut converter, _http_proxy) = create_c8y_converter_from_config(config);

    // Publish great-grand-child registration before grand-child and child
    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child000//"),
        json!({
            "@type": "child-device",
            "@id": "child000",
            "name": "child000",
            "@parent": "device/child00//",
        })
        .to_string(),
    );

    let outcome = converter
        .process_entity_metadata_message(&reg_message)
        .await
        .unwrap();
    assert_eq!(outcome, UpdateOutcome::Unchanged);

    // Publish grand-child registration before child
    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child00//"),
        json!({
            "@type": "child-device",
            "@id": "child00",
            "name": "child00",
            "@parent": "device/child0//",
        })
        .to_string(),
    );

    let outcome = converter
        .process_entity_metadata_message(&reg_message)
        .await
        .unwrap();
    assert_eq!(outcome, UpdateOutcome::Unchanged);

    // Register the immediate child device which will trigger the conversion of cached messages as well
    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child0//"),
        json!({
            "@type": "child-device",
            "@id": "child0",
            "name": "child0",
            "@parent": "device/main//",
        })
        .to_string(),
    );
    let UpdateOutcome::Inserted(entities) = converter
        .process_entity_metadata_message(&reg_message)
        .await
        .unwrap()
    else {
        panic!("Expected insert outcome");
    };
    let messages = registered_entities_into_mqtt_messages(entities);
    assert_messages_matching(
        &messages,
        [
            (
                "te/device/child0//",
                json!({
                    "@type": "child-device",
                    "@id": "child0",
                    "name": "child0",
                    "@parent": "device/main//",
                })
                .into(),
            ),
            (
                "te/device/child00//",
                json!({
                    "@type": "child-device",
                    "@id": "child00",
                    "name": "child00",
                    "@parent": "device/child0//",
                })
                .into(),
            ),
            (
                "te/device/child000//",
                json!({
                    "@type": "child-device",
                    "@id": "child000",
                    "name": "child000",
                    "@parent": "device/child00//",
                })
                .into(),
            ),
        ],
    );
}

#[tokio::test]
async fn custom_command_capability_is_declared_as_the_c8y_operation_of_its_template() {
    let tmp_dir = TempTedgeDir::new();
    tmp_dir
        .dir("operations")
        .dir("c8y")
        .file("c8y_Operation.template")
        .with_raw_content(
            r#"[exec]
            on_fragment = "c8y_Operation"

            [exec.workflow]
            operation = "my_operation"
            "#,
        );

    let config = c8y_converter_config(&tmp_dir);

    let (mut converter, _http_proxy) = create_c8y_converter_from_config(config);

    // main device command
    let main_device = EntityTopicId::default_main_device();

    let operation_topic = converter.mqtt_schema.topic_for(
        &main_device,
        &Channel::CommandMetadata {
            operation: OperationType::Custom("my_operation".to_string()),
        },
    );
    let operation_msg = MqttMessage::new(&operation_topic, "{}");

    let msgs = converter.convert(&operation_msg).await;
    assert_messages_matching(&msgs, [("c8y/s/us", "114,c8y_Operation".into())]);

    // child device command
    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child0//"),
        json!({
            "@type": "child-device",
            "@id": "child0",
            "name": "child0",
            "@parent": "device/main//",
        })
        .to_string(),
    );

    converter
        .process_entity_metadata_message(&reg_message)
        .await
        .unwrap();

    let child_device = EntityTopicId::default_child_device("child0").unwrap();
    let operation_topic = converter.mqtt_schema.topic_for(
        &child_device,
        &Channel::CommandMetadata {
            operation: OperationType::Custom("my_operation".to_string()),
        },
    );
    let operation_msg = MqttMessage::new(&operation_topic, "{}");

    let msgs = converter.convert(&operation_msg).await;
    assert_messages_matching(&msgs, [("c8y/s/us/child0", "114,c8y_Operation".into())]);
}

fn registered_entities_into_mqtt_messages(entities: Vec<RegisteredEntityData>) -> Vec<MqttMessage> {
    let mut messages = vec![];
    for entity in entities {
        messages.push(entity.reg_message.to_mqtt_message(&MqttSchema::default()));
        for data_message in entity.data_messages {
            messages.push(data_message);
        }
    }
    messages
}

pub(crate) fn create_c8y_converter(
    tmp_dir: &TempTedgeDir,
) -> (CumulocityConverter, FakeServerBox<HttpRequest, HttpResult>) {
    let config = c8y_converter_config(tmp_dir);
    create_c8y_converter_from_config(config)
}

pub(crate) fn c8y_converter_config(tmp_dir: &TempTedgeDir) -> C8yMapperConfig {
    tmp_dir.dir("operations").dir("c8y");
    tmp_dir.dir("tedge").dir("agent");
    tmp_dir.dir(".tedge-mapper-c8y");
    tmp_dir.dir("agent");

    let device_id = "test-device".into();
    let device_topic_id = EntityTopicId::default_main_device();
    let service_topic_id = EntityTopicId::default_main_service("tedge-mapper-c8y").unwrap();
    let tedge_config = TEdgeConfig::load_toml_str("service.ty = \"service\"");
    let c8y_host = "test.c8y.io".to_owned();
    let file_transfer_urls = FileTransferUrls::new("127.0.0.1".into(), Protocol::Http);
    let auth_proxy_addr = "127.0.0.1".into();
    let auth_proxy_port = 8001;
    let auth_proxy_protocol = Protocol::Http;
    let bridge_config = BridgeConfig {
        c8y_prefix: TopicPrefix::try_from("c8y").unwrap(),
    };

    let mut topics =
        C8yMapperConfig::default_internal_topic_filter(&"c8y".try_into().unwrap()).unwrap();
    let custom_operation_topics =
        C8yMapperConfig::get_topics_from_custom_operations(tmp_dir.path(), &bridge_config).unwrap();
    topics.add_all(custom_operation_topics);
    topics.remove_overlapping_patterns();

    let root_dir = TedgePaths::from_root_with_defaults(tmp_dir.path(), "", "");

    C8yMapperConfig::new(
        root_dir.clone().into(),
        Arc::new(OperationLogs::new(root_dir.root_dir())),
        root_dir.clone().into(),
        None,
        device_id,
        device_topic_id,
        service_topic_id,
        tedge_config.service.clone(),
        c8y_host.clone(),
        c8y_host,
        file_transfer_urls,
        topics,
        Capabilities::default(),
        auth_proxy_addr,
        auth_proxy_port,
        auth_proxy_protocol,
        MqttSchema::default(),
        bridge_config,
        false,
        SoftwareManagementApiFlag::Advanced,
        true,
        AutoLogUpload::Never,
        false,
        false,
        16184,
        "3s",
    )
}

pub(crate) fn create_c8y_converter_from_config(
    config: C8yMapperConfig,
) -> (CumulocityConverter, FakeServerBox<HttpRequest, HttpResult>) {
    let mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 5);
    let mqtt_publisher = LoggingSender::new("MQTT".into(), mqtt_builder.build().sender_clone());

    let mut http_builder: FakeServerBoxBuilder<HttpRequest, HttpResult> = FakeServerBox::builder();
    let http_proxy = C8YHttpProxy::new(&config, &mut http_builder);

    let mut uploader_builder: FakeServerBoxBuilder<IdUploadRequest, IdUploadResult> =
        FakeServerBox::builder();
    let uploader = ClientMessageBox::new(&mut uploader_builder);

    let mut downloader_builder: FakeServerBoxBuilder<IdDownloadRequest, IdDownloadResult> =
        FakeServerBox::builder();
    let downloader = ClientMessageBox::new(&mut downloader_builder);

    let mut http_client_builder: FakeServerBoxBuilder<HttpRequest, HttpResult> =
        FakeServerBox::builder();
    let http_client = ClientMessageBox::new(&mut http_client_builder);

    let flow_context = FlowContextHandle::default();
    let converter = CumulocityConverter::new(
        config,
        mqtt_publisher,
        http_proxy,
        uploader,
        downloader,
        http_client,
        flow_context,
    )
    .unwrap();

    (converter, http_builder.build())
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

pub(crate) async fn register_source_entities(topic: &str, converter: &mut CumulocityConverter) {
    let (topic_id, _) = converter.mqtt_schema.entity_channel_of(topic).unwrap();
    let entities = default_topic_schema::parse(&topic_id);

    for entity in entities {
        let message = entity.to_mqtt_message(&converter.mqtt_schema);
        converter
            .process_entity_metadata_message(&message)
            .await
            .unwrap();
    }
}

struct MeaConverter<T> {
    c8y_converter: CumulocityConverter,
    mea_converter: T,
    _http: FakeServerBox<HttpRequest, HttpResult>,
}

impl<T: Default + Transformer> MeaConverter<T> {
    fn new(tmp_dir: &TempTedgeDir) -> Self {
        let (c8y_converter, _http) = create_c8y_converter(tmp_dir);
        MeaConverter {
            c8y_converter,
            mea_converter: Default::default(),
            _http,
        }
    }

    fn from_c8y_config(config: C8yMapperConfig) -> Self {
        let (c8y_converter, _http) = create_c8y_converter_from_config(config);
        MeaConverter {
            c8y_converter,
            mea_converter: Default::default(),
            _http,
        }
    }

    fn set_config(&mut self, config: Value) -> Result<(), ConfigError> {
        self.mea_converter.set_config(config.into())
    }

    async fn register_source_entities(&mut self, topic: &str) {
        register_source_entities(topic, &mut self.c8y_converter).await
    }

    async fn process_entity_metadata_message(
        &mut self,
        message: &MqttMessage,
    ) -> Result<UpdateOutcome, ConversionError> {
        self.c8y_converter
            .process_entity_metadata_message(message)
            .await
    }

    async fn convert(&mut self, message: &MqttMessage) -> Vec<MqttMessage> {
        let context = self.c8y_converter.entity_cache.flow_context();
        let timestamp = SystemTime::now();
        let message: Message = message.clone().into();
        match self.mea_converter.on_message(timestamp, &message, context) {
            Ok(messages) => messages
                .into_iter()
                .filter_map(|msg| MqttMessage::try_from(msg).ok())
                .collect(),
            Err(_) => {
                vec![]
            }
        }
    }

    fn sync_messages(&mut self) -> Vec<MqttMessage> {
        let context = self.c8y_converter.entity_cache.flow_context();
        let timestamp = SystemTime::now();
        match self.mea_converter.on_interval(timestamp, context) {
            Ok(messages) => messages
                .into_iter()
                .filter_map(|msg| MqttMessage::try_from(msg).ok())
                .collect(),
            Err(_) => {
                vec![]
            }
        }
    }
}
