use super::actor::C8yMapperBuilder;
use super::actor::SyncComplete;
use super::actor::SyncStart;
use super::config::C8yMapperConfig;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResult;
use serde_json::json;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Duration;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::MessageReceiver;
use tedge_actors::NoMessage;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
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
    let (mqtt, _http, _fs, _timer) = spawn_c8y_mapper_actor(&cfg_dir, true).await;

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
async fn mapper_publishes_software_update_request() {
    // The test assures SM Mapper correctly receives software update request smartrest message on `c8y/s/ds`
    // and converts it to thin-edge json message published on `tedge/commands/req/software/update`.
    let (mqtt, http, _fs, _timer) = spawn_c8y_mapper_actor(&TempTedgeDir::new(), true).await;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    mqtt.skip(6).await; //Skip all init messages

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
async fn mapper_publishes_software_update_status_onto_c8y_topic() {
    // The test assures SM Mapper correctly receives software update response message on `tedge/commands/res/software/update`
    // and publishes status of the operation `501` on `c8y/s/us`

    // Start SM Mapper
    let (mqtt, http, _fs, _timer) = spawn_c8y_mapper_actor(&TempTedgeDir::new(), true).await;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

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
    let (mqtt, _http, _fs, _timer) = spawn_c8y_mapper_actor(&TempTedgeDir::new(), true).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

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

    let (mqtt, _http, _fs, _timer) = spawn_c8y_mapper_actor(&TempTedgeDir::new(), true).await;

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

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
    let (mqtt, _http, _fs, mut timer) = spawn_c8y_mapper_actor(&TempTedgeDir::new(), true).await;
    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("tedge/alarms/major/temperature_alarm"),
        r#"{ "text": "Temperature high" }"#,
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
    let (mqtt, _http, _fs, mut timer) = spawn_c8y_mapper_actor(&TempTedgeDir::new(), true).await;
    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("tedge/alarms/minor/temperature_high/external_sensor"),
        json!({ "text": "Temperature high" }).to_string(),
    ))
    .await
    .unwrap();

    // Expect child device creation and converted temperature alarm messages
    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us",
                "101,external_sensor,external_sensor,thin-edge.io-child",
            ),
            (
                "c8y/s/us/external_sensor",
                "303,temperature_high,\"Temperature high\"",
            ),
        ],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_with_custom_fragment_mapping_to_c8y_json() {
    let (mqtt, _http, _fs, mut timer) = spawn_c8y_mapper_actor(&TempTedgeDir::new(), true).await;
    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

    mqtt.send(MqttMessage::new(
        &"tedge/alarms/major/custom_temperature_alarm"
            .try_into()
            .unwrap(),
        json!({
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
    let (mqtt, _http, _fs, mut timer) = spawn_c8y_mapper_actor(&TempTedgeDir::new(), true).await;
    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

    mqtt.send(MqttMessage::new(
        &"tedge/alarms/major/custom_temperature_alarm/external_sensor"
            .try_into()
            .unwrap(),
        json!({
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

    // Expect child device creation message
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "101,external_sensor,external_sensor,thin-edge.io-child",
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
                    "externalId":"external_sensor",
                    "type":"c8y_Serial"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_with_message_as_custom_fragment_mapping_to_c8y_json() {
    let (mqtt, _http, _fs, mut timer) = spawn_c8y_mapper_actor(&TempTedgeDir::new(), true).await;
    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

    mqtt.send(MqttMessage::new(
        &"tedge/alarms/major/custom_msg_pressure_alarm"
            .try_into()
            .unwrap(),
        json!({
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
    let (mqtt, _http, _fs, mut timer) = spawn_c8y_mapper_actor(&TempTedgeDir::new(), true).await;
    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

    mqtt.send(MqttMessage::new(
        &"tedge/alarms/major/child_custom_msg_pressure_alarm/external_sensor"
            .try_into()
            .unwrap(),
        json!({
            "text": "Pressure high",
            "time":"2023-01-25T18:41:14.776170774Z",
            "message":"custom message"
        })
        .to_string(),
    ))
    .await
    .unwrap();

    mqtt.skip(1).await; //Skip child device creation message

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
                    "externalId":"external_sensor",
                    "type":"c8y_Serial"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_child_alarm_with_custom_message() {
    let (mqtt, _http, _fs, mut timer) = spawn_c8y_mapper_actor(&TempTedgeDir::new(), true).await;
    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

    mqtt.send(MqttMessage::new(
        &"tedge/alarms/major/child_msg_to_text_pressure_alarm/external_sensor"
            .try_into()
            .unwrap(),
        json!({
            "time":"2023-01-25T18:41:14.776170774Z",
            "message":"Pressure high"
        })
        .to_string(),
    ))
    .await
    .unwrap();

    mqtt.skip(1).await; //Skip child device creation message

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
                    "externalId":"external_sensor",
                    "type":"c8y_Serial"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_with_custom_message() {
    let (mqtt, _http, _fs, mut timer) = spawn_c8y_mapper_actor(&TempTedgeDir::new(), true).await;
    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

    mqtt.send(MqttMessage::new(
        &"tedge/alarms/major/msg_to_text_pressure_alarm"
            .try_into()
            .unwrap(),
        json!({
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
    let (mqtt, _http, _fs, mut timer) = spawn_c8y_mapper_actor(&TempTedgeDir::new(), true).await;
    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("tedge/alarms/major/empty_temperature_alarm/external_sensor"),
        json!({}).to_string(),
    ))
    .await
    .unwrap();

    mqtt.skip(1).await; //Skip child device creation message

    // Expect converted alarm SmartREST message
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us/external_sensor", "302,empty_temperature_alarm")],
    )
    .await;
}

#[tokio::test]
async fn c8y_mapper_alarm_empty_payload() {
    let (mqtt, _http, _fs, mut timer) = spawn_c8y_mapper_actor(&TempTedgeDir::new(), true).await;
    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("tedge/alarms/major/empty_temperature_alarm"),
        json!({}).to_string(),
    ))
    .await
    .unwrap();

    // Expect converted alarm SmartREST message
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "302,empty_temperature_alarm")]).await;
}

#[tokio::test]
async fn c8y_mapper_alarm_complex_text_fragment_in_payload_failed() {
    let (mqtt, _http, _fs, mut timer) = spawn_c8y_mapper_actor(&TempTedgeDir::new(), true).await;
    timer.send(Timeout::new(())).await.unwrap(); //Complete sync phase so that alarm mapping starts
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("tedge/alarms/major/complex_text_alarm"),
        json!({
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
    assert_received_contains_str(&mut mqtt, [("tedge/errors", "Parsing of alarm message received on topic: tedge/alarms/major/complex_text_alarm failed due to error: invalid")]).await;
}

#[tokio::test]
async fn mapper_handles_multiline_sm_requests() {
    // The test assures if Mapper can handle multiline smartrest messages arrived on `c8y/s/ds`
    let (mqtt, http, _fs, _timer) = spawn_c8y_mapper_actor(&TempTedgeDir::new(), true).await;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

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

    let (mqtt, _http, _fs, _timer) = spawn_c8y_mapper_actor(&cfg_dir, false).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    mqtt.skip(1).await;

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

    let (mqtt, _http, _fs, _timer) = spawn_c8y_mapper_actor(&cfg_dir, false).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(),
        "106,child-one",
    ))
    .await
    .expect("Send failed");

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

    let (mqtt, _http, _fs, _timer) = spawn_c8y_mapper_actor(&cfg_dir, false).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(),
        "106,child-one",
    ))
    .await
    .expect("Send failed");

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
async fn mapper_dynamically_updates_supported_operations_for_tedge_device() {
    // The test assures tedge-mapper checks if there are operations, then it reads and
    // correctly publishes them on to `c8y/s/us`.
    // When mapper is running test adds a new operation into the operations directory, then the mapper discovers the new
    // operation and publishes list of supported operation including the new operation, and verifies the device create message.
    let cfg_dir = TempTedgeDir::new();
    create_thin_edge_operations(&cfg_dir, vec!["c8y_TestOp1", "c8y_TestOp2"]);

    let (mqtt, _http, mut fs, _timer) = spawn_c8y_mapper_actor(&cfg_dir, false).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    // Simulate tedge-agent health status message
    mqtt.send(
        MqttMessage::new(
            &Topic::new_unchecked("tedge/health/tedge-agent"),
            "{\"status\":\"up\"}",
        )
        .with_retain(),
    )
    .await
    .expect("Send failed");
    mqtt.skip(8).await; //Skip all init messages

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
        "child1",
        vec!["c8y_ChildTestOp1", "c8y_ChildTestOp2"],
    );

    let (mqtt, _http, mut fs, _timer) = spawn_c8y_mapper_actor(&cfg_dir, false).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    mqtt.skip(6).await; //Skip all init messages

    // Add a new operation for the child device
    // Simulate FsEvent for the creation of a new operation file
    fs.send(FsWatchEvent::FileCreated(
        cfg_dir
            .dir("operations")
            .dir("c8y")
            .dir("child1")
            .file("c8y_ChildTestOp3")
            .to_path_buf(),
    ))
    .await
    .expect("Send failed");

    // Expect smartrest message on `c8y/s/us/child1` with expected payload "114,c8y_ChildTestOp1,c8y_ChildTestOp2,c8y_ChildTestOp3".
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/child1",
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
    let cfg_dir = TempTedgeDir::new();

    let version = env!("CARGO_PKG_VERSION");
    let custom_fragment_content = &json!({
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
    })
    .to_string();
    create_inventroy_json_file_with_content(&cfg_dir, custom_fragment_content);

    let (mqtt, _http, _fs, _timer) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/inventory/managedObjects/update/test-device",
            custom_fragment_content.as_str(),
        )],
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
    let content = r#"#!/usr/bin/bash    
    for i in {1..2}
    do
        sleep 1
    done
    echo "Executed successfully without timeout"
    "#;
    create_custom_cmd(cmd_file.as_path(), content);

    let (mqtt, http, _fs, _timer) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    mqtt.skip(6).await; //Skip all init messages

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
    let content = r#"#!/usr/bin/bash
    for i in {1..2}
    do
        sleep 1
    done
    echo "Successfully Executed"
    "#;
    create_custom_cmd(cmd_file.as_path(), content);

    let (mqtt, http, _fs, _timer) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    mqtt.skip(6).await; //Skip all init messages

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
    let content = r#"#!/usr/bin/bash
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

    let (mqtt, http, _fs, _timer) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    mqtt.skip(6).await; //Skip all init messages

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
    let content = r#"#!/usr/bin/bash
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

    let (mqtt, http, _fs, _timer) = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    mqtt.skip(6).await; //Skip all init messages

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
    let toml_content = toml::Value::Table(map);

    custom_op_file.with_toml_content(toml_content);
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

async fn spawn_c8y_mapper_actor(
    config_dir: &TempTedgeDir,
    init: bool,
) -> (
    SimpleMessageBox<MqttMessage, MqttMessage>,
    SimpleMessageBox<C8YRestRequest, C8YRestResult>,
    SimpleMessageBox<NoMessage, FsWatchEvent>,
    SimpleMessageBox<SyncStart, SyncComplete>,
) {
    let device_name = "test-device".into();
    let device_type = "test-device-type".into();
    let service_type = "service".into();
    let c8y_host = "test.c8y.io".into();

    if init {
        config_dir.dir("operations").dir("c8y");
    }

    let config = C8yMapperConfig::new(
        config_dir.to_path_buf(),
        config_dir.utf8_path_buf(),
        device_name,
        device_type,
        service_type,
        c8y_host,
    );

    let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 10);
    let mut c8y_proxy_builder: SimpleMessageBoxBuilder<C8YRestRequest, C8YRestResult> =
        SimpleMessageBoxBuilder::new("C8Y", 1);
    let mut fs_watcher_builder: SimpleMessageBoxBuilder<NoMessage, FsWatchEvent> =
        SimpleMessageBoxBuilder::new("FS", 5);
    let mut timer_builder: SimpleMessageBoxBuilder<SyncStart, SyncComplete> =
        SimpleMessageBoxBuilder::new("Timer", 5);

    let c8y_mapper_builder = C8yMapperBuilder::try_new(
        config,
        &mut mqtt_builder,
        &mut c8y_proxy_builder,
        &mut timer_builder,
        &mut fs_watcher_builder,
    )
    .unwrap();

    let mut actor = c8y_mapper_builder.build();
    tokio::spawn(async move { actor.run().await });

    (
        mqtt_builder.build(),
        c8y_proxy_builder.build(),
        fs_watcher_builder.build(),
        timer_builder.build(),
    )
}

fn spawn_dummy_c8y_http_proxy(mut http: SimpleMessageBox<C8YRestRequest, C8YRestResult>) {
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
                Some(C8YRestRequest::C8yUpdateSoftwareListResponse(_)) => {
                    let _ = http
                        .send(Ok(c8y_http_proxy::messages::C8YRestResponse::Unit(())))
                        .await;
                }
                _ => {}
            }
        }
    });
}
