use super::actor::C8yMapperBuilder;
use super::actor::SyncComplete;
use super::actor::SyncStart;
use super::config::C8yMapperConfig;
use super::converter::CumulocityConverter;
use crate::core::converter::Converter;
use crate::core::error::ConversionError;
use crate::core::size_threshold::SizeThresholdExceededError;
use anyhow::Result;
use assert_json_diff::assert_json_include;
use assert_matches::assert_matches;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::handle::C8YHttpProxy;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResult;
use mqtt_channel::Message;
use mqtt_channel::Topic;
use serde_json::json;
use std::time::Duration;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::LoggingSender;
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
            ("tedge/commands/req/software/list", r#"{"id":"#),
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

/*
#[tokio::test]
#[ignore]
async fn mapper_fails_during_sw_update_recovers_and_process_response() -> Result<(), anyhow::Error>
{
    // The test assures recovery and processing of messages by the SM-Mapper when it fails in the middle of the operation.
    let broker = mqtt_tests::test_mqtt_broker();

    // When a software update request message is received on `c8y/s/ds` by the sm mapper,
    // converts it to thin-edge json message, publishes a request message on `tedge/commands/req/software/update`.
    // SM Mapper fails before receiving the response message for the request.
    // Meanwhile the operation response message was published on `tedge/commands/res/software/update`.
    // Now the SM Mapper recovers and receives the response message and publishes the status on `c8y/s/us`.
    // The subscriber that was waiting for the response on `c8/s/us` receives the response and validates it.

    // Create a subscriber to receive messages on `tedge/commands/req/software/update` topic.
    let mut requests = broker
        .messages_published_on("tedge/commands/req/software/update")
        .await;

    // Create a subscriber to receive messages on `"c8y/s/us` topic.
    let mut responses = broker.messages_published_on("c8y/s/us").await;

    let cfg_dir = TempTedgeDir::new();
    // Start SM Mapper
    let (_tmp_dir, sm_mapper) = start_c8y_mapper(broker.port, &cfg_dir).await.unwrap();

    // Prepare and publish a software update smartrest request on `c8y/s/ds`.
    let smartrest = r#"528,test-device,nodered,1.0.0::debian,,install"#;
    broker.publish("c8y/s/ds", smartrest).await.unwrap();
    publish_a_fake_jwt_token(broker).await;

    let expected_update_list = r#"
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
            }"#;

    // Wait for the request being published by the mapper on `tedge/commands/req/software/update`.
    mqtt_tests::assert_received_all_expected(
        &mut requests,
        TEST_TIMEOUT_MS,
        &[&remove_whitespace(expected_update_list)],
    )
    .await;

    // Stop the SM Mapper (simulating a failure)
    sm_mapper.abort();
    assert!(sm_mapper.await.unwrap_err().is_cancelled());

    // Let the agent publish the response `successful`.
    let json_response = r#"{
         "id":"123",
         "status":"successful",
         "currentSoftwareList":[
            {
                "type":"apt",
                "modules": [
                    {
                        "name":"m",
                        "url":"https://foobar.io/m.epl"
                    }
                ]
            }
        ]}"#;
    broker
        .publish(
            "tedge/commands/res/software/update",
            &remove_whitespace(json_response),
        )
        .await
        .unwrap();
    let cfg_dir = TempTedgeDir::new();
    // Restart SM Mapper
    let (_tmp_dir, sm_mapper) = start_c8y_mapper(broker.port, &cfg_dir).await.unwrap();

    // Validate that the mapper process the response and forward it on 'c8y/s/us'
    // Expect init messages followed by a 503 (success)
    mqtt_tests::assert_received_all_expected(
        &mut responses,
        TEST_TIMEOUT_MS,
        &["500\n", "503,c8y_SoftwareUpdate,\n"],
    )
    .await;

    sm_mapper.abort();
    Ok(())
}
*/

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
async fn test_sync_alarms() {
    let (_temp_dir, mut converter, _http_proxy) = create_c8y_converter().await;

    let alarm_topic = "tedge/alarms/critical/temperature_alarm";
    let alarm_payload = r#"{ "text": "Temperature very high" }"#;
    let alarm_message = Message::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

    // During the sync phase, alarms are not converted immediately, but only cached to be synced later
    assert!(converter.convert(&alarm_message).await.is_empty());

    let non_alarm_topic = "tedge/measurements";
    let non_alarm_payload = r#"{"temp": 1}"#;
    let non_alarm_message = Message::new(&Topic::new_unchecked(non_alarm_topic), non_alarm_payload);

    // But non-alarms are converted immediately, even during the sync phase
    assert!(!converter.convert(&non_alarm_message).await.is_empty());

    let internal_alarm_topic = "c8y-internal/alarms/major/pressure_alarm";
    let internal_alarm_payload = r#"{ "text": "Temperature very high" }"#;
    let internal_alarm_message = Message::new(
        &Topic::new_unchecked(internal_alarm_topic),
        internal_alarm_payload,
    );

    // During the sync phase, internal alarms are not converted, but only cached to be synced later
    assert!(converter.convert(&internal_alarm_message).await.is_empty());

    // When sync phase is complete, all pending alarms are returned
    let sync_messages = converter.sync_messages();
    assert_eq!(sync_messages.len(), 2);

    // The first message will be clear alarm message for pressure_alarm
    let alarm_message = sync_messages.get(0).unwrap();
    assert_eq!(
        alarm_message.topic.name,
        "tedge/alarms/major/pressure_alarm"
    );
    assert_eq!(alarm_message.payload_bytes().len(), 0); //Clear messages are empty messages

    // The second message will be the temperature_alarm
    let alarm_message = sync_messages.get(1).unwrap();
    assert_eq!(alarm_message.topic.name, alarm_topic);
    assert_eq!(alarm_message.payload_str().unwrap(), alarm_payload);

    // After the sync phase, the conversion of both non-alarms as well as alarms are done immediately
    assert!(!converter.convert(alarm_message).await.is_empty());
    assert!(!converter.convert(&non_alarm_message).await.is_empty());

    // But, even after the sync phase, internal alarms are not converted and just ignored, as they are purely internal
    assert!(converter.convert(&internal_alarm_message).await.is_empty());
}

#[tokio::test]
async fn test_sync_child_alarms() {
    let (_temp_dir, mut converter, _http_proxy) = create_c8y_converter().await;

    let alarm_topic = "tedge/alarms/critical/temperature_alarm/external_sensor";
    let alarm_payload = r#"{ "text": "Temperature very high" }"#;
    let alarm_message = Message::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

    // During the sync phase, alarms are not converted immediately, but only cached to be synced later
    assert!(converter.convert(&alarm_message).await.is_empty());

    let non_alarm_topic = "tedge/measurements/external_sensor";
    let non_alarm_payload = r#"{"temp": 1}"#;
    let non_alarm_message = Message::new(&Topic::new_unchecked(non_alarm_topic), non_alarm_payload);

    // But non-alarms are converted immediately, even during the sync phase
    assert!(!converter.convert(&non_alarm_message).await.is_empty());

    let internal_alarm_topic = "c8y-internal/alarms/major/pressure_alarm/external_sensor";
    let internal_alarm_payload = r#"{ "text": "Temperature very high" }"#;
    let internal_alarm_message = Message::new(
        &Topic::new_unchecked(internal_alarm_topic),
        internal_alarm_payload,
    );

    // During the sync phase, internal alarms are not converted, but only cached to be synced later
    assert!(converter.convert(&internal_alarm_message).await.is_empty());

    // When sync phase is complete, all pending alarms are returned
    let sync_messages = converter.sync_messages();
    assert_eq!(sync_messages.len(), 2);

    // The first message will be clear alarm message for pressure_alarm
    let alarm_message = sync_messages.get(0).unwrap();
    assert_eq!(
        alarm_message.topic.name,
        "tedge/alarms/major/pressure_alarm/external_sensor"
    );
    assert_eq!(alarm_message.payload_bytes().len(), 0); //Clear messages are empty messages

    // The second message will be the temperature_alarm
    let alarm_message = sync_messages.get(1).unwrap();
    assert_eq!(alarm_message.topic.name, alarm_topic);
    assert_eq!(alarm_message.payload_str().unwrap(), alarm_payload);

    // After the sync phase, the conversion of both non-alarms as well as alarms are done immediately
    assert!(!converter.convert(alarm_message).await.is_empty());
    assert!(!converter.convert(&non_alarm_message).await.is_empty());

    // But, even after the sync phase, internal alarms are not converted and just ignored, as they are purely internal
    assert!(converter.convert(&internal_alarm_message).await.is_empty());
}

#[tokio::test]
async fn convert_measurement_with_child_id() {
    let (_temp_dir, mut converter, _http_proxy) = create_c8y_converter().await;

    let in_topic = "tedge/measurements/child1";
    let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
    let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);

    let expected_smart_rest_message = Message::new(
        &Topic::new_unchecked("c8y/s/us"),
        "101,child1,child1,thin-edge.io-child",
    );
    let expected_c8y_json_message = Message::new(
        &Topic::new_unchecked("c8y/measurement/measurements/create"),
        r#"{"type":"ThinEdgeMeasurement","externalSource":{"externalId":"child1","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00"}"#,
    );

    // Test the first output messages contains SmartREST and C8Y JSON.
    let out_first_messages = converter.convert(&in_message).await;
    assert_eq!(
        out_first_messages,
        vec![
            expected_smart_rest_message,
            expected_c8y_json_message.clone()
        ]
    );

    // Test the second output messages doesn't contain SmartREST child device creation.
    let out_second_messages = converter.convert(&in_message).await;
    assert_eq!(out_second_messages, vec![expected_c8y_json_message]);
}

#[tokio::test]
async fn convert_first_measurement_invalid_then_valid_with_child_id() {
    let (_temp_dir, mut converter, _http_proxy) = create_c8y_converter().await;

    let in_topic = "tedge/measurements/child1";
    let in_invalid_payload = r#"{"temp": invalid}"#;
    let in_valid_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
    let in_first_message = Message::new(&Topic::new_unchecked(in_topic), in_invalid_payload);
    let in_second_message = Message::new(&Topic::new_unchecked(in_topic), in_valid_payload);

    // First convert invalid Thin Edge JSON message.
    let out_first_messages = converter.convert(&in_first_message).await;
    let expected_error_message = Message::new(
        &Topic::new_unchecked("tedge/errors"),
        "Invalid JSON: expected value at line 1 column 10: `invalid}\n`",
    );
    assert_eq!(out_first_messages, vec![expected_error_message]);

    // Second convert valid Thin Edge JSON message.
    let out_second_messages = converter.convert(&in_second_message).await;
    let expected_smart_rest_message = Message::new(
        &Topic::new_unchecked("c8y/s/us"),
        "101,child1,child1,thin-edge.io-child",
    );
    let expected_c8y_json_message = Message::new(
        &Topic::new_unchecked("c8y/measurement/measurements/create"),
        r#"{"type":"ThinEdgeMeasurement","externalSource":{"externalId":"child1","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00"}"#,
    );
    assert_eq!(
        out_second_messages,
        vec![expected_smart_rest_message, expected_c8y_json_message]
    );
}

#[tokio::test]
async fn convert_two_measurement_messages_given_different_child_id() {
    let (_temp_dir, mut converter, _http_proxy) = create_c8y_converter().await;
    let in_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;

    // First message from "child1"
    let in_first_message = Message::new(
        &Topic::new_unchecked("tedge/measurements/child1"),
        in_payload,
    );
    let out_first_messages = converter.convert(&in_first_message).await;
    let expected_first_smart_rest_message = Message::new(
        &Topic::new_unchecked("c8y/s/us"),
        "101,child1,child1,thin-edge.io-child",
    );
    let expected_first_c8y_json_message = Message::new(
        &Topic::new_unchecked("c8y/measurement/measurements/create"),
        r#"{"type":"ThinEdgeMeasurement","externalSource":{"externalId":"child1","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00"}"#,
    );
    assert_eq!(
        out_first_messages,
        vec![
            expected_first_smart_rest_message,
            expected_first_c8y_json_message
        ]
    );

    // Second message from "child2"
    let in_second_message = Message::new(
        &Topic::new_unchecked("tedge/measurements/child2"),
        in_payload,
    );
    let out_second_messages = converter.convert(&in_second_message).await;
    let expected_second_smart_rest_message = Message::new(
        &Topic::new_unchecked("c8y/s/us"),
        "101,child2,child2,thin-edge.io-child",
    );
    let expected_second_c8y_json_message = Message::new(
        &Topic::new_unchecked("c8y/measurement/measurements/create"),
        r#"{"type":"ThinEdgeMeasurement","externalSource":{"externalId":"child2","type":"c8y_Serial"},"temp":{"temp":{"value":1.0}},"time":"2021-11-16T17:45:40.571760714+01:00"}"#,
    );
    assert_eq!(
        out_second_messages,
        vec![
            expected_second_smart_rest_message,
            expected_second_c8y_json_message
        ]
    );
}

#[tokio::test]
async fn check_c8y_threshold_packet_size() -> Result<(), anyhow::Error> {
    let (_temp_dir, mut converter, _http_proxy) = create_c8y_converter().await;

    let alarm_topic = "tedge/alarms/critical/temperature_alarm";
    let big_alarm_text = create_packet(1024 * 20);
    let alarm_payload = json!({ "text": big_alarm_text }).to_string();
    let alarm_message = Message::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

    assert_matches!(
        converter.try_convert(&alarm_message).await,
        Err(ConversionError::SizeThresholdExceeded(
            SizeThresholdExceededError {
                size: _,
                threshold: _
            }
        ))
    );
    Ok(())
}

#[tokio::test]
async fn convert_event_with_known_fields_to_c8y_smartrest() -> Result<()> {
    let (_temp_dir, mut converter, _http_proxy) = create_c8y_converter().await;
    let event_topic = "tedge/events/click_event";
    let event_payload = r#"{ "text": "Someone clicked", "time": "2020-02-02T01:02:03+05:30" }"#;
    let event_message = Message::new(&Topic::new_unchecked(event_topic), event_payload);

    let converted_events = converter.convert(&event_message).await;
    assert_eq!(converted_events.len(), 1);
    let converted_event = converted_events.get(0).unwrap();
    assert_eq!(converted_event.topic.name, "c8y/s/us");

    assert_eq!(
        converted_event.payload_str()?,
        r#"400,click_event,"Someone clicked",2020-02-02T01:02:03+05:30"#
    );

    Ok(())
}

#[tokio::test]
async fn convert_event_with_extra_fields_to_c8y_json() -> Result<()> {
    let (_temp_dir, mut converter, _http_proxy) = create_c8y_converter().await;
    let event_topic = "tedge/events/click_event";
    let event_payload = r#"{ "text": "tick", "foo": "bar" }"#;
    let event_message = Message::new(&Topic::new_unchecked(event_topic), event_payload);

    let converted_events = converter.convert(&event_message).await;
    assert_eq!(converted_events.len(), 1);
    let converted_event = converted_events.get(0).unwrap();
    assert_eq!(converted_event.topic.name, "c8y/event/events/create");
    let converted_c8y_json = json!({
        "type": "click_event",
        "text": "tick",
        "foo": "bar",
    });
    assert_eq!(converted_event.topic.name, "c8y/event/events/create");
    assert_json_include!(
        actual: serde_json::from_str::<serde_json::Value>(converted_event.payload_str()?)?,
        expected: converted_c8y_json
    );

    Ok(())
}

#[tokio::test]
async fn test_convert_big_event() {
    let (_temp_dir, mut converter, mut http_proxy) = create_c8y_converter().await;
    tokio::spawn(async move {
        if let Some(C8YRestRequest::C8yCreateEvent(_)) = http_proxy.recv().await {
            let _ = http_proxy
                .send(Ok(c8y_http_proxy::messages::C8YRestResponse::EventId(
                    "event-id".into(),
                )))
                .await;
        }
    });

    let event_topic = "tedge/events/click_event";
    let big_event_text = create_packet((16 + 1) * 1024); // Event payload > size_threshold
    let big_event_payload = json!({ "text": big_event_text }).to_string();
    let big_event_message = Message::new(&Topic::new_unchecked(event_topic), big_event_payload);

    println!("{:?}", converter.convert(&big_event_message).await);
    // assert!(converter.convert(&big_event_message).await.is_empty());
}

#[tokio::test]
async fn test_convert_big_measurement() {
    let (_temp_dir, mut converter, _http_proxy) = create_c8y_converter().await;
    let measurement_topic = "tedge/measurements";
    let big_measurement_payload = create_thin_edge_measurement(10 * 1024); // Measurement payload > size_threshold after converting to c8y json

    let big_measurement_message = Message::new(
        &Topic::new_unchecked(measurement_topic),
        big_measurement_payload,
    );
    let result = converter.convert(&big_measurement_message).await;

    let payload = result[0].payload_str().unwrap();
    assert!(payload.starts_with(
        r#"The payload {"temperature0":0,"temperature1":1,"temperature10" received on tedge/measurements after translation is"#
    ));
    assert!(payload.ends_with("greater than the threshold size of 16184."));
}

#[tokio::test]
async fn test_convert_small_measurement() {
    let (_temp_dir, mut converter, _http_proxy) = create_c8y_converter().await;
    let measurement_topic = "tedge/measurements";
    let big_measurement_payload = create_thin_edge_measurement(20); // Measurement payload size is 20 bytes

    let big_measurement_message = Message::new(
        &Topic::new_unchecked(measurement_topic),
        big_measurement_payload,
    );

    let result = converter.convert(&big_measurement_message).await;

    assert!(result[0].payload_str().unwrap().contains(
        r#"{"type":"ThinEdgeMeasurement","temperature0":{"temperature0":{"value":0.0}}"#
    ));
}

#[tokio::test]
async fn test_convert_big_measurement_for_child_device() {
    let (_temp_dir, mut converter, _http_proxy) = create_c8y_converter().await;
    let measurement_topic = "tedge/measurements/child1";
    let big_measurement_payload = create_thin_edge_measurement(10 * 1024); // Measurement payload > size_threshold after converting to c8y json

    let big_measurement_message = Message::new(
        &Topic::new_unchecked(measurement_topic),
        big_measurement_payload,
    );

    let result = converter.convert(&big_measurement_message).await;

    let payload = result[0].payload_str().unwrap();
    assert!(payload.starts_with(
        r#"The payload {"temperature0":0,"temperature1":1,"temperature10" received on tedge/measurements/child1 after translation is"#
    ));
    assert!(payload.ends_with("greater than the threshold size of 16184."));
}

#[tokio::test]
async fn test_convert_small_measurement_for_child_device() {
    let measurement_topic = "tedge/measurements/child1";
    let big_measurement_payload = create_thin_edge_measurement(20); // Measurement payload size is 20 bytes

    let big_measurement_message = Message::new(
        &Topic::new_unchecked(measurement_topic),
        big_measurement_payload,
    );
    let (_temp_dir, mut converter, _http_proxy) = create_c8y_converter().await;
    let result = converter.convert(&big_measurement_message).await;

    let payload1 = &result[0].payload_str().unwrap();
    let payload2 = &result[1].payload_str().unwrap();

    assert!(payload1.contains("101,child1,child1,thin-edge.io-child"));
    assert!(payload2 .contains(
        r#"{"type":"ThinEdgeMeasurement","externalSource":{"externalId":"child1","type":"c8y_Serial"},"temperature0":{"temperature0":{"value":0.0}},"#
    ));
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
    mqtt.skip(6).await; //Skip all init messages

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
        [("c8y/s/us", "114,c8y_TestOp1,c8y_TestOp2,c8y_TestOp3")],
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
async fn translate_service_monitor_message_for_child_device() {
    let (_temp_dir, mut converter, _http_proxy) = create_c8y_converter().await;

    let in_topic = "tedge/health/child1/child-service-c8y";
    let in_payload = r#"{"pid":"1234","status":"up","time":"2021-11-16T17:45:40.571760714+01:00","type":"thin-edge.io"}"#;
    let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);

    let expected_child_create_smart_rest_message = Message::new(
        &Topic::new_unchecked("c8y/s/us"),
        "101,child1,child1,thin-edge.io-child",
    );

    let expected_service_monitor_smart_rest_message = Message::new(
        &Topic::new_unchecked("c8y/s/us/child1"),
        r#"102,test-device_child1_child-service-c8y,"thin-edge.io",child-service-c8y,"up""#,
    );

    // Test the first output messages contains SmartREST and C8Y JSON.
    let out_first_messages = converter.convert(&in_message).await;

    assert_eq!(
        out_first_messages,
        vec![
            expected_child_create_smart_rest_message,
            expected_service_monitor_smart_rest_message.clone()
        ]
    );
}

#[tokio::test]
async fn translate_service_monitor_message_for_thin_edge_device() {
    let (_temp_dir, mut converter, _http_proxy) = create_c8y_converter().await;

    let in_topic = "tedge/health/test-tedge-mapper-c8y";
    let in_payload = r#"{"pid":"1234","status":"up","time":"2021-11-16T17:45:40.571760714+01:00","type":"thin-edge.io"}"#;
    let in_message = Message::new(&Topic::new_unchecked(in_topic), in_payload);

    let expected_service_monitor_smart_rest_message = Message::new(
        &Topic::new_unchecked("c8y/s/us"),
        r#"102,test-device_test-tedge-mapper-c8y,"thin-edge.io",test-tedge-mapper-c8y,"up""#,
    );

    // Test the output messages contains SmartREST and C8Y JSON.
    let out_messages = converter.convert(&in_message).await;

    assert_eq!(
        out_messages,
        vec![expected_service_monitor_smart_rest_message]
    );
}

fn create_inventroy_json_file_with_content(cfg_dir: &TempTedgeDir, content: &str) {
    let file = cfg_dir.dir("device").file("inventory.json");
    file.with_raw_content(content);
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

async fn create_c8y_converter() -> (
    TempTedgeDir,
    CumulocityConverter,
    SimpleMessageBox<C8YRestRequest, C8YRestResult>,
) {
    let device_id = "test-device".into();
    let device_type = "test-device-type".into();
    let service_type = "service".into();
    let c8y_host = "test.c8y.io".into();

    let tmp_dir = TempTedgeDir::new();
    tmp_dir.dir("operations").dir("c8y");

    let config = C8yMapperConfig::new(
        tmp_dir.to_path_buf(),
        tmp_dir.utf8_path_buf(),
        device_id,
        device_type,
        service_type,
        c8y_host,
    );

    let mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 5);
    let mqtt_publisher = LoggingSender::new("MQTT".into(), mqtt_builder.build().sender_clone());

    let mut c8y_proxy_builder: SimpleMessageBoxBuilder<C8YRestRequest, C8YRestResult> =
        SimpleMessageBoxBuilder::new("C8Y", 1);
    let http_proxy = C8YHttpProxy::new("C8Y", &mut c8y_proxy_builder);

    let converter = CumulocityConverter::new(config, mqtt_publisher, http_proxy).unwrap();

    (tmp_dir, converter, c8y_proxy_builder.build())
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

    let c8y_mapper_builder = C8yMapperBuilder::new(
        config,
        &mut mqtt_builder,
        &mut c8y_proxy_builder,
        &mut timer_builder,
        &mut fs_watcher_builder,
    );

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
