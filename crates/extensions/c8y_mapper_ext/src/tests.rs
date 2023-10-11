use super::actor::C8yMapperBuilder;
use super::actor::SyncComplete;
use super::actor::SyncStart;
use super::config::C8yMapperConfig;
use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::Capabilities;
use assert_json_diff::assert_json_include;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResult;
use c8y_http_proxy::messages::Url;
use serde_json::json;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Duration;
use std::time::SystemTime;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::MessageReceiver;
use tedge_actors::NoMessage;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_actors::WrappedInput;
use tedge_api::mqtt_topics::Channel;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::SoftwareUpdateResponse;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
use tedge_mqtt_ext::test_helpers::assert_received_includes_json;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_test_utils::fs::with_exec_permission;
use tedge_test_utils::fs::TempTedgeDir;
use tedge_timer_ext::Timeout;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

#[tokio::test]
async fn mapper_publishes_init_messages_on_startup() {
    // Start SM Mapper
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    let version = env!("CARGO_PKG_VERSION");
    let default_fragment_content = json!({
        "c8y_Agent": {
            "name": "thin-edge.io",
            "url": "https://thin-edge.io",
            "version": version
        }
    })
    .to_string();

    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/inventory/managedObjects/update/test-device",
                default_fragment_content.as_str(),
            ),
            (
                "te/device/main///twin/c8y_Agent",
                &json!({
                    "name": "thin-edge.io",
                    "url": "https://thin-edge.io",
                    "version": version
                })
                .to_string(),
            ),
            ("c8y/s/us", "114"),
            (
                "c8y/inventory/managedObjects/update/test-device",
                &json!({"type":"test-device-type"}).to_string(),
            ),
            ("c8y/s/us", "500"),
            ("c8y/s/us", "105"),
        ],
    )
    .await;
}

#[tokio::test]
async fn child_device_registration_mapping() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); // Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
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
            "101,test-device:device:child1,Child1,RaspberryPi",
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
            "101,test-device:device:child2,test-device:device:child2,thin-edge.io-child",
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
            "c8y/s/us/test-device:device:child1/test-device:device:child2",
            "101,child3,child3,thin-edge.io-child",
        )],
    )
    .await;
}

#[tokio::test]
async fn custom_topic_scheme_registration_mapping() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); // Complete sync phase so that alarm mapping starts
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
        [("c8y/s/us", "101,test-device:custom,Child1,RaspberryPi")],
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
            "101,test-device:custom:child1,Child1,RaspberryPi",
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
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); // Complete sync phase so that alarm mapping starts
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

    mqtt.skip(2).await; // Skip mappings of above child device creation messages

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
            "c8y/s/us/test-device:device:child1/test-device:device:child2",
            "102,test-device:device:child2:service:collectd,systemd,Collectd,up",
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_publishes_software_update_request() {
    // The test assures SM Mapper correctly receives software update request smartrest message on `c8y/s/ds`
    // and converts it to thin-edge json message published on `tedge/commands/req/software/update`.
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_SoftwareUpdate SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(),
        "528,test-device,nodered,1.0.0::debian,,install",
    ))
    .await
    .expect("Send failed");

    assert_received_includes_json(
        &mut mqtt,
        [(
            "tedge/commands/req/software/update",
            json!({
                "updateList": [
                    {
                        "type": "debian",
                        "modules": [
                            {
                                "name": "nodered",
                                "version": "1.0.0",
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
async fn mapper_publishes_software_update_request_with_new_token() {
    // The test assures SM Mapper correctly receives software update request smartrest message on `c8y/s/ds`
    // and converts it to thin-edge json message published on `tedge/commands/req/software/update` with new JWT token.
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_SoftwareUpdate SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(),
        "528,test-device,test-very-large-software,2.0,https://test.c8y.io,install",
    ))
    .await
    .expect("Send failed");
    let first_request = mqtt.recv().await.unwrap();
    // Simulate c8y_SoftwareUpdate SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(),
        "528,test-device,test-very-large-software,2.0,https://test.c8y.io,install",
    ))
    .await
    .expect("Send failed");
    let second_request = mqtt.recv().await.unwrap();

    // Both software update requests will have different tokens in it. So, they are not equal.
    assert_ne!(first_request, second_request);
}

#[tokio::test]
async fn mapper_publishes_software_update_status_onto_c8y_topic() {
    // The test assures SM Mapper correctly receives software update response message on `tedge/commands/res/software/update`
    // and publishes status of the operation `501` on `c8y/s/us`

    // Start SM Mapper
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Prepare and publish a software update status response message `executing` on `tedge/commands/res/software/update`.
    mqtt.send(MqttMessage::new(
        &SoftwareUpdateResponse::topic(),
        json!({
            "id": "123",
            "status": "executing"
        })
        .to_string(),
    ))
    .await
    .expect("Send failed");

    // Expect `501` smartrest message on `c8y/s/us`.
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_SoftwareUpdate")]).await;

    // Prepare and publish a software update response `successful`.
    mqtt.send(MqttMessage::new(
        &SoftwareUpdateResponse::topic(),
        json!({
            "id":"123",
            "status":"successful",
            "currentSoftwareList":[
                {
                    "type":"apt",
                    "modules":[
                        {
                            "name":"m",
                            "url":"https://foobar.io/m.epl"
                        }
                    ]
                }
            ]
        })
        .to_string(),
    ))
    .await
    .expect("Send failed");

    // Expect `503` messages with correct payload have been received on `c8y/s/us`, if no msg received for the timeout the test fails.
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "503,c8y_SoftwareUpdate")]).await;
}

#[tokio::test]
async fn mapper_publishes_software_update_failed_status_onto_c8y_topic() {
    // Start SM Mapper
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // The agent publish an error
    let json_response = r#"
        {
            "id": "123",
            "status":"failed",
            "reason":"Partial failure: Couldn't install collectd and nginx",
            "currentSoftwareList": [
                {
                    "type": "docker",
                    "modules": [
                        {
                            "name": "nginx",
                            "version": "1.21.0"
                        }
                    ]
                }
            ],
            "failures":[]
        }"#;

    mqtt.send(MqttMessage::new(
        &SoftwareUpdateResponse::topic(),
        json_response,
    ))
    .await
    .expect("Send failed");

    // `502` messages with correct payload have been received on `c8y/s/us`, if no msg received for the timeout the test fails.
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "502,c8y_SoftwareUpdate,\"Partial failure: Couldn\'t install collectd and nginx\"\n",
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_publishes_software_update_request_with_wrong_action() {
    // The test assures SM Mapper correctly receives software update request smartrest message on `c8y/s/ds`
    // Then the SM Mapper finds out that wrong action as part of the update request.
    // Then SM Mapper publishes an operation status message as executing `501,c8y_SoftwareUpdate'
    // Then SM Mapper publishes an operation status message as failed `502,c8y_SoftwareUpdate,Action remove is not recognized. It must be install or delete.` on `c8/s/us`.
    // Then the subscriber that subscribed for messages on `c8/s/us` receives these messages and verifies them.

    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Prepare and publish a c8y_SoftwareUpdate smartrest request on `c8y/s/ds` that contains a wrong action `remove`, that is not known by c8y.
    let smartrest = r#"528,test-device,nodered,1.0.0::debian,,remove"#;
    mqtt.send(MqttMessage::new(&C8yTopic::downstream_topic(), smartrest))
        .await
        .expect("Send failed");

    // Expect a 501 (executing) followed by a 502 (failed)
    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us",
                "501,c8y_SoftwareUpdate",
            ),
            (
                "c8y/s/us",
                "502,c8y_SoftwareUpdate,\"Parameter remove is not recognized. It must be install or delete.\""
            )
        ],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_mapping_to_smartrest() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
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
        [("c8y/s/us", "302,temperature_alarm,\"Temperature high\"")],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_alarm_mapping_to_smartrest() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor///a/temperature_high"),
        json!({ "severity": "minor", "text": "Temperature high" }).to_string(),
    ))
    .await
    .unwrap();

    // Expect auto-registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/external_sensor//",
            json!({
                "@type":"child-device",
                "@id":"test-device:device:external_sensor",
                "name": "external_sensor"
            }),
        )],
    )
    .await;
    // Expect child device creation and converted temperature alarm messages
    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us",
                "101,test-device:device:external_sensor,external_sensor,thin-edge.io-child",
            ),
            (
                "c8y/s/us/test-device:device:external_sensor",
                "303,temperature_high,\"Temperature high\"",
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_with_custom_fragment_mapping_to_c8y_json() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
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
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

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

    // Expect auto-registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/external_sensor//",
            json!({"@type":"child-device","@id":"test-device:device:external_sensor"}),
        )],
    )
    .await;

    // Expect child device creation message
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "101,test-device:device:external_sensor,external_sensor,thin-edge.io-child",
        )],
    )
    .await;

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
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
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
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

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

    mqtt.skip(2).await; //Skip child device creation message

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
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

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

    mqtt.skip(2).await; //Skip child device creation message

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
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
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
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/external_sensor///a/empty_temperature_alarm"),
        "".to_string(),
    ))
    .await
    .unwrap();

    mqtt.skip(2).await; //Skip child device creation message

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
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
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
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
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
        .starts_with(r#"303,empty_temperature_alarm,"empty_temperature_alarm""#));
}

#[tokio::test]
async fn c8y_mapper_child_event() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

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

    // Expect auto-registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/external_sensor//",
            json!({"@type":"child-device","@id":"test-device:device:external_sensor"}),
        )],
    )
    .await;

    // Expect child device creation message
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "101,test-device:device:external_sensor,external_sensor,thin-edge.io-child",
        )],
    )
    .await;

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
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

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

    // Expect auto-registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/external_sensor//",
            json!({"@type":"child-device","@id":"test-device:device:external_sensor"}),
        )],
    )
    .await;

    // Expect child device creation message
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "101,test-device:device:external_sensor")],
    )
    .await;

    // Expect child device service auto registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/external_sensor/service/service_child",
            json!({
               "@id":"test-device:device:external_sensor:service:service_child",
               "@parent":"device/external_sensor//",
               "@type":"service",
               "name":"service_child",
               "type":"service"
            }),
        )],
    )
    .await;

    // Expect child device service creation message
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:external_sensor",
            "102,test-device:device:external_sensor:service:service_child,service,service_child,up",
        )],
    )
    .await;

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
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

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

    // Expect auto-registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/main/service/service_main",
            json!({
                "@type":"service",
                "@parent":"device/main//",
                "@id":"test-device:device:main:service:service_main",
                "type":"service"
            }),
        )],
    )
    .await;

    // Create the service for the main device
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "102,test-device:device:main:service:service_main,service,service_main,up",
        )],
    )
    .await;

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
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

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

    // Expect auto-registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/external_sensor//",
            json!({"@type":"child-device","@id":"test-device:device:external_sensor"}),
        )],
    )
    .await;

    // Expect child device creation message
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "101,test-device:device:external_sensor")],
    )
    .await;

    // Expect child device service auto registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/external_sensor/service/service_child",
            json!({
               "@id":"test-device:device:external_sensor:service:service_child",
               "@parent":"device/external_sensor//",
               "@type":"service",
               "name":"service_child",
               "type":"service"
            }),
        )],
    )
    .await;

    // Expect child device service creation message
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:external_sensor",
            "102,test-device:device:external_sensor:service:service_child,service,service_child,up",
        )],
    )
    .await;

    // Expect converted alarm for the main device service
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:external_sensor:service:service_child",
            r#"301,custom_alarm,"temperature alarm",2023-01-25T18:41:14.776170774Z"#,
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_main_service_alarm() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

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

    // Expect auto-registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/main/service/service_main",
            json!({
                "@type":"service",
                "@parent":"device/main//",
                "@id":"test-device:device:main:service:service_main",
                "type":"service"
            }),
        )],
    )
    .await;

    // Create the service for the main device
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "102,test-device:device:main:service:service_main,service,service_main,up",
        )],
    )
    .await;

    // Expect converted alarm for the main device service
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:main:service:service_main",
            r#"301,custom_alarm,"temperature alarm",2023-01-25T18:41:14.776170774Z"#,
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_complex_text_fragment_in_payload_failed() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
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
    assert_received_contains_str(&mut mqtt, [("tedge/errors", "Parsing of alarm message for the type: complex_text_alarm failed due to error: invalid")]).await;
}

#[tokio::test]
async fn mapper_handles_multiline_sm_requests() {
    // The test assures if Mapper can handle multiline smartrest messages arrived on `c8y/s/ds`
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Prepare and publish multiline software update smartrest requests on `c8y/s/ds`.
    let smartrest = "528,test-device,nodered,1.0.0::debian,,install\n528,test-device,rolldice,2.0.0::debian,,install".to_string();
    mqtt.send(MqttMessage::new(&C8yTopic::downstream_topic(), smartrest))
        .await
        .expect("Send failed");

    assert_received_includes_json(
        &mut mqtt,
        [
            (
                "tedge/commands/req/software/update",
                json!({
                    "updateList": [
                        {
                            "type": "debian",
                            "modules": [
                                {
                                    "name": "nodered",
                                    "version": "1.0.0",
                                    "action": "install"
                                }
                            ]
                        }
                    ]
                }),
            ),
            (
                "tedge/commands/req/software/update",
                json!({
                    "updateList": [
                        {
                            "type": "debian",
                            "modules": [
                                {
                                    "name": "rolldice",
                                    "version": "2.0.0",
                                    "action": "install"
                                }
                            ]
                        }
                    ]
                }),
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn mapper_publishes_supported_operations() {
    // The test assures tede-mapper reads/parses the operations from operations directory and
    // correctly publishes the supported operations message on `c8y/s/us`
    // and verifies the supported operations that are published by the tedge-mapper.
    let cfg_dir = TempTedgeDir::new();
    create_thin_edge_operations(&cfg_dir, vec!["c8y_TestOp1", "c8y_TestOp2"]);

    let (mqtt, _http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, false).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    mqtt.skip(2).await;

    // Expect smartrest message on `c8y/s/us` with expected payload "114,c8y_TestOp1,c8y_TestOp2"
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "114,c8y_TestOp1,c8y_TestOp2")]).await;
}

#[tokio::test]
async fn mapper_publishes_child_device_create_message() {
    // The test assures tedge-mapper checks if there is a directory for operations for child devices, then it reads and
    // correctly publishes the child device create message on to `c8y/s/us`
    // and verifies the device create message.
    let cfg_dir = TempTedgeDir::new();
    create_thin_edge_child_devices(&cfg_dir, vec!["child1"]);

    let (mqtt, _http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, false).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(),
        "106,child-one",
    ))
    .await
    .expect("Send failed");

    // Expect auto-registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/child1//",
            json!({"@type":"child-device", "name": "child1"}),
        )],
    )
    .await;

    // Expect smartrest message on `c8y/s/us` with expected payload "101,child1,child1,thin-edge.io-child".
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "101,child1,child1,thin-edge.io-child")],
    )
    .await;
}

#[tokio::test]
async fn mapper_publishes_supported_operations_for_child_device() {
    // The test assures tedge-mapper checks if there is a directory for operations for child devices, then it reads and
    // correctly publishes supported operations message for that child on to `c8y/s/us/child1`
    // and verifies that message.
    let cfg_dir = TempTedgeDir::new();
    create_thin_edge_child_operations(
        &cfg_dir,
        "child1",
        vec!["c8y_ChildTestOp1", "c8y_ChildTestOp2"],
    );

    let (mqtt, _http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, false).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(),
        "106,child-one",
    ))
    .await
    .expect("Send failed");

    // Expect auto-registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/child1//",
            json!({"@type":"child-device", "@id": "child1", "name": "child1"}),
        )],
    )
    .await;

    // Expect smartrest message on `c8y/s/us/child1` with expected payload "114,c8y_ChildTestOp1,c8y_ChildTestOp2.
    assert_received_contains_str(
        &mut mqtt,
        [
            ("c8y/s/us", "101,child1,child1,thin-edge.io-child"),
            ("c8y/s/us/child1", "114,c8y_ChildTestOp1,c8y_ChildTestOp2\n"),
        ],
    )
    .await;
}

#[tokio::test]
async fn mapping_child_device_dirs_with_forbidden_characters() {
    let cfg_dir = TempTedgeDir::new();
    create_thin_edge_child_operations(&cfg_dir, "my#complex+child", vec!["c8y_ChildTestOp1"]);
    create_thin_edge_child_operations(&cfg_dir, "simple_child", vec!["c8y_ChildTestOp2"]);

    let (mqtt, _http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, false).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(),
        "106", // Empty child list from cloud, to trigger child dirs registration
    ))
    .await
    .expect("Send failed");

    // Expect auto-registration message for child2
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/simple_child//",
            json!({"@type":"child-device", "@id": "simple_child", "name": "simple_child"}),
        )],
    )
    .await;

    // Expect smartrest messages for child 2
    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us",
                "101,simple_child,simple_child,thin-edge.io-child",
            ),
            ("c8y/s/us/simple_child", "114,c8y_ChildTestOp2\n"),
        ],
    )
    .await;

    assert!(mqtt.recv().await.is_none()); // No more messages as my#complex+child is ignored
}

#[tokio::test]
async fn mapper_dynamically_updates_supported_operations_for_tedge_device() {
    // The test assures tedge-mapper checks if there are operations, then it reads and
    // correctly publishes them on to `c8y/s/us`.
    // When mapper is running test adds a new operation into the operations directory, then the mapper discovers the new
    // operation and publishes list of supported operation including the new operation, and verifies the device create message.
    let cfg_dir = TempTedgeDir::new();
    create_thin_edge_operations(&cfg_dir, vec!["c8y_TestOp1", "c8y_TestOp2"]);

    let (mqtt, _http, mut fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, false).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

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

    // Skip tedge-agent registration, health status mapping, and software list request
    mqtt.skip(4).await;

    // Simulate FsEvent for the creation of a new operation file
    fs.send(FsWatchEvent::FileCreated(
        cfg_dir
            .dir("operations")
            .dir("c8y")
            .file("c8y_TestOp3")
            .to_path_buf(),
    ))
    .await
    .expect("Send failed");

    // Expect smartrest message on `c8y/s/us` with expected payload "114,c8y_TestOp1,c8y_TestOp2,c8y_TestOp3".
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "114,c8y_SoftwareUpdate,c8y_TestOp1,c8y_TestOp2,c8y_TestOp3",
        )],
    )
    .await;

    // Then the agent start adding it's own set of capabilities
    mqtt.send(
        MqttMessage::new(&Topic::new_unchecked("te/device/main///cmd/restart"), "{}").with_retain(),
    )
    .await
    .expect("Send failed");

    // Expect an update list of capabilities with agent capabilities
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
    let cfg_dir = TempTedgeDir::new();
    create_thin_edge_child_operations(
        &cfg_dir,
        "test-device:device:child1",
        vec!["c8y_ChildTestOp1", "c8y_ChildTestOp2"],
    );

    let (mqtt, _http, mut fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, false).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Add a new operation for the child device
    // Simulate FsEvent for the creation of a new operation file
    fs.send(FsWatchEvent::FileCreated(
        cfg_dir
            .dir("operations")
            .dir("c8y")
            .dir("test-device:device:child1")
            .file("c8y_ChildTestOp3")
            .to_path_buf(),
    ))
    .await
    .expect("Send failed");

    // Expect smartrest message on `c8y/s/us/child1` with expected payload "114,c8y_ChildTestOp1,c8y_ChildTestOp2,c8y_ChildTestOp3".
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child1",
            "114,c8y_ChildTestOp1,c8y_ChildTestOp2,c8y_ChildTestOp3",
        )],
    )
    .await;

    // Then the agent start on the child device adding it's own set of capabilities
    mqtt.send(
        MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/restart"),
            "{}",
        )
        .with_retain(),
    )
    .await
    .expect("Send failed");

    // Expect auto-registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/child1//",
            json!({"@type":"child-device", "@id": "test-device:device:child1", "name": "child1"}),
        )],
    )
    .await;

    // Expect an update list of capabilities with agent capabilities
    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us",
                "101,test-device:device:child1,child1,thin-edge.io-child",
            ),
            (
                "c8y/s/us/test-device:device:child1",
                "114,c8y_ChildTestOp1,c8y_ChildTestOp2,c8y_ChildTestOp3,c8y_Restart",
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn mapping_dynamically_added_child_device_dir() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, mut fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, false).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;
    let ops_dir = cfg_dir.dir("operations").dir("c8y");

    // Simulate FsEvent for the creation of a new child device following default naming scheme
    fs.send(FsWatchEvent::DirectoryCreated(
        ops_dir.dir("child").to_path_buf(),
    ))
    .await
    .expect("Send failed");

    // Expect auto-registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/child//",
            json!({"@type":"child-device", "@id": "child", "name": "child"}),
        )],
    )
    .await;

    // ...and the corresponding SmartREST registration message
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "101,child,child,thin-edge.io-child")],
    )
    .await;
}

#[tokio::test]
async fn mapping_dynamically_added_child_device_dir_with_default_external_id_naming_scheme() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, mut fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, false).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;
    let ops_dir = cfg_dir.dir("operations").dir("c8y");

    // Simulate FsEvent for the creation of a new child device following default naming scheme
    fs.send(FsWatchEvent::DirectoryCreated(
        ops_dir.dir("test-device:device:child").to_path_buf(),
    ))
    .await
    .expect("Send failed");

    // Expect auto-registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/child//",
            json!({"@type":"child-device", "@id": "test-device:device:child", "name": "child"}),
        )],
    )
    .await;

    // ...and the corresponding SmartREST registration message
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "101,test-device:device:child,child,thin-edge.io-child",
        )],
    )
    .await;
}

#[tokio::test]
async fn mapping_dynamically_added_child_device_dir_with_forbidden_characters() {
    let cfg_dir = TempTedgeDir::new();
    let ops_dir = cfg_dir.dir("operations").dir("c8y");
    let (mqtt, _http, mut fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, false).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Simulate FsEvent for the creation of a new child device with a dir name containing forbidden chars
    let child_dir = ops_dir.dir("my#complex+child");
    fs.send(FsWatchEvent::DirectoryCreated(child_dir.to_path_buf()))
        .await
        .expect("Send failed");

    // No mapped messages as my#complex+child is ignored
    assert!(mqtt.recv().await.is_none());

    // Validate that further directory creation events are still processed
    fs.send(FsWatchEvent::DirectoryCreated(
        ops_dir.dir("simple_child").to_path_buf(),
    ))
    .await
    .expect("Send failed");

    // Expect auto-registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/simple_child//",
            json!({"@type":"child-device", "@id": "simple_child", "name": "simple_child"}),
        )],
    )
    .await;

    // ...and the corresponding SmartREST registration message
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "101,simple_child,simple_child,thin-edge.io-child",
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_updating_the_inventory_fragments_from_file() {
    // The test Creates an inventory file in (Temp_base_Dir)/device/inventory.json
    // The tedge-mapper parses the inventory fragment file and publishes on c8y/inventory/managedObjects/update/test-device
    // Verify the fragment message that is published
    let cfg_dir = TempTedgeDir::new();

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
    create_inventroy_json_file_with_content(&cfg_dir, &custom_fragment_content.to_string());

    let (mqtt, _http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    assert_received_includes_json(
        &mut mqtt,
        [
            (
                "c8y/inventory/managedObjects/update/test-device",
                custom_fragment_content,
            ),
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
            ("te/device/main///twin/numeric_key", json!(10)),
            ("te/device/main///twin/string_key", json!("value")),
        ],
    )
    .await;
}

#[tokio::test]
async fn forbidden_keys_in_inventory_fragments_file_ignored() {
    // The test Creates an inventory file in (Temp_base_Dir)/device/inventory.json
    // The tedge-mapper parses the inventory fragment file and publishes on c8y/inventory/managedObjects/update/test-device
    // Verify the fragment message that is published
    let cfg_dir = TempTedgeDir::new();

    let version = env!("CARGO_PKG_VERSION");
    let custom_fragment_content = json!({
        "name": "new-name",
        "type": "new-name",
        "c8y_Firmware": {
            "name": "raspberrypi-bootloader",
            "url": "31aab9856861b1a587e2094690c2f6e272712cb1",
            "version": "1.20140107-1"
        }
    });
    create_inventroy_json_file_with_content(&cfg_dir, &custom_fragment_content.to_string());

    let (mqtt, _http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    assert_received_includes_json(
        &mut mqtt,
        [
            (
                "c8y/inventory/managedObjects/update/test-device",
                json!({
                    "c8y_Firmware": {
                        "name": "raspberrypi-bootloader",
                        "url": "31aab9856861b1a587e2094690c2f6e272712cb1",
                        "version": "1.20140107-1"
                    }
                }),
            ),
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
        ],
    )
    .await;
}

#[tokio::test]
async fn custom_operation_without_timeout_successful() {
    // The test assures SM Mapper correctly receives custom operation on `c8y/s/ds`
    // and executes the custom operation successfully, no timeout given here.

    let cfg_dir = TempTedgeDir::new();

    let cmd_file = cfg_dir.path().join("command");
    //create custom operation file
    create_custom_op_file(&cfg_dir, cmd_file.as_path(), None, None);
    //create command
    let content = r#"#!/usr/bin/env bash
    for i in {1..2}
    do
        sleep 1
    done
    echo "Executed successfully without timeout"
    "#;
    create_custom_cmd(cmd_file.as_path(), content);

    let (mqtt, http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_Command SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(),
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
exit status: 0

stdout <<EOF
Executed successfully without timeout
EOF

stderr <<EOF
EOF
";

    assert_command_exec_log_content(cfg_dir, expected_content);
}

#[tokio::test]
async fn custom_operation_with_timeout_successful() {
    // The test assures SM Mapper correctly receives custom operation on `c8y/s/ds`
    // and executes the custom operation within the timeout period

    let cfg_dir = TempTedgeDir::new();
    let cmd_file = cfg_dir.path().join("command");
    //create custom operation file
    create_custom_op_file(&cfg_dir, cmd_file.as_path(), Some(4), Some(2));
    //create command
    let content = r#"#!/usr/bin/env bash
    for i in {1..2}
    do
        sleep 1
    done
    echo "Successfully Executed"
    "#;
    create_custom_cmd(cmd_file.as_path(), content);

    let (mqtt, http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_Command SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(),
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
exit status: 0

stdout <<EOF
Successfully Executed
EOF

stderr <<EOF
EOF
";

    assert_command_exec_log_content(cfg_dir, expected_content);
}

#[tokio::test]
async fn custom_operation_timeout_sigterm() {
    // The test assures SM Mapper correctly receives custom operation on `c8y/s/ds`
    // and executes the custom operation, it will timeout because it will not complete before given timeout
    // sigterm is sent to stop the custom operation

    let cfg_dir = TempTedgeDir::new();
    let cmd_file = cfg_dir.path().join("command");
    //create custom operation file
    create_custom_op_file(&cfg_dir, cmd_file.as_path(), Some(1), Some(2));
    //create command
    let content = r#"#!/usr/bin/env bash
    handle_term() {
        for i in {1..1}
        do
            echo "sigterm $i"
            sleep 1
        done
        exit 124
    }
    trap handle_term SIGTERM
    for i in {1..10}
    do
        echo "main $i"
        sleep 1
    done
    "#;
    create_custom_cmd(cmd_file.as_path(), content);

    let (mqtt, http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_Command SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(),
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
            "502,c8y_Command,\"operation failed due to timeout: duration=1s\"",
        )],
    )
    .await;

    // assert the signterm is handled
    let expected_content = "command \"511,test-device,c8y_Command\"
exit status: 124

stdout <<EOF
main 1
sigterm 1
EOF

stderr <<EOF
operation failed due to timeout: duration=1sEOF";

    assert_command_exec_log_content(cfg_dir, expected_content);
}

#[tokio::test]
async fn custom_operation_timeout_sigkill() {
    // The test assures SM Mapper correctly receives custom operation on `c8y/s/ds`
    // and executes the custom operation, it will timeout because it will not complete before given timeout
    // sigterm sent first, still the operation did not stop, so sigkill will be sent to stop the operation

    let cfg_dir = TempTedgeDir::new();

    let cmd_file = cfg_dir.path().join("command");
    //create custom operation file
    create_custom_op_file(&cfg_dir, cmd_file.as_path(), Some(1), Some(2));
    //create command
    let content = r#"#!/usr/bin/env bash
    handle_term() {
        for i in {1..50}
        do
            echo "sigterm $i"
            sleep 1
        done
        exit 124
    }
    trap handle_term SIGTERM
    for i in {1..50}
    do
        echo "main $i"
        sleep 1
    done
    "#;
    create_custom_cmd(cmd_file.as_path(), content);

    let (mqtt, http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_Command SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(),
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
            "502,c8y_Command,\"operation failed due to timeout: duration=1s\"",
        )],
    )
    .await;

    // assert the signterm is handled
    let expected_content = "command \"511,test-device,c8y_Command\"
exit status: unknown

stdout <<EOF
main 1
sigterm 1
sigterm 2
EOF

stderr <<EOF
operation failed due to timeout: duration=1sEOF
";

    assert_command_exec_log_content(cfg_dir, expected_content);
}

/// This test aims to verify that when a telemetry message is emitted from an
/// unknown device or service, the mapper will produce a registration message
/// for this entity. The registration message shall be published only once, when
/// an unknown entity first publishes its message. After that the entity shall
/// be considered registered and no more registration messages for this entity
/// shall be emitted by the mapper.
#[tokio::test]
async fn inventory_registers_unknown_entity_once() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    let measurement_message = MqttMessage::new(
        &Topic::new("te/device/main/service/my_service/m/measurement").unwrap(),
        r#"{"foo":25}"#,
    );

    for _ in 0..5 {
        mqtt.send(measurement_message.clone()).await.unwrap();
    }

    mqtt.close_sender();

    let mut messages = vec![];
    while let Some(WrappedInput::Message(msg)) = mqtt.recv_message().await {
        messages.push(msg);
    }

    // we should not emit a registration message for the main device, only the
    // service
    let mut dut_register_messages: Vec<_> = messages
        .iter()
        .filter(|message| message.topic.name.starts_with("te/device/main/service"))
        .collect();
    let service_register_message = dut_register_messages.remove(0);

    let service_register_payload =
        serde_json::from_slice::<serde_json::Value>(service_register_message.payload_bytes())
            .expect("Service register message payload must be JSON");
    assert_json_include!(
        actual: service_register_payload,
        expected: json!({"@type": "service", "type": "service"})
    );

    assert!(
        !dut_register_messages
            .into_iter()
            .any(|message| message == service_register_message),
        "duplicate registration message"
    );
}

#[tokio::test]
async fn mapper_converts_smartrest_logfile_req_to_log_upload_cmd_for_main_device() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_LogfileRequest SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(),
        "522,test-device,logfileA,2013-06-22T17:03:14.123+02:00,2013-06-23T18:03:14.123+02:00,ERROR,1000",
    ))
        .await
        .expect("Send failed");

    let (topic, received_json) = mqtt
        .recv()
        .await
        .map(|msg| {
            (
                msg.topic,
                serde_json::from_str::<serde_json::Value>(msg.payload.as_str().expect("UTF8"))
                    .expect("JSON"),
            )
        })
        .unwrap();

    let mqtt_schema = MqttSchema::default();
    let (entity, channel) = mqtt_schema.entity_channel_of(&topic).unwrap();
    assert_eq!(entity, "device/main//");

    if let Channel::Command {
        operation: OperationType::LogUpload,
        cmd_id,
    } = channel
    {
        // Validate the topic name
        assert_eq!(
            topic.name,
            format!("te/device/main///cmd/log_upload/{cmd_id}")
        );

        // Validate the payload JSON
        let expected_json = json!({
            "status": "init",
            "tedgeUrl": format!("http://localhost:8888/tedge/file-transfer/test-device/log_upload/logfileA-{cmd_id}"),
            "type": "logfileA",
            "dateFrom": "2013-06-22T17:03:14.123+02:00",
            "dateTo": "2013-06-23T18:03:14.123+02:00",
            "searchText": "ERROR",
            "lines": 1000
        });

        assert_json_diff::assert_json_include!(actual: received_json, expected: expected_json);
    } else {
        panic!("Unexpected response on channel: {:?}", topic)
    }
}

#[tokio::test]
async fn mapper_converts_smartrest_logfile_req_to_log_upload_cmd_for_child_device() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate log_upload cmd metadata message
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/DeviceSerial///cmd/log_upload"),
        r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
    ))
    .await
    .expect("Send failed");

    mqtt.skip(3).await; //Skip entity registration, mapping and supported log types messages

    // Simulate c8y_LogfileRequest SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(),
        "522,test-device:device:DeviceSerial,logfileA,2013-06-22T17:03:14.123+02:00,2013-06-23T18:03:14.123+02:00,ERROR,1000",
    ))
    .await
    .expect("Send failed");

    let (topic, received_json) = mqtt
        .recv()
        .await
        .map(|msg| {
            (
                msg.topic,
                serde_json::from_str::<serde_json::Value>(msg.payload.as_str().expect("UTF8"))
                    .expect("JSON"),
            )
        })
        .unwrap();

    let mqtt_schema = MqttSchema::default();
    let (entity, channel) = mqtt_schema.entity_channel_of(&topic).unwrap();
    assert_eq!(entity, "device/DeviceSerial//");

    if let Channel::Command {
        operation: OperationType::LogUpload,
        cmd_id,
    } = channel
    {
        // Validate the topic name
        assert_eq!(
            topic.name,
            format!("te/device/DeviceSerial///cmd/log_upload/{cmd_id}")
        );

        // Validate the payload JSON
        let expected_json = json!({
            "status": "init",
            "tedgeUrl": format!("http://localhost:8888/tedge/file-transfer/test-device:device:DeviceSerial/log_upload/logfileA-{cmd_id}"),
            "type": "logfileA",
            "dateFrom": "2013-06-22T17:03:14.123+02:00",
            "dateTo": "2013-06-23T18:03:14.123+02:00",
            "searchText": "ERROR",
            "lines": 1000
        });

        assert_json_diff::assert_json_include!(actual: received_json, expected: expected_json);
    } else {
        panic!("Unexpected response on channel: {:?}", topic)
    }
}

#[tokio::test]
async fn mapper_converts_log_upload_cmd_to_supported_op_and_types_for_main_device() {
    let ttd = TempTedgeDir::new();
    let (mqtt, _http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate log_upload cmd metadata message
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/log_upload"),
        r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
    ))
    .await
    .expect("Send failed");

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "118,typeA,typeB,typeC")]).await;

    // Validate if the supported operation file is created
    assert!(ttd
        .path()
        .join("operations/c8y/c8y_LogfileRequest")
        .exists());
}

#[tokio::test]
async fn mapper_converts_log_upload_cmd_to_supported_op_and_types_for_child_device() {
    let ttd = TempTedgeDir::new();
    let (mqtt, _http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate log_upload cmd metadata message
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1///cmd/log_upload"),
        r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
    ))
    .await
    .expect("Send failed");

    // Expect auto-registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/child1//",
            json!({"@type":"child-device","@id":"test-device:device:child1"}),
        )],
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "101,test-device:device:child1,child1,thin-edge.io-child",
        )],
    )
    .await;
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child1",
            "118,typeA,typeB,typeC",
        )],
    )
    .await;

    // Validate if the supported operation file is created
    assert!(ttd
        .path()
        .join("operations/c8y/test-device:device:child1/c8y_LogfileRequest")
        .exists());
}

#[tokio::test]
async fn handle_log_upload_executing_and_failed_cmd_for_main_device() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Simulate log_upload command with "executing" state
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/log_upload/1234"),
        json!({
            "status": "executing",
            "tedgeUrl": format!("http://localhost:8888/tedge/file-transfer/main/log_upload/typeA-1234"),
            "type": "typeA",
            "dateFrom": "2013-06-22T17:03:14.123+02:00",
            "dateTo": "2013-06-23T18:03:14.123+02:00",
            "searchText": "ERROR",
            "lines": 1000
        })
            .to_string(),
    ))
        .await
        .expect("Send failed");

    // Expect `501` smartrest message on `c8y/s/us`.
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_LogfileRequest")]).await;

    // Simulate log_upload command with "failed" state
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/log_upload/1234"),
        json!({
            "status": "failed",
            "tedgeUrl": format!("http://localhost:8888/tedge/file-transfer/main/log_upload/typeA-1234"),
            "type": "typeA",
            "dateFrom": "2013-06-22T17:03:14.123+02:00",
            "dateTo": "2013-06-23T18:03:14.123+02:00",
            "searchText": "ERROR",
            "lines": 1000,
            "reason": "Something went wrong"
        })
            .to_string(),
    ))
        .await
        .expect("Send failed");

    // Expect `502` smartrest message on `c8y/s/us`.
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "502,c8y_LogfileRequest,\"Something went wrong\"",
        )],
    )
    .await;
}

#[tokio::test]
async fn handle_log_upload_executing_and_failed_cmd_for_child_device() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Simulate log_upload command with "executing" state
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1///cmd/log_upload/1234"),
        json!({
            "status": "executing",
            "tedgeUrl": format!("http://localhost:8888/tedge/file-transfer/child1/log_upload/typeA-1234"),
            "type": "typeA",
            "dateFrom": "2013-06-22T17:03:14.123+02:00",
            "dateTo": "2013-06-23T18:03:14.123+02:00",
            "searchText": "ERROR",
            "lines": 1000
        })
            .to_string(),
    ))
        .await
        .expect("Send failed");

    // Expect auto-registration message
    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/child1//",
            json!({"@type":"child-device","@id":"test-device:device:child1"}),
        )],
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "101,test-device:device:child1,child1,thin-edge.io-child",
        )],
    )
    .await;

    // Expect `501` smartrest message on `c8y/s/us/child1`.
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child1",
            "501,c8y_LogfileRequest",
        )],
    )
    .await;

    // Simulate log_upload command with "failed" state
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1///cmd/log_upload/1234"),
        json!({
            "status": "failed",
            "tedgeUrl": format!("http://localhost:8888/tedge/file-transfer/child1/log_upload/typeA-1234"),
            "type": "typeA",
            "dateFrom": "2013-06-22T17:03:14.123+02:00",
            "dateTo": "2013-06-23T18:03:14.123+02:00",
            "searchText": "ERROR",
            "lines": 1000,
            "reason": "Something went wrong"
        })
            .to_string(),
    ))
        .await
        .expect("Send failed");

    // Expect `502` smartrest message on `c8y/s/us/child1`.
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child1",
            "502,c8y_LogfileRequest,\"Something went wrong\"",
        )],
    )
    .await;
}

#[tokio::test]
async fn handle_log_upload_successful_cmd_for_main_device() {
    let ttd = TempTedgeDir::new();
    let (mqtt, http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Simulate a log file is uploaded to the file transfer repository
    ttd.dir("tedge")
        .dir("file-transfer")
        .dir("main")
        .dir("log_upload")
        .file("typeA-1234");

    // Simulate log_upload command with "executing" state
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/log_upload/1234"),
        json!({
            "status": "successful",
            "tedgeUrl": format!("http://localhost:8888/tedge/file-transfer/main/log_upload/typeA-1234"),
            "type": "typeA",
            "dateFrom": "2013-06-22T17:03:14.123+02:00",
            "dateTo": "2013-06-23T18:03:14.123+02:00",
            "searchText": "ERROR",
            "lines": 1000
        })
            .to_string(),
    ))
        .await
        .expect("Send failed");

    // Expect `503` smartrest message on `c8y/s/us`.
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "503,c8y_LogfileRequest,http://c8y-binary.url")],
    )
    .await;
}

#[tokio::test]
async fn handle_log_upload_successful_cmd_for_child_device() {
    let ttd = TempTedgeDir::new();
    let (mqtt, http, _fs, _timer, _dl) = spawn_c8y_mapper_actor(&ttd, true).await;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Simulate a log file is uploaded to the file transfer repository
    ttd.dir("tedge")
        .dir("file-transfer")
        .dir("child1")
        .dir("log_upload")
        .file("typeA-1234");

    // Simulate log_upload command with "executing" state
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/child1///cmd/log_upload/1234"),
        json!({
            "status": "successful",
            "tedgeUrl": format!("http://localhost:8888/tedge/file-transfer/child1/log_upload/typeA-1234"),
            "type": "typeA",
            "dateFrom": "2013-06-22T17:03:14.123+02:00",
            "dateTo": "2013-06-23T18:03:14.123+02:00",
            "searchText": "ERROR",
            "lines": 1000
        })
            .to_string(),
    ))
        .await
        .expect("Send failed");

    mqtt.skip(2).await; // Skip child device registration messages

    // Expect `503` smartrest message on `c8y/s/us`.
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child1",
            "503,c8y_LogfileRequest,http://c8y-binary.url",
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_nested_child_alarm_mapping_to_smartrest() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
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
                "101,immediate_child,immediate_child,thin-edge.io-child",
            ),
            (
                "c8y/s/us/immediate_child",
                "101,nested_child,nested_child,thin-edge.io-child",
            ),
            (
                "c8y/s/us/nested_child",
                "303,ThinEdgeAlarm,\"Temperature high\",2023-10-13T15:00:07.172674353Z",
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_nested_child_event_mapping_to_smartrest() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
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
                "101,immediate_child,immediate_child,thin-edge.io-child",
            ),
            (
                "c8y/s/us/immediate_child",
                "101,nested_child,nested_child,thin-edge.io-child",
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
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
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
            "303,ThinEdgeAlarm,\"Temperature high\",2023-10-13T15:00:07.172674353Z",
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_nested_child_service_event_mapping_to_smartrest() {
    let cfg_dir = TempTedgeDir::new();
    let (mqtt, _http, _fs, mut timer, _dl) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
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

fn assert_command_exec_log_content(cfg_dir: TempTedgeDir, expected_contents: &str) {
    let paths = fs::read_dir(cfg_dir.to_path_buf().join("tedge/agent")).unwrap();
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
    cfg_dir: &TempTedgeDir,
    cmd_file: &Path,
    graceful_timeout: Option<i64>,
    forceful_timeout: Option<i64>,
) {
    let custom_op_file = cfg_dir.dir("operations").dir("c8y").file("c8y_Command");
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

fn create_inventroy_json_file_with_content(cfg_dir: &TempTedgeDir, content: &str) {
    let file = cfg_dir.dir("device").file("inventory.json");
    file.with_raw_content(content);
}

fn create_thin_edge_operations(cfg_dir: &TempTedgeDir, ops: Vec<&str>) {
    let p1 = cfg_dir.dir("operations");
    let tedge_ops_dir = p1.dir("c8y");
    for op in ops {
        tedge_ops_dir.file(op);
    }
}

fn create_thin_edge_child_devices(cfg_dir: &TempTedgeDir, children: Vec<&str>) {
    let tedge_ops_dir = cfg_dir.dir("operations").dir("c8y");
    for child in children {
        tedge_ops_dir.dir(child);
    }
}

fn create_thin_edge_child_operations(cfg_dir: &TempTedgeDir, child_id: &str, ops: Vec<&str>) {
    let p1 = cfg_dir.dir("operations");
    let tedge_ops_dir = p1.dir("c8y");
    let child_ops_dir = tedge_ops_dir.dir(child_id);
    for op in ops {
        child_ops_dir.file(op);
    }
}

pub(crate) async fn spawn_c8y_mapper_actor(
    config_dir: &TempTedgeDir,
    init: bool,
) -> (
    SimpleMessageBox<MqttMessage, MqttMessage>,
    SimpleMessageBox<C8YRestRequest, C8YRestResult>,
    SimpleMessageBox<NoMessage, FsWatchEvent>,
    SimpleMessageBox<SyncStart, SyncComplete>,
    SimpleMessageBox<IdDownloadRequest, IdDownloadResult>,
) {
    if init {
        config_dir.dir("operations").dir("c8y");
    }

    let device_name = "test-device".into();
    let device_topic_id = EntityTopicId::default_main_device();
    let device_type = "test-device-type".into();
    let service_type = "service".into();
    let c8y_host = "test.c8y.io".into();
    let tedge_http_host = "localhost:8888".into();
    let mqtt_schema = MqttSchema::default();
    let auth_proxy_addr = [127, 0, 0, 1].into();
    let auth_proxy_port = 8001;
    let mut topics = C8yMapperConfig::default_internal_topic_filter(config_dir.path()).unwrap();
    topics.add_all(crate::log_upload::log_upload_topic_filter(&mqtt_schema));
    topics.add_all(crate::config_operations::config_snapshot_topic_filter(
        &mqtt_schema,
    ));
    topics.add_all(crate::config_operations::config_update_topic_filter(
        &mqtt_schema,
    ));
    topics.add_all(C8yMapperConfig::default_external_topic_filter());

    let config = C8yMapperConfig::new(
        config_dir.to_path_buf(),
        config_dir.utf8_path_buf(),
        config_dir.utf8_path_buf().into(),
        device_name,
        device_topic_id,
        device_type,
        service_type,
        c8y_host,
        tedge_http_host,
        topics,
        Capabilities::default(),
        auth_proxy_addr,
        auth_proxy_port,
    );

    let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 10);
    let mut c8y_proxy_builder: SimpleMessageBoxBuilder<C8YRestRequest, C8YRestResult> =
        SimpleMessageBoxBuilder::new("C8Y", 1);
    let mut fs_watcher_builder: SimpleMessageBoxBuilder<NoMessage, FsWatchEvent> =
        SimpleMessageBoxBuilder::new("FS", 5);
    let mut downloader_builder: SimpleMessageBoxBuilder<IdDownloadRequest, IdDownloadResult> =
        SimpleMessageBoxBuilder::new("Downloader", 5);
    let mut timer_builder: SimpleMessageBoxBuilder<SyncStart, SyncComplete> =
        SimpleMessageBoxBuilder::new("Timer", 5);

    let c8y_mapper_builder = C8yMapperBuilder::try_new(
        config,
        &mut mqtt_builder,
        &mut c8y_proxy_builder,
        &mut timer_builder,
        &mut downloader_builder,
        &mut fs_watcher_builder,
    )
    .unwrap();

    let actor = c8y_mapper_builder.build();
    tokio::spawn(async move { actor.run().await });

    (
        mqtt_builder.build(),
        c8y_proxy_builder.build(),
        fs_watcher_builder.build(),
        timer_builder.build(),
        downloader_builder.build(),
    )
}

pub(crate) async fn skip_init_messages(mqtt: &mut impl MessageReceiver<MqttMessage>) {
    //Skip all the init messages by still doing loose assertions
    assert_received_contains_str(
        mqtt,
        [
            ("c8y/inventory/managedObjects/update/test-device", "{"),
            ("te/device/main///twin/c8y_Agent", "{"),
            ("c8y/s/us", "114"),
            (
                "c8y/inventory/managedObjects/update/test-device",
                &json!({"type":"test-device-type"}).to_string(),
            ),
            ("c8y/s/us", "500"),
            ("c8y/s/us", "105"),
        ],
    )
    .await;
}

pub(crate) fn spawn_dummy_c8y_http_proxy(
    mut http: SimpleMessageBox<C8YRestRequest, C8YRestResult>,
) {
    tokio::spawn(async move {
        loop {
            match http.recv().await {
                Some(C8YRestRequest::GetJwtToken(_)) => {
                    let _ = http
                        .send(Ok(c8y_http_proxy::messages::C8YRestResponse::EventId(
                            "dummy-token".into(),
                        )))
                        .await;
                }
                Some(C8YRestRequest::GetFreshJwtToken(_)) => {
                    let now = SystemTime::now();
                    let _ = http
                        .send(Ok(c8y_http_proxy::messages::C8YRestResponse::EventId(
                            format!("dummy-token-{:?}", now),
                        )))
                        .await;
                }
                Some(C8YRestRequest::SoftwareListResponse(_)) => {
                    let _ = http
                        .send(Ok(c8y_http_proxy::messages::C8YRestResponse::Unit(())))
                        .await;
                }
                Some(C8YRestRequest::UploadFile(_)) => {
                    let _ = http
                        .send(Ok(c8y_http_proxy::messages::C8YRestResponse::Url(Url(
                            "http://c8y-binary.url".into(),
                        ))))
                        .await;
                }
                _ => {}
            }
        }
    });
}
