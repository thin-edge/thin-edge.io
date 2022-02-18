use crate::core::{converter::Converter, mapper::create_mapper, size_threshold::SizeThreshold};
use c8y_api::{
    http_proxy::{C8YHttpProxy, JwtAuthHttpProxy},
    json_c8y::C8yUpdateSoftwareListResponse,
};
use c8y_smartrest::{
    error::SMCumulocityMapperError, operations::Operations,
    smartrest_deserializer::SmartRestJwtResponse,
};
use mqtt_channel::{Connection, Message, Topic, TopicFilter};
use mqtt_tests::test_mqtt_server::MqttProcessHandler;
use serial_test::serial;
use std::time::Duration;
use test_case::test_case;
use tokio::task::JoinHandle;

use super::converter::{get_child_id_from_topic, CumulocityConverter};

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn mapper_publishes_a_software_list_request() {
    // The test assures the mapper publishes request for software list on `tedge/commands/req/software/list`.
    let broker = mqtt_tests::test_mqtt_broker();

    let mut messages = broker
        .messages_published_on("tedge/commands/req/software/list")
        .await;

    // Start the SM Mapper
    let sm_mapper = start_c8y_mapper(broker.port).await;

    // Expect on `tedge/commands/req/software/list` a software list request.
    mqtt_tests::assert_received_all_expected(&mut messages, TEST_TIMEOUT_MS, &[r#"{"id":"#]).await;

    sm_mapper.unwrap().abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn mapper_publishes_a_supported_operation_and_a_pending_operations_onto_c8y_topic() {
    // The test assures the mapper publishes smartrest messages 114 and 500 on `c8y/s/us` which shall be send over to the cloud if bridge connection exists.
    let broker = mqtt_tests::test_mqtt_broker();
    let mut messages = broker.messages_published_on("c8y/s/us").await;

    // Start SM Mapper
    let sm_mapper = start_c8y_mapper(broker.port).await;

    // Expect both 118 and 500 messages has been received on `c8y/s/us`, if no msg received for the timeout the test fails.
    mqtt_tests::assert_received_all_expected(
        &mut messages,
        TEST_TIMEOUT_MS,
        &["118,software-management\n", "500\n"],
    )
    .await;

    sm_mapper.unwrap().abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn mapper_publishes_software_update_request() {
    // The test assures SM Mapper correctly receives software update request smartrest message on `c8y/s/ds`
    // and converts it to thin-edge json message published on `tedge/commands/req/software/update`.
    let broker = mqtt_tests::test_mqtt_broker();
    let mut messages = broker
        .messages_published_on("tedge/commands/req/software/update")
        .await;

    let sm_mapper = start_c8y_mapper(broker.port).await;

    // Prepare and publish a software update smartrest request on `c8y/s/ds`.
    let smartrest = r#"528,external_id,nodered,1.0.0::debian,,install"#;
    let _ = broker.publish("c8y/s/ds", smartrest).await.unwrap();
    let _ = publish_a_fake_jwt_token(broker).await;

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

    // Expect thin-edge json message on `tedge/commands/req/software/update` with expected payload.
    mqtt_tests::assert_received_all_expected(
        &mut messages,
        TEST_TIMEOUT_MS,
        &["{\"id\":\"", &remove_whitespace(expected_update_list)],
    )
    .await;

    sm_mapper.unwrap().abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn mapper_publishes_software_update_status_onto_c8y_topic() {
    // The test assures SM Mapper correctly receives software update response message on `tedge/commands/res/software/update`
    // and publishes status of the operation `501` on `c8y/s/us`
    let broker = mqtt_tests::test_mqtt_broker();

    let mut messages = broker.messages_published_on("c8y/s/us").await;

    // Start SM Mapper
    let sm_mapper = start_c8y_mapper(broker.port).await;
    let _ = publish_a_fake_jwt_token(broker).await;

    // Prepare and publish a software update status response message `executing` on `tedge/commands/res/software/update`.
    let json_response = r#"{
            "id": "123",
            "status": "executing"
        }"#;

    let _ = broker
        .publish("tedge/commands/res/software/update", json_response)
        .await
        .unwrap();

    // Expect `501` smartrest message on `c8y/s/us`.
    mqtt_tests::assert_received_all_expected(
        &mut messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_SoftwareUpdate\n"],
    )
    .await;

    // Prepare and publish a software update response `successful`.
    let json_response = r#"{
            "id":"123",
            "status":"successful",
            "currentSoftwareList":[
                {"type":"apt","modules":[
                    {"name":"m","url":"https://foobar.io/m.epl"}
                ]}
            ]}"#;

    let _ = broker
        .publish("tedge/commands/res/software/update", json_response)
        .await
        .unwrap();

    // Expect `503` messages with correct payload have been received on `c8y/s/us`, if no msg received for the timeout the test fails.
    mqtt_tests::assert_received_all_expected(
        &mut messages,
        TEST_TIMEOUT_MS,
        &["503,c8y_SoftwareUpdate,\n"],
    )
    .await;

    sm_mapper.unwrap().abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn mapper_publishes_software_update_failed_status_onto_c8y_topic() {
    let broker = mqtt_tests::test_mqtt_broker();
    let mut messages = broker.messages_published_on("c8y/s/us").await;

    // Start SM Mapper
    let sm_mapper = start_c8y_mapper(broker.port).await;
    let _ = publish_a_fake_jwt_token(broker).await;

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

    let _ = broker
        .publish("tedge/commands/res/software/update", json_response)
        .await
        .unwrap();

    // `502` messages with correct payload have been received on `c8y/s/us`, if no msg received for the timeout the test fails.

    mqtt_tests::assert_received_all_expected(
        &mut messages,
        TEST_TIMEOUT_MS,
        &["502,c8y_SoftwareUpdate,\"Partial failure: Couldn\'t install collectd and nginx\"\n"],
    )
    .await;

    sm_mapper.unwrap().abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
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

    // Start SM Mapper
    let sm_mapper = start_c8y_mapper(broker.port).await?;

    // Prepare and publish a software update smartrest request on `c8y/s/ds`.
    let smartrest = r#"528,external_id,nodered,1.0.0::debian,,install"#;
    let _ = broker.publish("c8y/s/ds", smartrest).await.unwrap();
    let _ = publish_a_fake_jwt_token(broker).await;

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
    let _ = broker
        .publish(
            "tedge/commands/res/software/update",
            &remove_whitespace(json_response),
        )
        .await
        .unwrap();

    // Restart SM Mapper
    let sm_mapper = start_c8y_mapper(broker.port).await?;

    // Validate that the mapper process the response and forward it on 'c8y/s/us'
    // Expect init messages followed by a 503 (success)
    mqtt_tests::assert_received_all_expected(
        &mut responses,
        TEST_TIMEOUT_MS,
        &[
            "118,software-management\n",
            "500\n",
            "503,c8y_SoftwareUpdate,\n",
        ],
    )
    .await;

    sm_mapper.abort();
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn mapper_publishes_software_update_request_with_wrong_action() {
    // The test assures SM Mapper correctly receives software update request smartrest message on `c8y/s/ds`
    // Then the SM Mapper finds out that wrong action as part of the update request.
    // Then SM Mapper publishes an operation status message as executing `501,c8y_SoftwareUpdate'
    // Then SM Mapper publishes an operation status message as failed `502,c8y_SoftwareUpdate,Action remove is not recognized. It must be install or delete.` on `c8/s/us`.
    // Then the subscriber that subscribed for messages on `c8/s/us` receives these messages and verifies them.

    let broker = mqtt_tests::test_mqtt_broker();

    // Create a subscriber to receive messages on `c8y/s/us` topic.
    let mut messages = broker.messages_published_on("c8y/s/us").await;

    let _sm_mapper = start_c8y_mapper(broker.port).await;

    // Prepare and publish a c8y_SoftwareUpdate smartrest request on `c8y/s/ds` that contains a wrong action `remove`, that is not known by c8y.
    let smartrest = r#"528,external_id,nodered,1.0.0::debian,,remove"#;
    let _ = broker.publish("c8y/s/ds", smartrest).await.unwrap();

    // Expect a 501 (executing) followed by a 502 (failed)
    mqtt_tests::assert_received_all_expected(
        &mut messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_SoftwareUpdate",
        "502,c8y_SoftwareUpdate,\"Parameter remove is not recognized. It must be install or delete.\""],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
#[ignore]
async fn get_jwt_token_full_run() {
    // Given a background process that publish JWT tokens on demand.
    let broker = mqtt_tests::test_mqtt_broker();
    broker.map_messages_background(|(topic, _)| {
        let mut response = vec![];
        if &topic == "c8y/s/uat" {
            response.push(("c8y/s/dat".into(), "71,1111".into()));
        }
        response
    });

    // An JwtAuthHttpProxy ...
    let mqtt_config = mqtt_channel::Config::default()
        .with_port(broker.port)
        .with_session_name("JWT-Requester-Test")
        .with_subscriptions(TopicFilter::new_unchecked("c8y/s/dat"));
    let mqtt_client = Connection::new(&mqtt_config).await.unwrap();
    let http_client = reqwest::ClientBuilder::new().build().unwrap();
    let mut http_proxy =
        JwtAuthHttpProxy::new(mqtt_client, http_client, "test.tenant.com", "test-device");

    // ... fetches and returns these JWT tokens.
    let jwt_token = http_proxy.get_jwt_token().await;

    // `get_jwt_token` should return `Ok` and the value of token should be as set above `1111`.
    assert!(jwt_token.is_ok());
    assert_eq!(jwt_token.unwrap().token(), "1111");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn c8y_mapper_alarm_mapping_to_smartrest() {
    let broker = mqtt_tests::test_mqtt_broker();

    let mut messages = broker.messages_published_on("c8y/s/us").await;

    // Start the C8Y Mapper
    let c8y_mapper = start_c8y_mapper(broker.port).await.unwrap();

    let _ = broker
        .publish_with_opts(
            "tedge/alarms/major/temperature_alarm",
            r#"{ "message": "Temperature high" }"#,
            mqtt_channel::QoS::AtLeastOnce,
            true,
        )
        .await
        .unwrap();

    // Expect converted temperature alarm message
    mqtt_tests::assert_received_all_expected(
        &mut messages,
        TEST_TIMEOUT_MS,
        &["302,temperature_alarm"],
    )
    .await;

    //Clear the previously published alarm
    let _ = broker
        .publish_with_opts(
            "tedge/alarms/major/temperature_alarm",
            "",
            mqtt_channel::QoS::AtLeastOnce,
            true,
        )
        .await
        .unwrap();

    c8y_mapper.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn c8y_mapper_syncs_pending_alarms_on_startup() {
    let broker = mqtt_tests::test_mqtt_broker();

    let mut messages = broker.messages_published_on("c8y/s/us").await;

    // Start the C8Y Mapper
    let c8y_mapper = start_c8y_mapper(broker.port).await.unwrap();

    let _ = broker
        .publish_with_opts(
            "tedge/alarms/critical/temperature_alarm",
            r#"{ "message": "Temperature very high" }"#,
            mqtt_channel::QoS::AtLeastOnce,
            true,
        )
        .await
        .unwrap();

    // Expect converted temperature alarm message
    mqtt_tests::assert_received_all_expected(
        &mut messages,
        TEST_TIMEOUT_MS,
        &["301,temperature_alarm"],
    )
    .await;

    c8y_mapper.abort();

    //Publish a new alarm while the mapper is down
    let _ = broker
        .publish_with_opts(
            "tedge/alarms/critical/pressure_alarm",
            r#"{ "message": "Pressure very high" }"#,
            mqtt_channel::QoS::AtLeastOnce,
            true,
        )
        .await
        .unwrap();

    // Ignored until the rumqttd broker bug that doesn't handle empty retained messages
    //Clear the existing alarm while the mapper is down
    // let _ = broker
    //     .publish_with_opts(
    //         "tedge/alarms/critical/temperature_alarm",
    //         "",
    //         mqtt_channel::QoS::AtLeastOnce,
    //         true,
    //     )
    //     .await
    //     .unwrap();

    // Restart the C8Y Mapper
    let _ = start_c8y_mapper(broker.port).await.unwrap();

    // Ignored until the rumqttd broker bug that doesn't handle empty retained messages
    // Expect the previously missed clear temperature alarm message
    // let msg = messages
    //     .next()
    //     .with_timeout(ALARM_SYNC_TIMEOUT_MS)
    //     .await
    //     .expect_or("No message received after a second.");
    // dbg!(&msg);
    // assert!(&msg.contains("306,temperature_alarm"));

    // Expect the new pressure alarm message
    mqtt_tests::assert_received_all_expected(
        &mut messages,
        TEST_TIMEOUT_MS,
        &["301,pressure_alarm"],
    )
    .await;

    c8y_mapper.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn test_sync_alarms() {
    let size_threshold = SizeThreshold(16 * 1024);
    let device_name = String::from("test");
    let device_type = String::from("test_type");
    let operations = Operations::new();
    let http_proxy = FakeC8YHttpProxy {};

    let mut converter = CumulocityConverter::new(
        size_threshold,
        device_name,
        device_type,
        operations,
        http_proxy,
    );

    let alarm_topic = "tedge/alarms/critical/temperature_alarm";
    let alarm_payload = r#"{ "message": "Temperature very high" }"#;
    let alarm_message = Message::new(&Topic::new_unchecked(alarm_topic), alarm_payload);

    // During the sync phase, alarms are not converted immediately, but only cached to be synced later
    assert!(converter.convert(&alarm_message).await.is_empty());

    let non_alarm_topic = "tedge/measurements";
    let non_alarm_payload = r#"{"temp": 1}"#;
    let non_alarm_message = Message::new(&Topic::new_unchecked(non_alarm_topic), non_alarm_payload);

    // But non-alarms are converted immediately, even during the sync phase
    assert!(!converter.convert(&non_alarm_message).await.is_empty());

    let internal_alarm_topic = "c8y-internal/alarms/major/pressure_alarm";
    let internal_alarm_payload = r#"{ "message": "Temperature very high" }"#;
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn convert_thin_edge_json_with_child_id() {
    let device_name = String::from("test");
    let device_type = String::from("test");
    let operations = Operations::new();
    let http_proxy = FakeC8YHttpProxy {};

    let mut converter = Box::new(CumulocityConverter::new(
        SizeThreshold(16 * 1024),
        device_name,
        device_type,
        operations,
        http_proxy,
    ));

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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn convert_first_thin_edge_json_invalid_then_valid_with_child_id() {
    let device_name = String::from("test");
    let device_type = String::from("test");
    let operations = Operations::new();
    let http_proxy = FakeC8YHttpProxy {};

    let mut converter = Box::new(CumulocityConverter::new(
        SizeThreshold(16 * 1024),
        device_name,
        device_type,
        operations,
        http_proxy,
    ));

    let in_topic = "tedge/measurements/child1";
    let in_invalid_payload = r#"{"temp": invalid}"#;
    let in_valid_payload = r#"{"temp": 1, "time": "2021-11-16T17:45:40.571760714+01:00"}"#;
    let in_first_message = Message::new(&Topic::new_unchecked(in_topic), in_invalid_payload);
    let in_second_message = Message::new(&Topic::new_unchecked(in_topic), in_valid_payload);

    // First convert invalid Thin Edge JSON message.
    let out_first_messages = converter.convert(&in_first_message).await;
    let expected_error_message = Message::new(
        &Topic::new_unchecked("tedge/errors"),
        r#"Invalid JSON: expected value at line 1 column 10: `invalid}`"#,
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial]
async fn convert_two_thin_edge_json_messages_given_different_child_id() {
    let device_name = String::from("test");
    let device_type = String::from("test");
    let operations = Operations::new();
    let http_proxy = FakeC8YHttpProxy {};

    let mut converter = Box::new(CumulocityConverter::new(
        SizeThreshold(16 * 1024),
        device_name,
        device_type,
        operations,
        http_proxy,
    ));
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

#[test_case("tedge/measurements/test", Some("test".to_string()); "valid child id")]
#[test_case("tedge/measurements/", None; "returns an error (empty value)")]
#[test_case("tedge/measurements", None; "invalid child id (parent topic)")]
#[test_case("foo/bar", None; "invalid child id (invalid topic)")]
fn extract_child_id(in_topic: &str, expected_child_id: Option<String>) {
    match get_child_id_from_topic(in_topic) {
        Ok(maybe_id) => assert_eq!(maybe_id, expected_child_id),
        Err(crate::core::error::ConversionError::InvalidChildId { id }) => {
            assert_eq!(id, "".to_string())
        }
        _ => {
            panic!("Unexpected error type")
        }
    }
}

#[test]
fn check_c8y_threshold_packet_size() -> Result<(), anyhow::Error> {
    let size_threshold = SizeThreshold(16 * 1024);
    let device_name = String::from("test");
    let device_type = String::from("test");
    let operations = Operations::new();
    let http_proxy = FakeC8YHttpProxy {};

    let converter = CumulocityConverter::new(
        size_threshold,
        device_name,
        device_type,
        operations,
        http_proxy,
    );
    let buffer = create_packet(1024 * 20);
    let err = converter.size_threshold.validate(&buffer).unwrap_err();
    assert_eq!(
        err.to_string(),
        "The input size 20480 is too big. The threshold is 16384."
    );
    Ok(())
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

pub struct FakeC8YHttpProxy {}

#[async_trait::async_trait]
impl C8YHttpProxy for FakeC8YHttpProxy {
    async fn init(&mut self) -> Result<(), SMCumulocityMapperError> {
        Ok(())
    }

    fn url_is_in_my_tenant_domain(&self, _url: &str) -> bool {
        true
    }

    async fn get_jwt_token(&mut self) -> Result<SmartRestJwtResponse, SMCumulocityMapperError> {
        Ok(SmartRestJwtResponse::try_new("71,fake-token")?)
    }

    async fn send_software_list_http(
        &mut self,
        _c8y_software_list: &C8yUpdateSoftwareListResponse,
    ) -> Result<(), SMCumulocityMapperError> {
        Ok(())
    }

    async fn upload_log_binary(
        &mut self,
        _log_content: &str,
    ) -> Result<String, SMCumulocityMapperError> {
        Ok("fake/upload/url".into())
    }
}

async fn start_c8y_mapper(mqtt_port: u16) -> Result<JoinHandle<()>, anyhow::Error> {
    let device_name = "test-device".into();
    let device_type = "test-device-type".into();
    let size_threshold = SizeThreshold(16 * 1024);
    let operations = Operations::new();
    let http_proxy = FakeC8YHttpProxy {};

    let converter = Box::new(CumulocityConverter::new(
        size_threshold,
        device_name,
        device_type,
        operations,
        http_proxy,
    ));

    let mut mapper = create_mapper("c8y-mapper-test", mqtt_port, converter).await?;

    let mapper_task = tokio::spawn(async move {
        let _ = mapper.run().await;
    });
    Ok(mapper_task)
}

fn remove_whitespace(s: &str) -> String {
    let mut s = String::from(s);
    s.retain(|c| !c.is_whitespace());
    s
}

async fn publish_a_fake_jwt_token(broker: &MqttProcessHandler) {
    let _ = broker.publish("c8y/s/dat", "71,1111").await.unwrap();
}
