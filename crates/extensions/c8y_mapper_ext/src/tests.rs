use super::actor::C8yMapperBuilder;
use super::actor::SyncComplete;
use super::actor::SyncStart;
use super::config::C8yMapperConfig;
use crate::actor::IdDownloadRequest;
use crate::actor::IdDownloadResult;
use crate::actor::IdUploadRequest;
use crate::actor::IdUploadResult;
use crate::actor::PublishMessage;
use crate::availability::AvailabilityBuilder;
use crate::Capabilities;
use assert_json_diff::assert_json_include;
use c8y_api::json_c8y_deserializer::C8yDeviceControlTopic;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_auth_proxy::url::Protocol;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResult;
use serde_json::json;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Duration;
use std::time::SystemTime;
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
use tedge_api::CommandStatus;
use tedge_api::SoftwareUpdateCommand;
use tedge_config::AutoLogUpload;
use tedge_config::SoftwareManagementApiFlag;
use tedge_config::TEdgeConfig;
use tedge_file_system_ext::FsWatchEvent;
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
            "101,test-device:device:child1,Child1,RaspberryPi",
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
            "101,test-device:device:child2,test-device:device:child2,thin-edge.io-child",
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
            "c8y/s/us/test-device:device:child1/test-device:device:child2",
            "101,child3,child3,thin-edge.io-child",
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
            "c8y/s/us/test-device:device:child1/test-device:device:child2",
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
async fn mapper_publishes_advanced_software_list() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate software_list request
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/software_list/c8y-mapper-1234"),
        json!({
        "id":"1",
        "status":"successful",
        "currentSoftwareList":[
            {"type":"debian", "modules":[
                {"name":"a"},
                {"name":"b","version":"1.0"},
                {"name":"c","url":"https://foobar.io/c.deb"},
                {"name":"d","version":"beta","url":"https://foobar.io/d.deb"}
            ]},
            {"type":"apama","modules":[
                {"name":"m","url":"https://foobar.io/m.epl"}
            ]}
        ]})
        .to_string(),
    ))
    .await
    .expect("Send failed");

    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us",
                "140,a,,debian,,b,1.0,debian,,c,,debian,https://foobar.io/c.deb,d,beta,debian,https://foobar.io/d.deb,m,,apama,https://foobar.io/m.epl"
            )
        ])
        .await;
}

#[tokio::test]
async fn mapper_publishes_software_update_request() {
    // The test assures c8y mapper correctly receives software update request from JSON over MQTT
    // and converts it to thin-edge json message published on `te/device/main///cmd/software_update/+`.
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    skip_init_messages(&mut mqtt).await;

    // Simulate c8y_SoftwareUpdate JSON over MQTT request
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
async fn mapper_publishes_software_update_status_onto_c8y_topic() {
    // The test assures SM Mapper correctly receives software update response message on `te/device/main///cmd/software_update/123`
    // and publishes status of the operation `501` on `c8y/s/us`

    // Start SM Mapper
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Prepare and publish a software update status response message `executing` on `te/device/main///cmd/software_update/123`.
    let mqtt_schema = MqttSchema::default();
    let device = EntityTopicId::default_main_device();
    let request = SoftwareUpdateCommand::new(&device, "c8y-mapper-123".to_string());
    let response = request.with_status(CommandStatus::Executing);
    mqtt.send(response.command_message(&mqtt_schema))
        .await
        .expect("Send failed");

    // Expect `501` smartrest message on `c8y/s/us`.
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_SoftwareUpdate")]).await;

    // Prepare and publish a software update response `successful`.
    let response = response.with_status(CommandStatus::Successful);
    mqtt.send(response.command_message(&mqtt_schema))
        .await
        .expect("Send failed");

    // Expect `503` messages with correct payload have been received on `c8y/s/us`, if no msg received for the timeout the test fails.
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "503,c8y_SoftwareUpdate")]).await;

    // The successful state is cleared
    assert_received_contains_str(
        &mut mqtt,
        [("te/device/main///cmd/software_update/c8y-mapper-123", "")],
    )
    .await;

    // An updated list of software is requested
    assert_received_contains_str(
        &mut mqtt,
        [(
            "te/device/main///cmd/software_list/+",
            r#"{"status":"init"}"#,
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_publishes_software_update_failed_status_onto_c8y_topic() {
    // Start SM Mapper
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, .. } = test_handle;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // The agent publish an error
    let mqtt_schema = MqttSchema::default();
    let device = EntityTopicId::default_main_device();
    let response = SoftwareUpdateCommand::new(&device, "c8y-mapper-123".to_string())
        .with_error("Partial failure: Couldn't install collectd and nginx".to_string());
    mqtt.send(response.command_message(&mqtt_schema))
        .await
        .expect("Send failed");

    // `502` messages with correct payload have been received on `c8y/s/us`, if no msg received for the timeout the test fails.
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "502,c8y_SoftwareUpdate,Partial failure: Couldn't install collectd and nginx",
        )],
    )
    .await;

    // The failed state is cleared
    assert_received_contains_str(
        &mut mqtt,
        [("te/device/main///cmd/software_update/c8y-mapper-123", "")],
    )
    .await;

    // An updated list of software is requested
    assert_received_contains_str(
        &mut mqtt,
        [(
            "te/device/main///cmd/software_list/+",
            r#"{"status":"init"}"#,
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_publishes_software_update_request_with_wrong_action() {
    // The test assures c8y-mapper correctly receives software update request via JSON over MQTT
    // Then the c8y-mapper finds out that wrong action as part of the update request.
    // Then c8y-mapper publishes an operation status message as executing `501,c8y_SoftwareUpdate'
    // Then c8y-mapper publishes an operation status message as failed `502,c8y_SoftwareUpdate,Action remove is not recognized. It must be install or delete.` on `c8/s/us`.
    // Then the subscriber that subscribed for messages on `c8/s/us` receives these messages and verifies them.

    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, .. } = test_handle;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    skip_init_messages(&mut mqtt).await;

    // Publish a c8y_SoftwareUpdate via JSON over MQTT that contains a wrong action `remove`, that is not known by c8y.
    mqtt.send(MqttMessage::new(
        &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
        json!({
            "id": "123456",
            "c8y_SoftwareUpdate": [
                {
                    "name": "nodered",
                    "action": "remove",
                    "version": "1.0.0::debian"
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
                "502,c8y_SoftwareUpdate,Parameter remove is not recognized. It must be install or delete."
            )
        ],
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
                "303,temperature_high,Temperature high",
            ),
        ],
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
    // The test assures tede-mapper reads/parses the operations from operations directory and
    // correctly publishes the supported operations message on `c8y/s/us`
    // and verifies the supported operations that are published by the tedge-mapper.
    let ttd = TempTedgeDir::new();
    create_thin_edge_operations(&ttd, vec!["c8y_TestOp1", "c8y_TestOp2"]);

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, .. } = test_handle;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    mqtt.skip(2).await;

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

    // Skip tedge-agent registration, health status mapping
    mqtt.skip(2).await;

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
    let TestHandle { mqtt, .. } = test_handle;
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
            ("c8y/s/us", "101,child1,child1,thin-edge.io-child"),
            ("c8y/s/us/child1", "101,child11,child11,thin-edge.io-child"),
        ],
    )
    .await;

    // Send any command metadata message to trigger the supported operations update
    mqtt.send(
        MqttMessage::new(
            &Topic::new_unchecked("te/device/child11///cmd/c8y_ChildTestOp3"),
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
            "114,c8y_ChildTestOp1,c8y_ChildTestOp2,c8y_ChildTestOp3",
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
    create_inventroy_json_file_with_content(&ttd, &custom_fragment_content.to_string());

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, .. } = test_handle;
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
    let ttd = TempTedgeDir::new();

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
    create_inventroy_json_file_with_content(&ttd, &custom_fragment_content.to_string());

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, .. } = test_handle;
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

/// This test aims to verify that when a telemetry message is emitted from an
/// unknown device or service, the mapper will produce a registration message
/// for this entity. The registration message shall be published only once, when
/// an unknown entity first publishes its message. After that the entity shall
/// be considered registered and no more registration messages for this entity
/// shall be emitted by the mapper.
#[tokio::test]
async fn inventory_registers_unknown_entity_once() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, .. } = test_handle;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    let measurement_message = MqttMessage::new(
        &Topic::new("te/device/main/service/my_service/m/measurement").unwrap(),
        r#"{"foo":25}"#,
    );

    for _ in 0..5 {
        mqtt.send(measurement_message.clone()).await.unwrap();
    }

    let mut messages = vec![];
    while let Ok(Some(msg)) = mqtt.try_recv().await {
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
                "101,immediate_child,immediate_child,thin-edge.io-child",
            ),
            (
                "c8y/s/us/immediate_child",
                "101,nested_child,nested_child,thin-edge.io-child",
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

    let mut fts_server = mockito::Server::new();
    let _mock = fts_server
        .mock(
            "GET",
            "/tedge/file-transfer/test-device/config_snapshot/c8y-mapper-1234",
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
            "tedgeUrl": format!("http://{host_port}/tedge/file-transfer/test-device/log_upload/c8y-mapper-1234"),
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
            "tedgeUrl": format!("http://{host_port}/tedge/file-transfer/test-device/config_snapshot/c8y-mapper-1234"),
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

fn create_inventroy_json_file_with_content(ttd: &TempTedgeDir, content: &str) {
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
    pub http: FakeServerBox<C8YRestRequest, C8YRestResult>,
    pub fs: SimpleMessageBox<NoMessage, FsWatchEvent>,
    pub timer: FakeServerBox<SyncStart, SyncComplete>,
    pub ul: FakeServerBox<IdUploadRequest, IdUploadResult>,
    pub dl: FakeServerBox<IdDownloadRequest, IdDownloadResult>,
    pub avail: SimpleMessageBox<MqttMessage, PublishMessage>,
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
    let mut c8y_proxy_builder: FakeServerBoxBuilder<C8YRestRequest, C8YRestResult> =
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
    let mut c8y_mapper_builder = C8yMapperBuilder::try_new(
        config,
        &mut mqtt_builder,
        &mut c8y_proxy_builder,
        &mut timer_builder,
        &mut uploader_builder,
        &mut downloader_builder,
        &mut fs_watcher_builder,
        &mut service_monitor_builder,
    )
    .unwrap();

    let mut availability_box_builder: SimpleMessageBoxBuilder<MqttMessage, PublishMessage> =
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
        http: c8y_proxy_builder.build(),
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
    let c8y_host = "test.c8y.io".into();
    let tedge_http_host = "localhost:8888".into();
    let mqtt_schema = MqttSchema::default();
    let auth_proxy_addr = "127.0.0.1".into();
    let auth_proxy_port = 8001;
    let mut topics =
        C8yMapperConfig::default_internal_topic_filter(tmp_dir.path(), &"c8y".try_into().unwrap())
            .unwrap();
    topics.add_all(crate::operations::log_upload::log_upload_topic_filter(
        &mqtt_schema,
    ));
    topics.add_all(crate::operations::config_snapshot::topic_filter(
        &mqtt_schema,
    ));
    topics.add_all(crate::operations::config_update::topic_filter(&mqtt_schema));
    topics.add_all(C8yMapperConfig::default_external_topic_filter());

    C8yMapperConfig::new(
        tmp_dir.utf8_path().into(),
        tmp_dir.utf8_path().into(),
        tmp_dir.utf8_path_buf().into(),
        tmp_dir.utf8_path().into(),
        device_name,
        device_topic_id,
        device_type,
        config.service.clone(),
        c8y_host,
        tedge_http_host,
        topics,
        Capabilities::default(),
        auth_proxy_addr,
        auth_proxy_port,
        Protocol::Http,
        MqttSchema::default(),
        true,
        true,
        "c8y".try_into().unwrap(),
        false,
        SoftwareManagementApiFlag::Advanced,
        true,
        AutoLogUpload::Never,
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
        ],
    )
    .await;
}

pub(crate) fn spawn_dummy_c8y_http_proxy(mut http: FakeServerBox<C8YRestRequest, C8YRestResult>) {
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
                Some(C8YRestRequest::CreateEvent(_)) => {
                    let _ = http
                        .send(Ok(c8y_http_proxy::messages::C8YRestResponse::EventId(
                            "dummy-event-id-1234".to_string(),
                        )))
                        .await;
                }
                _ => {}
            }
        }
    });
}
