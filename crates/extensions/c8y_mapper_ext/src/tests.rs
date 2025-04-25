use super::actor::C8yMapperBuilder;
use super::actor::SyncComplete;
use super::actor::SyncStart;
use super::config::C8yMapperConfig;
use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::actor::IdUploadRequest;
use crate::actor::IdUploadResult;
use crate::availability::AvailabilityBuilder;
use crate::config::BridgeConfig;
use crate::operations::OperationHandler;
use crate::Capabilities;
use c8y_api::json_c8y::C8yEventResponse;
use c8y_api::json_c8y::InternalIdResponse;
use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
use c8y_api::proxy_url::Protocol;
use c8y_api::smartrest::topic::C8yTopic;
use serde_json::json;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Duration;
use tedge_actors::test_helpers::FakeServerBox;
use tedge_actors::test_helpers::FakeServerBoxBuilder;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSink;
use tedge_actors::NoConfig;
use tedge_actors::NoMessage;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_config::models::AutoLogUpload;
use tedge_config::models::SoftwareManagementApiFlag;
use tedge_config::models::TopicPrefix;
use tedge_config::tedge_toml::C8Y_MQTT_PAYLOAD_LIMIT;
use tedge_config::TEdgeConfig;
use tedge_downloader_ext::DownloadResponse;
use tedge_file_system_ext::FsWatchEvent;
use tedge_http_ext::test_helpers::HttpResponseBuilder;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpResult;
use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
use tedge_mqtt_ext::test_helpers::assert_received_includes_json;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_test_utils::fs::with_exec_permission;
use tedge_test_utils::fs::TempTedgeDir;
use tedge_timer_ext::Timeout;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

#[tokio::test]
async fn mapper_publishes_init_messages_on_startup() {
    // Start SM Mapper
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, .. } = test_handle;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    let version = env!("CARGO_PKG_VERSION");

    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "te/device/main///twin/c8y_Agent",
                json!({
                    "name": "thin-edge.io",
                    "url": "https://thin-edge.io",
                    "version": version
                })
                .to_string()
                .as_str(),
            ),
            ("te/device/main///twin/name", "test-device"),
            ("te/device/main///twin/type", "test-device-type"),
            ("c8y/s/us", "114"),
            ("c8y/s/us", "500"),
        ],
    )
    .await;
}

#[tokio::test]
async fn child_device_registration_mapping() {
    let ttd = TempTedgeDir::new();
    let test_handle =
        spawn_c8y_mapper_actor_with_config(&ttd, test_mapper_config(&ttd), true).await;
    let mut mqtt = test_handle.mqtt.with_timeout(TEST_TIMEOUT_MS);
    let mut timer = test_handle.timer;
    let mut avail = test_handle.avail.with_timeout(TEST_TIMEOUT_MS);

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;
    skip_init_messages(&mut mqtt).await;

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1//"),
        r#"{ "@type": "child-device", "type": "RaspberryPi", "name": "Child1" }"#,
    ))
    .await
    .unwrap();

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "101,test-device:device:child1,Child1,RaspberryPi,false",
        )],
    )
    .await;

    assert_received_contains_str(
        &mut avail,
        [(
            "te/device/child1//",
            r#"{"@id":"test-device:device:child1","@type":"child-device","name":"Child1","type":"RaspberryPi"}"#
        )],
    )
    .await;

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child2//"),
        r#"{ "@type": "child-device", "@parent": "device/child1//" }"#,
    ))
    .await
    .unwrap();

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child1",
            "101,test-device:device:child2,test-device:device:child2,thin-edge.io-child,false",
        )],
    )
    .await;

    assert_received_contains_str(
        &mut avail,
        [(
            "te/device/child2//",
            r#"{"@id":"test-device:device:child2","@parent":"device/child1//","@type":"child-device"}"#
        )],
    )
    .await;

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child3//"),
        r#"{ "@type": "child-device", "@id": "child3", "@parent": "device/child2//" }"#,
    ))
    .await
    .unwrap();

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child2",
            "101,child3,child3,thin-edge.io-child,false",
        )],
    )
    .await;

    assert_received_contains_str(
        &mut avail,
        [(
            "te/device/child3//",
            r#"{"@id":"child3","@parent":"device/child2//","@type":"child-device"}"#,
        )],
    )
    .await;
}

#[tokio::test]
async fn custom_topic_scheme_registration_mapping() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Child device with custom scheme
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/custom///"),
        r#"{ "@type": "child-device", "type": "RaspberryPi", "name": "Child1" }"#,
    ))
    .await
    .unwrap();

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "101,test-device:custom,Child1,RaspberryPi,false",
        )],
    )
    .await;

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/custom/child1//"),
        r#"{ "@type": "child-device", "type": "RaspberryPi", "name": "Child1" }"#,
    ))
    .await
    .unwrap();

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "101,test-device:custom:child1,Child1,RaspberryPi,false",
        )],
    )
    .await;

    // Service with custom scheme
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/custom/service/collectd/"),
        r#"{ "@type": "service", "type": "systemd", "name": "Collectd" }"#,
    ))
    .await
    .unwrap();

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "102,test-device:custom:service:collectd,systemd,Collectd,up",
        )],
    )
    .await;
}

#[tokio::test]
async fn service_registration_mapping() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Create a direct child device: child1 and a nested child device: child2
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1//"),
        r#"{ "@type": "child-device" }"#,
    ))
    .await
    .unwrap();
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child2//"),
        r#"{ "@type": "child-device", "@parent": "device/child1//" }"#,
    ))
    .await
    .unwrap();

    mqtt.skip(2).await; // Skip mappings of above child device creation messages and republished messages with @id

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main/service/collectd"),
        r#"{ "@type": "service", "type": "systemd", "name": "Collectd" }"#,
    ))
    .await
    .unwrap();

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "102,test-device:device:main:service:collectd,systemd,Collectd,up",
        )],
    )
    .await;

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1/service/collectd"),
        r#"{ "@type": "service", "type": "systemd", "name": "Collectd" }"#,
    ))
    .await
    .unwrap();

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child1",
            "102,test-device:device:child1:service:collectd,systemd,Collectd,up",
        )],
    )
    .await;

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child2/service/collectd"),
        r#"{ "@type": "service", "type": "systemd", "name": "Collectd" }"#,
    ))
    .await
    .unwrap();

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child2",
            "102,test-device:device:child2:service:collectd,systemd,Collectd,up",
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_publishes_supported_software_types() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate software_list capability message
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/software_list"),
        json!({"types": ["apt", "docker"]}).to_string(),
    ))
    .await
    .expect("Send failed");

    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/inventory/managedObjects/update/test-device",
            json!({"c8y_SupportedSoftwareTypes":["apt","docker"]}),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_mapping_to_smartrest() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main///a/temperature_alarm"),
        r#"{ "severity": "major", "text": "Temperature high" }"#,
    ))
    .await
    .unwrap();

    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "302,temperature_alarm,Temperature high")],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_alarm_mapping_to_smartrest() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Register the device upfront
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor//"),
        r#"{"@type": "child-device"}"#,
    ))
    .await
    .expect("Send failed");
    mqtt.skip(1).await; // Skip the mapped registration message

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor///a/temperature_high"),
        json!({ "severity": "minor", "text": "Temperature high" }).to_string(),
    ))
    .await
    .unwrap();

    // Expect child device creation and converted temperature alarm messages
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:external_sensor",
            "303,temperature_high,Temperature high",
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_with_custom_fragment_mapping_to_c8y_json() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    mqtt.send(MqttMessage::new(
        &"te/device/main///a/custom_temperature_alarm"
            .try_into()
            .unwrap(),
        json!({
            "severity": "major",
            "text": "Temperature high",
            "time":"2023-01-25T18:41:14.776170774Z",
            "customFragment": {
                "nested": {
                    "value": "extra info"
                }
            }
        })
        .to_string(),
    ))
    .await
    .unwrap();

    // Expect converted temperature alarm message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/alarm/alarms/create",
            json!({
                "type":"custom_temperature_alarm",
                "severity":"MAJOR",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"Temperature high",
                "customFragment": {
                    "nested": {
                        "value":"extra info"
                    }
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_alarm_with_custom_fragment_mapping_to_c8y_json() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Register the device upfront
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor//"),
        r#"{"@type": "child-device"}"#,
    ))
    .await
    .expect("Send failed");
    mqtt.skip(1).await; // Skip the mapped registration message

    mqtt.send(MqttMessage::new(
        &"te/device/external_sensor///a/custom_temperature_alarm"
            .try_into()
            .unwrap(),
        json!({
            "severity": "major",
            "text": "Temperature high",
            "time":"2023-01-25T18:41:14.776170774Z",
            "customFragment": {
                "nested": {
                    "value": "extra info"
                }
            }
        })
        .to_string(),
    ))
    .await
    .unwrap();

    // Expect converted temperature alarm message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/alarm/alarms/create",
            json!({
                "type":"custom_temperature_alarm",
                "severity":"MAJOR",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"Temperature high",
                "customFragment": {
                    "nested": {
                        "value":"extra info"
                    }
                },
                "externalSource": {
                    "externalId":"test-device:device:external_sensor",
                    "type":"c8y_Serial"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_with_message_as_custom_fragment_mapping_to_c8y_json() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    mqtt.send(MqttMessage::new(
        &"te/device/main///a/custom_msg_pressure_alarm"
            .try_into()
            .unwrap(),
        json!({
            "severity": "major",
            "text": "Pressure high",
            "time":"2023-01-25T18:41:14.776170774Z",
            "message":"custom message"
        })
        .to_string(),
    ))
    .await
    .unwrap();

    // Expect converted temperature alarm message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/alarm/alarms/create",
            json!({
                "type":"custom_msg_pressure_alarm",
                "severity":"MAJOR",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"Pressure high",
                "message":"custom message"
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_alarm_with_message_custom_fragment_mapping_to_c8y_json() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Register the device upfront
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor//"),
        r#"{"@type": "child-device"}"#,
    ))
    .await
    .expect("Send failed");
    mqtt.skip(1).await; // Skip the mapped registration message

    mqtt.send(MqttMessage::new(
        &"te/device/external_sensor///a/child_custom_msg_pressure_alarm"
            .try_into()
            .unwrap(),
        json!({
            "severity": "major",
            "text": "Pressure high",
            "time":"2023-01-25T18:41:14.776170774Z",
            "message":"custom message"
        })
        .to_string(),
    ))
    .await
    .unwrap();

    // Expect converted temperature alarm message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/alarm/alarms/create",
            json!({
                "type":"child_custom_msg_pressure_alarm",
                "severity":"MAJOR",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"Pressure high",
                "message":"custom message",
                "externalSource":{
                    "externalId":"test-device:device:external_sensor",
                    "type":"c8y_Serial"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_alarm_with_custom_message() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Register the device upfront
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor//"),
        r#"{"@type": "child-device"}"#,
    ))
    .await
    .expect("Send failed");
    mqtt.skip(1).await; // Skip the mapped registration message

    mqtt.send(MqttMessage::new(
        &"te/device/external_sensor///a/child_msg_to_text_pressure_alarm"
            .try_into()
            .unwrap(),
        json!({
            "severity": "major",
            "time":"2023-01-25T18:41:14.776170774Z",
            "message":"Pressure high"
        })
        .to_string(),
    ))
    .await
    .unwrap();

    // Expect converted temperature alarm message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/alarm/alarms/create",
            json!({
                "type":"child_msg_to_text_pressure_alarm",
                "severity":"MAJOR",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"child_msg_to_text_pressure_alarm",
                "message":"Pressure high",
                "externalSource":{
                    "externalId":"test-device:device:external_sensor",
                    "type":"c8y_Serial"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_with_custom_message() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    mqtt.send(MqttMessage::new(
        &"te/device/main///a/msg_to_text_pressure_alarm"
            .try_into()
            .unwrap(),
        json!({
            "severity": "major",
            "time":"2023-01-25T18:41:14.776170774Z",
            "message":"Pressure high"
        })
        .to_string(),
    ))
    .await
    .unwrap();

    // Expect converted temperature alarm message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/alarm/alarms/create",
            json!({
                "type":"msg_to_text_pressure_alarm",
                "severity":"MAJOR",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"msg_to_text_pressure_alarm",
                "message":"Pressure high",
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_alarm_empty_payload() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Register the device upfront
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor//"),
        r#"{"@type": "child-device"}"#,
    ))
    .await
    .expect("Send failed");
    mqtt.skip(1).await; // Skip the mapped registration message

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor///a/empty_temperature_alarm"),
        "".to_string(),
    ))
    .await
    .unwrap();

    // Expect converted alarm SmartREST message
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:external_sensor",
            "306,empty_temperature_alarm",
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_empty_payload() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main///a/empty_temperature_alarm"),
        "".to_string(),
    ))
    .await
    .unwrap();

    // Expect converted alarm SmartREST message
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "306,empty_temperature_alarm")]).await;
}

#[tokio::test]
async fn c8y_mapper_alarm_empty_json_payload() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main///a/empty_temperature_alarm"),
        "{}",
    ))
    .await
    .unwrap();

    // Expect converted alarm SmartREST message
    let smartrest = mqtt.recv().await.unwrap();
    assert_eq!(smartrest.topic.name, "c8y/s/us");
    assert!(smartrest
        .payload
        .as_str()
        .unwrap()
        .starts_with("303,empty_temperature_alarm,empty_temperature_alarm"));
}

#[tokio::test]
async fn c8y_mapper_child_event() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;
    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Register the device upfront
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor//"),
        r#"{"@type": "child-device"}"#,
    ))
    .await
    .expect("Send failed");
    mqtt.skip(1).await; // Skip the mapped registration message

    mqtt.send(MqttMessage::new(
        &"te/device/external_sensor///e/custom_event"
            .try_into()
            .unwrap(),
        json!({
            "text": "Someone logged-in",
            "time":"2023-01-25T18:41:14.776170774Z",
        })
        .to_string(),
    ))
    .await
    .unwrap();

    // Expect converted temperature alarm message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/event/events/create",
            json!({
                "type":"custom_event",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"Someone logged-in",
                "externalSource": {
                    "externalId":"test-device:device:external_sensor",
                    "type":"c8y_Serial"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_service_event() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;
    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Register the device upfront
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor//"),
        r#"{"@type": "child-device"}"#,
    ))
    .await
    .expect("Send failed");
    // Register the device upfront
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor/service/service_child"),
        r#"{"@type": "child-device"}"#,
    ))
    .await
    .expect("Send failed");
    mqtt.skip(2).await; // Skip the mapped registration messages

    mqtt.send(MqttMessage::new(
        &"te/device/external_sensor/service/service_child/e/custom_event"
            .try_into()
            .unwrap(),
        json!({
            "text": "Someone logged-in",
            "time":"2023-01-25T18:41:14.776170774Z",
        })
        .to_string(),
    ))
    .await
    .unwrap();

    // Expect converted event message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/event/events/create",
            json!({
                "type":"custom_event",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"Someone logged-in",
                "externalSource": {
                    "externalId":"test-device:device:external_sensor:service:service_child",
                    "type":"c8y_Serial"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_main_service_event() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;
    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Register the service upfront
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main/service/service_main"),
        r#"{"@type": "service"}"#,
    ))
    .await
    .expect("Send failed");
    mqtt.skip(1).await; // Skip the mapped registration messages

    mqtt.send(MqttMessage::new(
        &"te/device/main/service/service_main/e/custom_event"
            .try_into()
            .unwrap(),
        json!({
            "text": "Someone logged-in",
            "time":"2023-01-25T18:41:14.776170774Z",
        })
        .to_string(),
    ))
    .await
    .unwrap();

    // Expect converted event for the main device service
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/event/events/create",
            json!({
                "type":"custom_event",
                "time":"2023-01-25T18:41:14.776170774Z",
                "text":"Someone logged-in",
                "externalSource": {
                    "externalId":"test-device:device:main:service:service_main",
                    "type":"c8y_Serial"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_service_alarm() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;
    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Register the device and service upfront
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor//"),
        r#"{"@type": "child-device"}"#,
    ))
    .await
    .expect("Send failed");
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor/service/service_child"),
        r#"{"@type": "service"}"#,
    ))
    .await
    .expect("Send failed");
    mqtt.skip(2).await; // Skip the mapped registration messages

    mqtt.send(MqttMessage::new(
        &"te/device/external_sensor/service/service_child/a/custom_alarm"
            .try_into()
            .unwrap(),
        json!({
            "severity":"critical",
            "text": "temperature alarm",
            "time":"2023-01-25T18:41:14.776170774Z",
        })
        .to_string(),
    ))
    .await
    .unwrap();

    // Expect converted alarm for the main device service
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:external_sensor:service:service_child",
            "301,custom_alarm,temperature alarm,2023-01-25T18:41:14.776170774Z",
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_main_service_alarm() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;
    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Register the service upfront
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main/service/service_main"),
        r#"{"@type": "service"}"#,
    ))
    .await
    .expect("Send failed");
    mqtt.skip(1).await; // Skip the mapped registration messages

    mqtt.send(MqttMessage::new(
        &"te/device/main/service/service_main/a/custom_alarm"
            .try_into()
            .unwrap(),
        json!({
            "severity":"critical",
            "text": "temperature alarm",
            "time":"2023-01-25T18:41:14.776170774Z",
        })
        .to_string(),
    ))
    .await
    .unwrap();

    // Expect converted alarm for the main device service
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:main:service:service_main",
            "301,custom_alarm,temperature alarm,2023-01-25T18:41:14.776170774Z",
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_complex_text_fragment_in_payload_failed() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main///a/complex_text_alarm"),
        json!({
            "severity": "major",
            "text":{
                "nested":{
                    "value":"extra info"
                }
            },
            "time":"2023-01-25T18:41:14.776170774Z",
            "message":"custom message"
        })
        .to_string(),
    ))
    .await
    .unwrap();

    // Expect converted alarm SmartREST message
    assert_received_contains_str(&mut mqtt, [("te/errors", "Parsing of alarm message for the type: complex_text_alarm failed due to error: invalid")]).await;
}

#[tokio::test]
async fn mapper_handles_multiple_modules_in_update_list_sm_requests() {
    // The test assures if Mapper can handle multiple update modules received via JSON over MQTT
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;

    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Publish multiple modules software update via JSON over MQTT.
    mqtt.send(MqttMessage::new(
        &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
        json!({
            "id": "123456",
            "c8y_SoftwareUpdate": [
                {
                    "name": "nodered",
                    "action": "install",
                    "version": "1.0.0::debian",
                    "url": ""
                },
                {
                    "name": "rolldice",
                    "action": "install",
                    "version": "2.0.0::debian",
                    "url": ""
                }
            ],
            "externalSource": {
                "externalId": "test-device",
                "type": "c8y_Serial"
            }
        })
        .to_string(),
    ))
    .await
    .expect("Send failed");

    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/main///cmd/software_update/c8y-mapper-123456",
            json!({
                "status": "init",
                "updateList": [
                    {
                        "type": "debian",
                        "modules": [
                            {
                                "name": "nodered",
                                "version": "1.0.0",
                                "action": "install"
                            },
                            {
                                "name": "rolldice",
                                "version": "2.0.0",
                                "action": "install"
                            }
                        ]
                    }
                ]
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_publishes_supported_operations() {
    // The test assures tedge-mapper reads/parses the operations from operations directory and
    // correctly publishes the supported operations message on `c8y/s/us`
    // and verifies the supported operations that are published by the tedge-mapper.
    let ttd = TempTedgeDir::new();
    create_thin_edge_operations(&ttd, vec!["c8y_TestOp1", "c8y_TestOp2"]);

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, .. } = test_handle;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    mqtt.skip(3).await;

    // Expect smartrest message on `c8y/s/us` with expected payload "114,c8y_TestOp1,c8y_TestOp2"
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "114,c8y_TestOp1,c8y_TestOp2")]).await;
}

#[tokio::test]
async fn mapper_dynamically_updates_supported_operations_for_tedge_device() {
    // The test assures tedge-mapper checks if there are operations, then it reads and
    // correctly publishes them on to `c8y/s/us`.
    // When mapper is running test adds a new operation into the operations directory, then the mapper discovers the new
    // operation and publishes list of supported operation including the new operation, and verifies the device create message.
    let ttd = TempTedgeDir::new();
    create_thin_edge_operations(&ttd, vec!["c8y_TestOp1", "c8y_TestOp2"]);

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, mut fs, .. } = test_handle;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Register the device upfront
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main/service/tedge-agent"),
        r#"{"@type": "service"}"#,
    ))
    .await
    .expect("Send failed");
    mqtt.skip(1).await; // Skip the mapped registration message

    // Simulate tedge-agent health status message
    mqtt.send(
        MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/tedge-agent/status/health"),
            "{\"status\":\"up\"}",
        )
        .with_retain(),
    )
    .await
    .expect("Send failed");

    // Skip tedge-agent health status mapping
    mqtt.skip(1).await;

    // Simulate FsEvent for the creation of a new operation file
    fs.send(FsWatchEvent::FileCreated(
        ttd.dir("operations")
            .dir("c8y")
            .file("c8y_TestOp3")
            .to_path_buf(),
    ))
    .await
    .expect("Send failed");

    // Expect smartrest message on `c8y/s/us` with expected payload "114,c8y_TestOp1,c8y_TestOp2,c8y_TestOp3".
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "114,c8y_TestOp1,c8y_TestOp2,c8y_TestOp3")],
    )
    .await;

    // Then the agent start adding it's own set of capabilities
    mqtt.send(
        MqttMessage::new(&Topic::new_unchecked("te/device/main///cmd/restart"), "{}").with_retain(),
    )
    .await
    .expect("Send failed");
    mqtt.send(
        MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/software_update"),
            "{}",
        )
        .with_retain(),
    )
    .await
    .expect("Send failed");

    // Expect an update list of capabilities with agent capabilities
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "114,c8y_Restart,c8y_TestOp1,c8y_TestOp2,c8y_TestOp3",
        )],
    )
    .await;
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "114,c8y_Restart,c8y_SoftwareUpdate,c8y_TestOp1,c8y_TestOp2,c8y_TestOp3",
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_dynamically_updates_supported_operations_for_child_device() {
    // The test assures tedge-mapper reads the operations for the child devices from the operations directory, and then it publishes them on to `c8y/s/us/child1`.
    // When mapper is running test adds a new operation for a child into the operations directory, then the mapper discovers the new
    // operation and publishes list of supported operation for the child device including the new operation, and verifies the device create message.
    let ttd = TempTedgeDir::new();
    create_thin_edge_child_operations(
        &ttd,
        "test-device:device:child1",
        vec!["c8y_ChildTestOp1", "c8y_ChildTestOp2"],
    );

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, mut fs, .. } = test_handle;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Register the device upfront
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1//"),
        r#"{"@type": "child-device"}"#,
    ))
    .await
    .expect("Send failed");
    mqtt.skip(1).await; // Skip the mapped registration message

    // Add a new operation for the child device
    // Simulate FsEvent for the creation of a new operation file
    fs.send(FsWatchEvent::FileCreated(
        ttd.dir("operations")
            .dir("c8y")
            .dir("test-device:device:child1")
            .file("c8y_ChildTestOp3")
            .to_path_buf(),
    ))
    .await
    .expect("Send failed");

    // Assert that the creation of the operation file alone doesn't trigger the supported operations update
    assert!(
        mqtt.recv().await.is_none(),
        "No messages expected on operation file creation event"
    );

    // Send any command metadata message to trigger the supported operations update
    mqtt.send(
        MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/restart"),
            "{}",
        )
        .with_retain(),
    )
    .await
    .expect("Send failed");

    // Expect an update list of capabilities with agent capabilities
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child1",
            "114,c8y_ChildTestOp1,c8y_ChildTestOp2,c8y_ChildTestOp3,c8y_Restart",
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_dynamically_updates_supported_operations_for_nested_child_device() {
    // The test assures tedge-mapper reads the operations for the child devices from the operations directory, and then it publishes them on to `c8y/s/us/child1`.
    // When mapper is running test adds a new operation for a child into the operations directory, then the mapper discovers the new
    // operation and publishes list of supported operation for the child device including the new operation, and verifies the device create message.
    let ttd = TempTedgeDir::new();
    create_thin_edge_child_operations(
        &ttd,
        "child11",
        vec!["c8y_ChildTestOp1", "c8y_ChildTestOp2"],
    );

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, mut fs, .. } = test_handle;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Register nested child device
    mqtt.send(
        MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            json!({
                "@type":"child-device",
                "@id": "child1",
                "name": "child1"
            })
            .to_string(),
        )
        .with_retain(),
    )
    .await
    .expect("Send failed");
    mqtt.send(
        MqttMessage::new(
            &Topic::new_unchecked("te/device/child11//"),
            json!({
                "@type":"child-device",
                "@id": "child11",
                "name": "child11",
                "@parent": "device/child1//"
            })
            .to_string(),
        )
        .with_retain(),
    )
    .await
    .expect("Send failed");

    assert_received_contains_str(
        &mut mqtt,
        [
            ("c8y/s/us", "101,child1,child1,thin-edge.io-child,false"),
            (
                "c8y/s/us/child1",
                "101,child11,child11,thin-edge.io-child,false",
            ),
        ],
    )
    .await;

    // Add a new operation for the child device
    // Simulate FsEvent for the creation of a new operation file
    fs.send(FsWatchEvent::FileCreated(
        ttd.dir("operations")
            .dir("c8y")
            .dir("child11")
            .file("c8y_ChildTestOp3")
            .to_path_buf(),
    ))
    .await
    .expect("Send failed");

    // Assert that the creation of the operation file alone doesn't trigger the supported operations update
    assert!(
        mqtt.recv().await.is_none(),
        "No messages expected on operation file creation event"
    );

    // Send any command metadata message to trigger the supported operations update
    mqtt.send(
        MqttMessage::new(
            &Topic::new_unchecked("te/device/child11///cmd/restart"),
            "{}",
        )
        .with_retain(),
    )
    .await
    .expect("Send failed");

    // Expect an update list of capabilities with agent capabilities
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/child11",
            "114,c8y_ChildTestOp1,c8y_ChildTestOp2,c8y_ChildTestOp3,c8y_Restart",
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_updating_the_inventory_fragments_from_file() {
    // The test Creates an inventory file in (Temp_base_Dir)/device/inventory.json
    // The tedge-mapper parses the inventory fragment file and publishes on c8y/inventory/managedObjects/update/test-device
    // Verify the fragment message that is published
    let ttd = TempTedgeDir::new();

    let version = env!("CARGO_PKG_VERSION");
    let custom_fragment_content = json!({
        "boolean_key": true,
        "numeric_key": 10,
        "string_key": "value",
        "c8y_Agent": {
            "name": "thin-edge.io",
            "url": "https://thin-edge.io",
            "version": version
        },
        "c8y_Firmware": {
            "name": "raspberrypi-bootloader",
            "url": "31aab9856861b1a587e2094690c2f6e272712cb1",
            "version": "1.20140107-1"
        }
    });
    create_inventory_json_file_with_content(&ttd, &custom_fragment_content.to_string());

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, .. } = test_handle;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    assert_received_includes_json(
        &mut mqtt,
        [
            ("te/device/main///twin/boolean_key", json!(true)),
            (
                "te/device/main///twin/c8y_Agent",
                json!({
                    "name": "thin-edge.io",
                    "url": "https://thin-edge.io",
                    "version": version
                }),
            ),
            (
                "te/device/main///twin/c8y_Firmware",
                json!({
                    "name": "raspberrypi-bootloader",
                    "url": "31aab9856861b1a587e2094690c2f6e272712cb1",
                    "version": "1.20140107-1"
                }),
            ),
            ("te/device/main///twin/name", json!("test-device")),
            ("te/device/main///twin/numeric_key", json!(10)),
            ("te/device/main///twin/string_key", json!("value")),
            ("te/device/main///twin/type", json!("test-device-type")),
        ],
    )
    .await;
}

#[tokio::test]
async fn override_type_using_inventory_fragments_file() {
    // The test Creates an inventory file in (Temp_base_Dir)/device/inventory.json
    // The tedge-mapper parses the inventory fragment file and publishes on c8y/inventory/managedObjects/update/test-device
    // Verify the fragment message that is published
    let ttd = TempTedgeDir::new();

    let version = env!("CARGO_PKG_VERSION");
    let custom_fragment_content = json!({
        "name": "new-name",
        "type": "new-type",
        "c8y_Firmware": {
            "name": "raspberrypi-bootloader",
            "url": "31aab9856861b1a587e2094690c2f6e272712cb1",
            "version": "1.20140107-1"
        }
    });
    create_inventory_json_file_with_content(&ttd, &custom_fragment_content.to_string());

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, .. } = test_handle;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    assert_received_includes_json(
        &mut mqtt,
        [
            (
                "te/device/main///twin/c8y_Agent",
                json!({
                    "name": "thin-edge.io",
                    "url": "https://thin-edge.io",
                    "version": version
                }),
            ),
            (
                "te/device/main///twin/c8y_Firmware",
                json!({
                    "name": "raspberrypi-bootloader",
                    "url": "31aab9856861b1a587e2094690c2f6e272712cb1",
                    "version": "1.20140107-1"
                }),
            ),
            ("te/device/main///twin/name", json!("new-name")),
            ("te/device/main///twin/type", json!("new-type")),
        ],
    )
    .await;
}

#[tokio::test]
async fn custom_operation_without_timeout_successful() {
    // The test assures SM Mapper correctly receives custom operation on `c8y/s/ds`
    // and executes the custom operation successfully, no timeout given here.

    let ttd = TempTedgeDir::new();

    let cmd_file = ttd.path().join("command");
    //create custom operation file
    create_custom_op_file(&ttd, cmd_file.as_path(), None, None);
    //create command
    let content = r#"#!/bin/sh
    for i in $(seq 1 2)
    do
        sleep 1
    done
    echo "Executed successfully without timeout"
    "#;
    create_custom_cmd(cmd_file.as_path(), content);

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_Command SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(&"c8y".try_into().unwrap()),
        "511,test-device,c8y_Command",
    ))
    .await
    .expect("Send failed");

    // Expect `501` smartrest message on `c8y/s/us`.
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_Command")]).await;

    // Expect `503` smartrest message on `c8y/s/us`.
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "503,c8y_Command,\"Executed successfully without timeout\n\"",
        )],
    )
    .await;

    // assert the signterm is handled
    let expected_content = "command \"511,test-device,c8y_Command\"
Exit status: 0 (OK)

stderr (EMPTY)

stdout <<EOF
Executed successfully without timeout
EOF
";

    assert_command_exec_log_content(ttd, expected_content);
}

#[tokio::test]
async fn custom_operation_with_timeout_successful() {
    // The test assures SM Mapper correctly receives custom operation on `c8y/s/ds`
    // and executes the custom operation within the timeout period

    let ttd = TempTedgeDir::new();
    let cmd_file = ttd.path().join("command");
    //create custom operation file
    create_custom_op_file(&ttd, cmd_file.as_path(), Some(4), Some(2));
    //create command
    let content = r#"#!/bin/sh
    for i in $(seq 1 2)
    do
        sleep 1
    done
    echo "Successfully Executed"
    "#;
    create_custom_cmd(cmd_file.as_path(), content);

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_Command SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(&"c8y".try_into().unwrap()),
        "511,test-device,c8y_Command",
    ))
    .await
    .expect("Send failed");

    // Expect `501` smartrest message on `c8y/s/us`.
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_Command")]).await;

    // Expect `503` smartrest message on `c8y/s/us`.
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "503,c8y_Command,\"Successfully Executed\n\"")],
    )
    .await;

    // assert the signterm is handled
    let expected_content = "command \"511,test-device,c8y_Command\"
Exit status: 0 (OK)

stderr (EMPTY)

stdout <<EOF
Successfully Executed
EOF
";

    assert_command_exec_log_content(ttd, expected_content);
}

#[tokio::test]
async fn custom_operation_timeout_sigterm() {
    // The test assures SM Mapper correctly receives custom operation on `c8y/s/ds`
    // and executes the custom operation, it will timeout because it will not complete before given timeout
    // sigterm is sent to stop the custom operation

    let ttd = TempTedgeDir::new();
    let cmd_file = ttd.path().join("command");
    //create custom operation file
    create_custom_op_file(&ttd, cmd_file.as_path(), Some(1), Some(2));
    //create command
    let content = r#"#!/bin/sh
    trap 'echo received SIGTERM; exit 124' TERM
    for i in $(seq 1 10)
    do
        echo "main $i"
        sleep 2
    done
    "#;
    create_custom_cmd(cmd_file.as_path(), content);

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_Command SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(&"c8y".try_into().unwrap()),
        "511,test-device,c8y_Command",
    ))
    .await
    .expect("Send failed");

    // Expect `501` smartrest message on `c8y/s/us`.
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_Command")]).await;

    // Expect `501` smartrest message on `c8y/s/us`.
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "502,c8y_Command,operation failed due to timeout: duration=1s",
        )],
    )
    .await;

    // assert the signterm is handled
    let expected_content = "command \"511,test-device,c8y_Command\"
Exit status: 124 (ERROR)

stderr <<EOF
operation failed due to timeout: duration=1sEOF

stdout <<EOF
main 1
received SIGTERM
EOF
";

    assert_command_exec_log_content(ttd, expected_content);
}

#[tokio::test]
async fn custom_operation_timeout_sigkill() {
    // The test assures SM Mapper correctly receives custom operation on `c8y/s/ds`
    // and executes the custom operation, it will timeout because it will not complete before given timeout
    // sigterm sent first, still the operation did not stop, so sigkill will be sent to stop the operation

    let ttd = TempTedgeDir::new();

    let cmd_file = ttd.path().join("command");
    //create custom operation file
    create_custom_op_file(&ttd, cmd_file.as_path(), Some(1), Some(2));
    //create command
    let content = r#"#!/bin/sh
    trap 'echo ignore SIGTERM' TERM
    for i in $(seq 1 50)
    do
        echo "main $i"
        sleep 2
    done
    "#;
    create_custom_cmd(cmd_file.as_path(), content);

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(Duration::from_secs(5));

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_Command SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(&"c8y".try_into().unwrap()),
        "511,test-device,c8y_Command",
    ))
    .await
    .expect("Send failed");

    // Expect `501` smartrest message on `c8y/s/us`.
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_Command")]).await;

    // Expect `502` smartrest message on `c8y/s/us`.
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "502,c8y_Command,operation failed due to timeout: duration=1s",
        )],
    )
    .await;

    // assert the signterm is handled
    let expected_content = "command \"511,test-device,c8y_Command\"
Killed by signal: 9

stderr <<EOF
operation failed due to timeout: duration=1sEOF

stdout <<EOF
main 1
ignore SIGTERM
main 2
EOF
";

    assert_command_exec_log_content(ttd, expected_content);
}

#[tokio::test]
async fn json_custom_operation_status_update_with_operation_id() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(
            r#"[exec]
            command = "echo ${.payload.c8y_Command.text}"
            on_fragment = "c8y_Command"
            "#,
        );

    let config = C8yMapperConfig {
        smartrest_use_operation_id: true,
        ..test_mapper_config(&ttd)
    };
    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_Command SmartREST request
    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_Command": {
                     "text": "do something"
                 },
            "externalSource":{
           "externalId":"test-device",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "504,1234")]).await;
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "506,1234,\"do something\n\"")]).await;
}

#[tokio::test]
async fn json_custom_operation_status_multiple_operations_in_one_mqtt_message() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(
            r#"[exec]
            command = "echo ${.payload.c8y_Command.text}"
            on_fragment = "c8y_Command"
            "#,
        );

    let config = C8yMapperConfig {
        smartrest_use_operation_id: true,
        ..test_mapper_config(&ttd)
    };
    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_Command SmartREST request
    let operation_1 = json!({
             "status":"PENDING",
             "id": "111",
             "c8y_Command": {
                 "text": "do something \"1\""
             },
        "externalSource":{
       "externalId":"test-device",
       "type":"c8y_Serial"
    }
         })
    .to_string();
    let operation_2 = json!({
             "status":"PENDING",
             "id": "222",
             "c8y_Command": {
                 "text": "do something \"2\""
             },
        "externalSource":{
       "externalId":"test-device",
       "type":"c8y_Serial"
    }
         })
    .to_string();
    let operation_3 = json!({
             "status":"PENDING",
             "id": "333",
             "c8y_Command": {
                 "text": "do something \"3\""
             },
        "externalSource":{
       "externalId":"test-device",
       "type":"c8y_Serial"
    }
         })
    .to_string();

    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        [operation_1, operation_2, operation_3].join("\n"),
    );
    mqtt.send(input_message).await.expect("Send failed");

    let mut messages = vec![];
    for _ in 0..6 {
        messages.push(mqtt.recv().await.unwrap());
    }
    let (mut requests, mut responses): (Vec<_>, Vec<_>) = messages
        .iter()
        .map(|msg| (msg.topic.name.as_str(), msg.payload.as_str().unwrap()))
        .partition(|(_topic, payload)| payload.starts_with("504,"));

    // The messages might get processed out of order, we don't care about the ordering of the messages
    requests.sort();
    responses.sort();

    assert_eq!(
        requests,
        [
            ("c8y/s/us", "504,111"),
            ("c8y/s/us", "504,222"),
            ("c8y/s/us", "504,333"),
        ]
    );
    // escapes: we input JSON over MQTT, but emit Smartrest, thus initial: `do something "1"` becomes `"do something
    // ""1""\n"` (outer "" for the Smartrest record field, and then inside double quotes escape a single quote)
    assert_eq!(
        responses,
        [
            ("c8y/s/us", "506,111,\"do something \"\"1\"\"\n\""),
            ("c8y/s/us", "506,222,\"do something \"\"2\"\"\n\""),
            ("c8y/s/us", "506,333,\"do something \"\"3\"\"\n\""),
        ]
    );
}

#[tokio::test]
async fn json_custom_operation_status_update_with_operation_name() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(
            r#"[exec]
            command = "echo ${.payload.c8y_Command.text}"
            on_fragment = "c8y_Command"
            "#,
        );

    let config = C8yMapperConfig {
        smartrest_use_operation_id: false,
        ..test_mapper_config(&ttd)
    };
    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_Command SmartREST request
    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_Command": {
                     "text": "do something"
                 },
            "externalSource":{
           "externalId":"test-device",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_Command")]).await;
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "503,c8y_Command,\"do something\n\"")],
    )
    .await;
}

#[tokio::test]
async fn json_custom_operation_skip_status_update() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(
            r#"[exec]
            command = "echo ${.payload.c8y_Command.text}"
            on_fragment = "c8y_Command"
            skip_status_update = true
            "#,
        );

    let config = C8yMapperConfig {
        smartrest_use_operation_id: true,
        ..test_mapper_config(&ttd)
    };
    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_Command SmartREST request
    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_Command": {
                     "text": "do something"
                 },
            "externalSource":{
           "externalId":"TST_haul_searing_set",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    // No MQTT message should be sent
    let recv = mqtt.recv().await;
    assert!(recv.is_none());
}

#[tokio::test]
async fn json_custom_operation_status_update_with_custom_topic() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(
            r#"[exec]
            topic = "${.bridge.topic_prefix}/custom/operation/one"
            command = "echo ${.payload.c8y_Command.text}"
            on_fragment = "c8y_Command"
            "#,
        );

    let config = C8yMapperConfig {
        smartrest_use_operation_id: false,
        ..test_mapper_config(&ttd)
    };
    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_Command SmartREST request
    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/custom/operation/one"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_Command": {
                     "text": "do something"
                 },
            "externalSource":{
           "externalId":"test-device",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_Command")]).await;
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "503,c8y_Command,\"do something\n\"")],
    )
    .await;
}

#[tokio::test]
async fn mapper_converts_custom_operation_for_main_device() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command.template")
        .with_raw_content(
            r#"[exec]
            topic = "c8y/devicecontrol/notifications"
            on_fragment = "c8y_Command"
            
            [exec.workflow]
            operation = "command"
            input = "${.payload.c8y_Command}"
            output = "${.payload.result}"
            "#,
        );

    let config = test_mapper_config(&ttd);

    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // indicate that main device supports the operation
    let capability_message =
        MqttMessage::new(&Topic::new_unchecked("te/device/main///cmd/command"), "{}");

    mqtt.send(capability_message).await.unwrap();

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "114,c8y_Command")]).await;

    assert!(ttd.path().join("operations/c8y/c8y_Command").is_symlink());

    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_Command": {
                     "text": "do something"
                 },
            "externalSource":{
           "externalId":"test-device",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/main///cmd/command/c8y-mapper-1234",
            json!({
                "status": "init",
                "text": "do something",
                "c8y-mapper": {
                    "on_fragment": "c8y_Command",
                    "output": "${.payload.result}"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_converts_custom_operation_with_combined_input() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_CombinedInput.template")
        .with_raw_content(
            r#"[exec]
            topic = "c8y/devicecontrol/notifications"
            on_fragment = "c8y_CombinedInput"

            [exec.workflow]
            operation = "command"
            input.x = "${.payload.c8y_CombinedInput.inner.x}"
            input.y = "${.payload.c8y_CombinedInput.inner.y}"
            input.z = { foo = "bar" }
            "#,
        );

    let config = test_mapper_config(&ttd);

    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // indicate that main device supports the operation
    let capability_message =
        MqttMessage::new(&Topic::new_unchecked("te/device/main///cmd/command"), "{}");

    mqtt.send(capability_message).await.unwrap();

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "114,c8y_CombinedInput")]).await;

    assert!(ttd
        .path()
        .join("operations/c8y/c8y_CombinedInput")
        .is_symlink());

    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_CombinedInput": {
                     "text": "do something",
                     "inner": {
                        "x": "x value",
                        "y": 42,
                        "z": "z unused value",
                     },
                 },
            "externalSource":{
           "externalId":"test-device",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/main///cmd/command/c8y-mapper-1234",
            json!({
                "status": "init",
                "x": "x value",
                "y": 42,
                "z": {
                    "foo": "bar"
                },
                "c8y-mapper": {
                    "on_fragment": "c8y_CombinedInput"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_converts_custom_operation_for_main_device_without_workflow_input() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command.template")
        .with_raw_content(
            r#"[exec]
            topic = "c8y/devicecontrol/notifications"
            on_fragment = "c8y_Command"
            
            [exec.workflow]
            operation = "command"
            "#,
        );

    let config = test_mapper_config(&ttd);

    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // indicate that main device supports the operation
    let capability_message =
        MqttMessage::new(&Topic::new_unchecked("te/device/main///cmd/command"), "{}");

    mqtt.send(capability_message).await.unwrap();

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "114,c8y_Command")]).await;

    assert!(ttd.path().join("operations/c8y/c8y_Command").is_symlink());

    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_Command": {
                     "text": "do something"
                 },
            "externalSource":{
           "externalId":"test-device",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/main///cmd/command/c8y-mapper-1234",
            json!({
                "status": "init",
                "c8y-mapper": {
                    "on_fragment": "c8y_Command"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_converts_custom_operation_for_main_device_with_invalid_workflow_input() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command.template")
        .with_raw_content(
            r#"[exec]
            topic = "c8y/devicecontrol/notifications"
            on_fragment = "c8y_Command"
            
            [exec.workflow]
            operation = "command"
            input = "invalid input"
            "#,
        );

    let config = test_mapper_config(&ttd);

    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // indicate that main device supports the operation
    let capability_message =
        MqttMessage::new(&Topic::new_unchecked("te/device/main///cmd/command"), "{}");

    mqtt.send(capability_message).await.unwrap();

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "114,c8y_Command")]).await;

    assert!(ttd.path().join("operations/c8y/c8y_Command").is_symlink());

    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_Command": {
                     "text": "do something"
                 },
            "externalSource":{
           "externalId":"test-device",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    // message should not be sent due to incorrect payload
    assert!(mqtt.recv().await.is_none());
}

#[tokio::test]
async fn mapper_converts_custom_operation_for_child_device() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command.template")
        .with_raw_content(
            r#"[exec]
            topic = "c8y/devicecontrol/notifications"
            on_fragment = "c8y_Command"

            [exec.workflow]
            operation = "command"
            input = "${.payload.c8y_Command}"
            "#,
        );

    let config = test_mapper_config(&ttd);

    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    //register child device
    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1//"),
        json!({
            "@type":"child-device",
            "@parent":"device/main//",
            "@id":"child1"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    mqtt.skip(1).await;

    // indicate that child device supports the operation
    let capability_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1///cmd/command"),
        "{}",
    );

    mqtt.send(capability_message).await.unwrap();

    assert_received_contains_str(&mut mqtt, [("c8y/s/us/child1", "114,c8y_Command")]).await;

    assert!(ttd
        .path()
        .join("operations/c8y/child1/c8y_Command")
        .is_symlink());

    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_Command": {
                     "text": "do something"
                 },
            "externalSource":{
           "externalId":"child1",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/child1///cmd/command/c8y-mapper-1234",
            json!({
                "status": "init",
                "text": "do something",
                "c8y-mapper": {
                    "on_fragment": "c8y_Command"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_nested_child_alarm_mapping_to_smartrest() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/immediate_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/main//",
            "@id":"immediate_child"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/immediate_child//",
            "@id":"nested_child"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child///a/"),
        json!({ "severity": "minor", "text": "Temperature high","time":"2023-10-13T15:00:07.172674353Z" }).to_string(),
    ))
        .await
        .unwrap();

    // Expect nested child device creating an minor alarm
    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us",
                "101,immediate_child,immediate_child,thin-edge.io-child,false",
            ),
            (
                "c8y/s/us/immediate_child",
                "101,nested_child,nested_child,thin-edge.io-child,false",
            ),
            (
                "c8y/s/us/nested_child",
                "303,ThinEdgeAlarm,Temperature high,2023-10-13T15:00:07.172674353Z",
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_nested_child_event_mapping_to_smartrest() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/immediate_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/main//",
            "@id":"immediate_child"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/immediate_child//",
            "@id":"nested_child"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child///e/"),
        json!({ "text": "Temperature high","time":"2023-10-13T15:00:07.172674353Z" }).to_string(),
    ))
    .await
    .unwrap();

    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us",
                "101,immediate_child,immediate_child,thin-edge.io-child,false",
            ),
            (
                "c8y/s/us/immediate_child",
                "101,nested_child,nested_child,thin-edge.io-child,false",
            ),
        ],
    )
    .await;
    // Expect nested child device creating an event
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/event/events/create",
            json!({
                "type":"ThinEdgeEvent",
                "time":"2023-10-13T15:00:07.172674353Z",
                "text":"Temperature high",
                "externalSource":{"externalId":"nested_child","type":"c8y_Serial"}
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_nested_child_service_alarm_mapping_to_smartrest() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/immediate_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/main//",
            "@id":"immediate_child"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/immediate_child//",
            "@id":"nested_child"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child/service/nested_service"),
        json!({
            "@type":"service",
            "@parent":"device/nested_child//",
            "@id":"nested_service"
        })
        .to_string(),
    );

    mqtt.send(reg_message).await.unwrap();

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child/service/nested_service/a/"),
        json!({ "severity": "minor", "text": "Temperature high","time":"2023-10-13T15:00:07.172674353Z" }).to_string(),
    ))
        .await
        .unwrap();

    mqtt.skip(3).await;

    // Expect child device service creating minor temperature alarm messages
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/nested_service",
            "303,ThinEdgeAlarm,Temperature high,2023-10-13T15:00:07.172674353Z",
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_nested_child_service_event_mapping_to_smartrest() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/immediate_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/main//",
            "@id":"immediate_child"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child//"),
        json!({
            "@type":"child-device",
            "@parent":"device/immediate_child//",
            "@id":"nested_child"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child/service/nested_service"),
        json!({
            "@type":"service",
            "@parent":"device/nested_child//",
            "@id":"nested_service"
        })
        .to_string(),
    );

    mqtt.send(reg_message).await.unwrap();

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/nested_child/service/nested_service/e/"),
        json!({ "text": "Temperature high","time":"2023-10-13T15:00:07.172674353Z" }).to_string(),
    ))
    .await
    .unwrap();

    mqtt.skip(3).await;

    // Expect nested child device service creating the event messages
    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/event/events/create",
            json!({
                "type":"ThinEdgeEvent",
                "time":"2023-10-13T15:00:07.172674353Z",
                "text":"Temperature high",
                "externalSource":{"externalId":"nested_service","type":"c8y_Serial"}
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_processes_operations_concurrently() {
    let num_operations = 20;

    let mut fts_server = mockito::Server::new_async().await;
    let _mock = fts_server
        .mock(
            "GET",
            "/te/v1/files/test-device/config_snapshot/c8y-mapper-1234",
        )
        // make each download block so it doesn't complete before we submit all operations
        .with_chunked_body(|_w| {
            std::thread::sleep(Duration::from_secs(5));
            Ok(())
        })
        .expect(num_operations)
        .create_async()
        .await;
    let host_port = fts_server.host_with_port();

    let cfg_dir = TempTedgeDir::new();
    let TestHandle {
        mqtt,
        http,
        dl,
        mut timer,
        ..
    } = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    let mut dl = dl.with_timeout(TEST_TIMEOUT_MS);

    spawn_dummy_c8y_http_proxy(http);

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // simulate many successful operations that needs to be handled by the mapper
    for i in 0..num_operations {
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked(&format!("te/device/main///cmd/log_upload/c8y-mapper-{i}")),
            json!({
            "status": "successful",
            "tedgeUrl": format!("http://{host_port}/te/v1/files/test-device/log_upload/c8y-mapper-1234"),
            "type": "mosquitto",
            "dateFrom": "2023-11-28T16:33:50+01:00",
            "dateTo": "2023-11-29T16:33:50+01:00",
            "searchText": "ERROR",
            "lines": 1000

        })
                .to_string(),
        ))
            .await.unwrap();

        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked(&format!("te/device/main///cmd/config_snapshot/c8y-mapper-{i}")),
            json!({
            "status": "successful",
            "tedgeUrl": format!("http://{host_port}/te/v1/files/test-device/config_snapshot/c8y-mapper-1234"),
            "type": "path/type/A",
        })
                .to_string(),
        ))
            .await.unwrap();
    }

    for _ in 0..(num_operations * 2) {
        dl.recv()
            .await
            .expect("there should be one DownloadRequest per operation");
    }
}

#[tokio::test]
async fn mapper_processes_other_operations_while_uploads_and_downloads_are_ongoing() {
    let cfg_dir = TempTedgeDir::new();
    let TestHandle {
        mqtt,
        http,
        dl,
        ul,
        mut timer,
        ..
    } = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    let mut dl = dl.with_timeout(TEST_TIMEOUT_MS);
    let mut ul = ul.with_timeout(TEST_TIMEOUT_MS);

    spawn_dummy_c8y_http_proxy(http);

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // simulate many successful operations that needs to be handled by the mapper
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/log_upload/c8y-mapper-1"),
        json!({
            "status": "successful",
            "tedgeUrl": "http://localhost:8888/te/v1/files/test-device/log_upload/c8y-mapper-1",
            "type": "mosquitto",
            "dateFrom": "2023-11-28T16:33:50+01:00",
            "dateTo": "2023-11-29T16:33:50+01:00",
            "searchText": "ERROR",
            "lines": 1000
        })
        .to_string(),
    ))
    .await
    .unwrap();

    let (download_id, download_request) = dl
        .recv()
        .await
        .expect("DownloadRequest for log_upload should be sent");
    assert_eq!(download_id, "c8y-mapper-1");
    assert_eq!(
        download_request.url,
        "http://localhost:8888/te/v1/files/test-device/log_upload/c8y-mapper-1"
    );

    // here it would be good to assert that upload message hasn't been sent yet, but due to the
    // behaviour of message channels it can't be easily done

    dl.send((
        "c8y-mapper-1".to_string(),
        Ok(DownloadResponse {
            url: "http://localhost:8888/te/v1/files/test-device/log_upload/c8y-mapper-1"
                .to_string(),
            file_path: "whatever".into(),
        }),
    ))
    .await
    .unwrap();

    let (upload_id, _) = ul
        .recv()
        .await
        .expect("UploadRequest for log_upload should be sent");
    assert_eq!(upload_id, "c8y-mapper-1");

    // now that an upload is ongoing, check that downloads can also be triggered
    mqtt.send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/config_snapshot/c8y-mapper-2"),
            json!({
            "status": "successful",
            "tedgeUrl": "http://localhost:8888/te/v1/files/test-device/config_snapshot/c8y-mapper-2",
            "type": "typeA",
        })
                .to_string(),
        ))
            .await.unwrap();

    let (config_snapshot_id, _) = dl
        .recv()
        .await
        .expect("DownloadRequest for config snapshot should be sent");
    assert_eq!(config_snapshot_id, "c8y-mapper-2");

    // while download and upload are ongoing, try some other operation that doesn't do download or upload
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/restart/c8y-mapper-3"),
        json!({
            "status": "successful",
        })
        .to_string(),
    ))
    .await
    .unwrap();

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "503,c8y_Restart")]).await;
}

#[tokio::test]
async fn mapper_doesnt_update_status_of_subworkflow_commands_3048() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // should hold for any operation type
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked(
            "te/device/rpizero2-d83add42f121///cmd/restart/sub:firmware_update:c8y-mapper-192481",
        ),
        r#"{"logPath":"/var/log/tedge/agent/workflow-firmware_update-c8y-mapper-192481.log","status":"executing"}"#,
    )).await.unwrap();

    while let Some(msg) = dbg!(mqtt.recv().await) {
        assert_ne!(msg.payload_str().unwrap(), "501,c8y_Restart");
    }
}

#[tokio::test]
async fn mapper_doesnt_send_duplicate_operation_status() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle {
        mqtt, mut timer, ..
    } = test_handle;

    // Complete sync phase so that alarm mapping starts
    trigger_timeout(&mut timer).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    for _ in 0..3 {
        mqtt.send(
            MqttMessage::new(
                &Topic::new_unchecked("te/device/main///cmd/config_snapshot/c8y-mapper-1"),
                r#"{"status":"executing", "type": "typeA"}"#,
            )
            .with_retain(),
        )
        .await
        .unwrap();
    }

    assert_eq!(
        mqtt.recv().await.unwrap().payload_str().unwrap(),
        "501,c8y_UploadConfigFile"
    );

    while let Some(msg) = mqtt.recv().await {
        assert_ne!(msg.payload_str().unwrap(), "501,c8y_UploadConfigFile");
    }
}

#[tokio::test]
async fn mapper_converts_config_metadata_to_supported_op_and_types_for_main_device() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, .. } = test_handle;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate config_snapshot cmd metadata message
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/config_snapshot"),
        r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
    ))
    .await
    .expect("Send failed");

    // Validate SmartREST message is published
    assert_received_contains_str(
        &mut mqtt,
        [
            ("c8y/s/us", "114,c8y_UploadConfigFile"),
            ("c8y/s/us", "119,typeA,typeB,typeC"),
        ],
    )
    .await;

    // Validate if the supported operation file is created
    assert!(ttd
        .path()
        .join("operations/c8y/c8y_UploadConfigFile")
        .exists());

    // Simulate config_update cmd metadata message
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/config_update"),
        r#"{"types" : [ "typeD", "typeE", "typeF" ]}"#,
    ))
    .await
    .expect("Send failed");

    // Validate SmartREST message is published
    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us",
                "114,c8y_DownloadConfigFile,c8y_UploadConfigFile",
            ),
            ("c8y/s/us", "119,typeD,typeE,typeF"),
        ],
    )
    .await;

    // Validate if the supported operation file is created
    assert!(ttd
        .path()
        .join("operations/c8y/c8y_DownloadConfigFile")
        .exists());
}

#[tokio::test]
async fn mapper_converts_config_cmd_to_supported_op_and_types_for_child_device() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, .. } = test_handle;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Register the device upfront
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1//"),
        r#"{"@type": "child-device"}"#,
    ))
    .await
    .expect("Send failed");
    mqtt.skip(1).await; // Skip the mapped registration message

    // Simulate config_snapshot cmd metadata message
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1///cmd/config_snapshot"),
        r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
    ))
    .await
    .expect("Send failed");

    // Validate SmartREST message is published
    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us/test-device:device:child1",
                "114,c8y_UploadConfigFile",
            ),
            (
                "c8y/s/us/test-device:device:child1",
                "119,typeA,typeB,typeC",
            ),
        ],
    )
    .await;

    // Validate if the supported operation file is created
    assert!(ttd
        .path()
        .join("operations/c8y/test-device:device:child1/c8y_UploadConfigFile")
        .exists());

    // Sending an updated list of config types
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1///cmd/config_snapshot"),
        r#"{"types" : [ "typeB", "typeC", "typeD" ]}"#,
    ))
    .await
    .expect("Send failed");

    // Assert that the updated config type list does not trigger a duplicate supported ops message
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child1",
            "119,typeB,typeC,typeD",
        )],
    )
    .await;

    // Simulate config_update cmd metadata message
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1///cmd/config_update"),
        r#"{"types" : [ "typeD", "typeE", "typeF" ]}"#,
    ))
    .await
    .expect("Send failed");

    // Validate SmartREST message is published
    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us/test-device:device:child1",
                "114,c8y_DownloadConfigFile,c8y_UploadConfigFile",
            ),
            (
                "c8y/s/us/test-device:device:child1",
                "119,typeD,typeE,typeF",
            ),
        ],
    )
    .await;

    // Validate if the supported operation file is created
    assert!(ttd
        .path()
        .join("operations/c8y/test-device:device:child1/c8y_DownloadConfigFile")
        .exists());

    // Sending an updated list of config types
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1///cmd/config_update"),
        r#"{"types" : [ "typeB", "typeC", "typeD" ]}"#,
    ))
    .await
    .expect("Send failed");

    // Assert that the updated config type list does not trigger a duplicate supported ops message
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child1",
            "119,typeB,typeC,typeD",
        )],
    )
    .await;
}

fn assert_command_exec_log_content(cfg_dir: TempTedgeDir, expected_contents: &str) {
    let paths = fs::read_dir(cfg_dir.to_path_buf().join("agent")).unwrap();
    for path in paths {
        let mut file =
            File::open(path.unwrap().path()).expect("Unable to open the command exec log file");
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .expect("Unable to read the file");
        assert!(contents.contains(expected_contents));
    }
}

fn create_custom_op_file(
    ttd: &TempTedgeDir,
    cmd_file: &Path,
    graceful_timeout: Option<i64>,
    forceful_timeout: Option<i64>,
) {
    let custom_op_file = ttd.dir("operations").dir("c8y").file("c8y_Command");
    let mut custom_content = toml::map::Map::new();
    custom_content.insert("name".into(), toml::Value::String("c8y_Command".into()));
    custom_content.insert("topic".into(), toml::Value::String("c8y/s/ds".into()));
    custom_content.insert("on_message".into(), toml::Value::String("511".into()));
    if let Some(timeout) = graceful_timeout {
        custom_content.insert("timeout".into(), toml::Value::Integer(timeout));
    }
    if let Some(timeout) = forceful_timeout {
        custom_content.insert("forceful_timeout".into(), toml::Value::Integer(timeout));
    }
    custom_content.insert(
        "command".into(),
        toml::Value::String(cmd_file.display().to_string()),
    );

    let mut map = toml::map::Map::new();
    map.insert("exec".into(), toml::Value::Table(custom_content));
    custom_op_file.with_toml_content(map);
}

fn create_custom_cmd(custom_cmd: &Path, content: &str) {
    with_exec_permission(custom_cmd, content);
}

fn create_inventory_json_file_with_content(ttd: &TempTedgeDir, content: &str) {
    let file = ttd.dir("device").file("inventory.json");
    file.with_raw_content(content);
}

fn create_thin_edge_operations(ttd: &TempTedgeDir, ops: Vec<&str>) {
    let p1 = ttd.dir("operations");
    let tedge_ops_dir = p1.dir("c8y");
    for op in ops {
        tedge_ops_dir.file(op);
    }
}

fn create_thin_edge_child_operations(ttd: &TempTedgeDir, child_id: &str, ops: Vec<&str>) {
    let p1 = ttd.dir("operations");
    let tedge_ops_dir = p1.dir("c8y");
    let child_ops_dir = tedge_ops_dir.dir(child_id);
    for op in ops {
        child_ops_dir.file(op);
    }
}

async fn trigger_timeout(timer: &mut FakeServerBox<SyncStart, SyncComplete>) {
    timer.recv().await.unwrap();
    timer.send(Timeout::new(())).await.unwrap();
}

pub(crate) async fn spawn_c8y_mapper_actor(tmp_dir: &TempTedgeDir, init: bool) -> TestHandle {
    spawn_c8y_mapper_actor_with_config(tmp_dir, test_mapper_config(tmp_dir), init).await
}

pub(crate) struct TestHandle {
    pub mqtt: SimpleMessageBox<MqttMessage, MqttMessage>,
    pub http: FakeServerBox<HttpRequest, HttpResult>,
    pub fs: SimpleMessageBox<NoMessage, FsWatchEvent>,
    pub timer: FakeServerBox<SyncStart, SyncComplete>,
    pub ul: FakeServerBox<IdUploadRequest, IdUploadResult>,
    pub dl: FakeServerBox<IdDownloadRequest, IdDownloadResult>,
    pub avail: SimpleMessageBox<MqttMessage, MqttMessage>,
}

pub(crate) async fn spawn_c8y_mapper_actor_with_config(
    tmp_dir: &TempTedgeDir,
    config: C8yMapperConfig,
    init: bool,
) -> TestHandle {
    if init {
        tmp_dir.dir("operations").dir("c8y");
        tmp_dir.dir(".tedge-mapper-c8y");
    }

    let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 10);
    let mut http_builder: FakeServerBoxBuilder<HttpRequest, HttpResult> =
        FakeServerBoxBuilder::default();
    let mut fs_watcher_builder: SimpleMessageBoxBuilder<NoMessage, FsWatchEvent> =
        SimpleMessageBoxBuilder::new("FS", 5);
    let mut uploader_builder: FakeServerBoxBuilder<IdUploadRequest, IdUploadResult> =
        FakeServerBoxBuilder::default();
    let mut downloader_builder: FakeServerBoxBuilder<IdDownloadRequest, IdDownloadResult> =
        FakeServerBoxBuilder::default();
    let mut timer_builder: FakeServerBoxBuilder<SyncStart, SyncComplete> =
        FakeServerBoxBuilder::default();
    let mut service_monitor_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("ServiceMonitor", 1);

    let bridge_health_topic = config.bridge_health_topic.clone();
    C8yMapperBuilder::init(&config).await.unwrap();
    let mut c8y_mapper_builder = C8yMapperBuilder::try_new(
        config,
        &mut mqtt_builder,
        &mut http_builder,
        &mut timer_builder,
        &mut uploader_builder,
        &mut downloader_builder,
        &mut fs_watcher_builder,
        &mut service_monitor_builder,
    )
    .unwrap();

    let mut availability_box_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("Availability", 10);
    availability_box_builder
        .connect_source(AvailabilityBuilder::channels(), &mut c8y_mapper_builder);
    c8y_mapper_builder.connect_source(NoConfig, &mut availability_box_builder);

    let actor = c8y_mapper_builder.build();
    tokio::spawn(async move { actor.run().await });

    let mut service_monitor_box = service_monitor_builder.build();
    let bridge_status_msg = MqttMessage::new(&bridge_health_topic, "1");
    service_monitor_box.send(bridge_status_msg).await.unwrap();

    TestHandle {
        mqtt: mqtt_builder.build(),
        http: http_builder.build(),
        fs: fs_watcher_builder.build(),
        timer: timer_builder.build(),
        ul: uploader_builder.build(),
        dl: downloader_builder.build(),
        avail: availability_box_builder.build(),
    }
}

pub(crate) fn test_mapper_config(tmp_dir: &TempTedgeDir) -> C8yMapperConfig {
    let device_name = "test-device".into();
    let device_topic_id = EntityTopicId::default_main_device();
    let device_type = "test-device-type".into();
    let config = TEdgeConfig::load_toml_str("service.ty = \"service\"");
    let c8y_host = "test.c8y.io".to_owned();
    let tedge_http_host = "localhost:8888".into();
    let mqtt_schema = MqttSchema::default();
    let auth_proxy_addr = "127.0.0.1".into();
    let auth_proxy_port = 8001;
    let bridge_config = BridgeConfig {
        c8y_prefix: TopicPrefix::try_from("c8y").unwrap(),
    };

    let mut topics =
        C8yMapperConfig::default_internal_topic_filter(&"c8y".try_into().unwrap()).unwrap();
    let custom_operation_topics =
        C8yMapperConfig::get_topics_from_custom_operations(tmp_dir.path(), &bridge_config).unwrap();
    topics.add_all(custom_operation_topics);

    let capabilities = Capabilities::default();

    let operation_topics = OperationHandler::topic_filter(&capabilities)
        .into_iter()
        .map(|(e, c)| mqtt_schema.topics(e, c))
        .collect();
    topics.add_all(operation_topics);

    topics.add_all(C8yMapperConfig::default_external_topic_filter());

    topics.remove_overlapping_patterns();

    C8yMapperConfig::new(
        tmp_dir.utf8_path().into(),
        tmp_dir.utf8_path().into(),
        tmp_dir.utf8_path_buf().into(),
        tmp_dir.utf8_path().into(),
        device_name,
        device_topic_id,
        device_type,
        config.service.clone(),
        c8y_host.clone(),
        c8y_host,
        tedge_http_host,
        topics,
        capabilities,
        auth_proxy_addr,
        auth_proxy_port,
        Protocol::Http,
        MqttSchema::default(),
        true,
        true,
        bridge_config,
        false,
        SoftwareManagementApiFlag::Advanced,
        true,
        AutoLogUpload::Never,
        false,
        false,
        C8Y_MQTT_PAYLOAD_LIMIT,
    )
}

pub(crate) async fn skip_init_messages(mqtt: &mut impl MessageReceiver<MqttMessage>) {
    //Skip all the init messages by still doing loose assertions
    assert_received_contains_str(
        mqtt,
        [
            ("te/device/main///twin/c8y_Agent", "{"),
            ("te/device/main///twin/name", "test-device"),
            ("te/device/main///twin/type", "test-device-type"),
            ("c8y/s/us", "114"),
            ("c8y/s/us", "500"),
        ],
    )
    .await;
}

pub(crate) fn spawn_dummy_c8y_http_proxy(mut http: FakeServerBox<HttpRequest, HttpResult>) {
    tokio::spawn(async move {
        while let Some(request) = http.recv().await {
            let uri = request.uri().path();
            eprintln!("C8Y Proxy: {} {uri}", request.method());
            if uri.starts_with("/c8y/inventory/managedObjects/") {
                let _ = http
                    .send(HttpResponseBuilder::new().status(200).build())
                    .await;
            } else if uri == "/c8y/event/events/" {
                let response = C8yEventResponse {
                    id: "dummy-event-id-1234".to_string(),
                };
                let _ = http
                    .send(
                        HttpResponseBuilder::new()
                            .status(200)
                            .json(&response)
                            .build(),
                    )
                    .await;
            } else if let Some(id) = uri.strip_prefix("/c8y/identity/externalIds/c8y_Serial/") {
                let response = InternalIdResponse::new(id, id);
                let _ = http
                    .send(
                        HttpResponseBuilder::new()
                            .status(200)
                            .json(&response)
                            .build(),
                    )
                    .await;
            }
        }
    });
}
