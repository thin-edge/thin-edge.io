use crate::sm_c8y_mapper::mapper::CumulocitySoftwareManagement;
use mqtt_client::{Client, MqttClient, MqttMessageStream, Topic, TopicFilter};
use serial_test::serial;
use std::{io::Write, time::Duration};
use tedge_config::{ConfigRepository, TEdgeConfig, TEdgeConfigLocation};
use tokio::task::JoinHandle;

const MQTT_TEST_PORT: u16 = 55555;
const TEST_TIMEOUT_MS: Duration = Duration::from_millis(2000);

#[tokio::test]
#[cfg_attr(not(feature = "mosquitto-available"), ignore)]
#[serial]
async fn mapper_publishes_a_software_list_request() {
    // The test assures the mapper publishes request for software list on `tedge/commands/req/software/list`.

    // Create a subscriber to receive messages on the bus.
    let mut subscriber = get_subscriber(
        "tedge/commands/req/software/list",
        "mapper_publishes_a_software_list_request",
    )
    .await;

    // Start SM Mapper
    let sm_mapper = start_sm_mapper().await;

    // Expect message that arrives on `tedge/commands/req/software/list` is software list request.

    // Expect `501` smartrest message on `c8y/s/us`.
    for _ in 0..5 {
        // Loop 5 times, because it needs time to receive the messages
        match tokio::time::timeout(TEST_TIMEOUT_MS, subscriber.next()).await {
            Ok(Some(msg)) => {
                dbg!(&msg.payload_str().unwrap());
                if msg.payload_str().unwrap().contains("{\"id\":\"") {
                    assert!(&msg.payload_str().unwrap().contains("{\"id\":\""));
                    break;
                } else {
                    continue;
                }
            }
            _ => panic!("No message received after a second."),
        }
    }
    sm_mapper.unwrap().abort();
}

#[tokio::test]
#[cfg_attr(not(feature = "mosquitto-available"), ignore)]
#[serial]
async fn mapper_publishes_a_supported_operation_and_a_pending_operations_onto_c8y_topic() {
    // The test assures the mapper publishes smartrest messages 114 and 500 on `c8y/s/us` which shall be send over to the cloud if bridge connection exists.

    // Create a subscriber to receive messages on `c8y/s/us` topic.
    let mut c8y_subscriber = get_subscriber(
        "c8y/s/us",
        "mapper_publishes_a_supported_operation_and_a_pending_operations_onto_c8y_topic",
    )
    .await;

    // Start SM Mapper
    let sm_mapper = start_sm_mapper().await;

    // Expect both 114 and 500 messages has been received on `c8y/s/us`, if no msg received for the timeout the test fails.
    let mut received_supported_operation = false;
    let mut received_pending_operation_request = false;
    for _ in 0..2 {
        match tokio::time::timeout(TEST_TIMEOUT_MS, c8y_subscriber.next()).await {
            Ok(Some(msg)) => {
                dbg!(&msg.payload_str().unwrap());
                match msg.payload_str().unwrap() {
                    "114,c8y_SoftwareUpdate\n" => received_supported_operation = true,
                    "500\n" => received_pending_operation_request = true,
                    _ => {}
                }
                if received_supported_operation && received_pending_operation_request {
                    break;
                }
                continue;
            }
            _ => panic!("No message received after a second."),
        }
    }
    sm_mapper.unwrap().abort();
    assert!(received_supported_operation);
    assert!(received_pending_operation_request);
}

#[tokio::test]
#[cfg_attr(not(feature = "mosquitto-available"), ignore)]
#[serial]
async fn mapper_publishes_software_update_request() {
    // The test assures SM Mapper correctly receives software update request smartrest message on `c8y/s/ds`
    // and converts it to thin-edge json message published on `tedge/commands/req/software/update`.

    // Create a subscriber to receive messages on `c8y/s/us` topic.
    let mut subscriber = get_subscriber(
        "tedge/commands/req/software/update",
        "mapper_publishes_software_update_request",
    )
    .await;

    let sm_mapper = start_sm_mapper().await;
    let _ = publish_a_fake_jwt_token().await;

    // Prepare and publish a software update smartrest request on `c8y/s/ds`.
    let smartrest = r#"528,external_id,nodered,1.0.0::debian,,install"#;
    let _ = publish(&Topic::new("c8y/s/ds").unwrap(), smartrest.to_string()).await;

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
    match tokio::time::timeout(TEST_TIMEOUT_MS, subscriber.next()).await {
        Ok(Some(msg)) => {
            dbg!(&msg.payload_str().unwrap());
            assert!(&msg.payload_str().unwrap().contains("{\"id\":\""));
            assert!(&msg
                .payload_str()
                .unwrap()
                .contains(&remove_whitespace(expected_update_list)));
        }
        _ => {
            panic!("No message received after a second.");
        }
    }
    sm_mapper.unwrap().abort();
}

#[tokio::test]
#[cfg_attr(not(feature = "mosquitto-available"), ignore)]
#[serial]
async fn mapper_publishes_software_update_status_onto_c8y_topic() {
    // The test assures SM Mapper correctly receives software update response message on `tedge/commands/res/software/update`
    // and publishes status of the operation `501` on `c8y/s/us`
    // Create a subscriber to receive messages on `c8y/s/us` topic.
    let mut subscriber = get_subscriber(
        "c8y/s/us",
        "mapper_publishes_software_update_status_and_software_list_onto_c8y_topic",
    )
    .await;

    // Start SM Mapper
    let sm_mapper = start_sm_mapper().await;
    let _ = publish_a_fake_jwt_token().await;

    // Prepare and publish a software update status response message `executing` on `tedge/commands/res/software/update`.
    let json_response = r#"{
            "id": "123",
            "status": "executing"
        }"#;

    let _ = publish(
        &Topic::new("tedge/commands/res/software/update").unwrap(),
        json_response.to_string(),
    )
    .await;

    // Expect `501` smartrest message on `c8y/s/us`.
    loop {
        match tokio::time::timeout(TEST_TIMEOUT_MS, subscriber.next()).await {
            Ok(Some(msg)) => {
                dbg!(&msg.payload_str().unwrap());
                match msg.payload_str().unwrap() {
                    "501,c8y_SoftwareUpdate\n" => break,
                    _ => continue,
                }
            }
            _ => panic!("No update operation status message received after a second."),
        }
    }

    // Prepare and publish a software update response `successful`.
    let json_response = r#"{
            "id":"123",
            "status":"successful",
            "currentSoftwareList":[
                {"type":"apt","modules":[
                    {"name":"m","url":"https://foobar.io/m.epl"}
                ]}
            ]}"#;

    let _ = publish(
        &Topic::new("tedge/commands/res/software/update").unwrap(),
        json_response.to_string(),
    )
    .await;

    // Expect `503` messages with correct payload have been received on `c8y/s/us`, if no msg received for the timeout the test fails.
    let mut received_status_successful = false;

    match tokio::time::timeout(TEST_TIMEOUT_MS, subscriber.next()).await {
        Ok(Some(msg)) => {
            dbg!(&msg.payload_str().unwrap());
            match msg.payload_str().unwrap() {
                "503,c8y_SoftwareUpdate\n" => {
                    received_status_successful = true;
                }
                _ => {}
            }
        }
        _ => panic!("No update operation result message received after a second."),
    }
    sm_mapper.unwrap().abort();
    assert!(received_status_successful);
}

#[tokio::test]
#[cfg_attr(not(feature = "mosquitto-available"), ignore)]
#[serial]
async fn mapper_publishes_software_update_failed_status_onto_c8y_topic() {
    // Publish a software update response `failed`.
    let mut subscriber = get_subscriber(
        "c8y/s/us",
        "mapper_publishes_software_update_failed_status_onto_c8y_topic",
    )
    .await;

    // Start SM Mapper
    let sm_mapper = start_sm_mapper().await;
    let _ = publish_a_fake_jwt_token().await;

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

    let _ = publish(
        &Topic::new("tedge/commands/res/software/update").unwrap(),
        json_response.to_string(),
    )
    .await;

    // `502` messages with correct payload have been received on `c8y/s/us`, if no msg received for the timeout the test fails.
    let mut received_status_failed = false;

    for _ in 0..10 {
        // Loop 10 times, because it needs time to receive the messages
        match tokio::time::timeout(TEST_TIMEOUT_MS, subscriber.next()).await {
            Ok(Some(msg)) => {
                dbg!(&msg.payload_str().unwrap());
                match msg.payload_str().unwrap() {
                        "502,c8y_SoftwareUpdate,\"Partial failure: Couldn\'t install collectd and nginx\"\n" => { received_status_failed = true; break;}
                        _ => {}
                    }
                continue;
            }
            _ => panic!("No failed status message received after a second."),
        }
    }
    sm_mapper.unwrap().abort();
    assert!(received_status_failed);
}

#[tokio::test]
#[cfg_attr(not(feature = "mosquitto-available"), ignore)]
#[serial]
async fn mapper_fails_during_sw_update_recovers_and_process_response() -> Result<(), anyhow::Error>
{
    // The test assures recovery and processing of messages by the SM-Mapper when it fails in the middle of the operation.

    // The test does the following steps

    // When a software update request message is received on `c8y/s/ds` by the sm mapper,
    // converts it to thin-edge json message, publishes a request message on `tedge/commands/req/software/update`.
    // SM Mapper fails before receiving the response message for the request.
    // Meanwhile the operation response message was published on `tedge/commands/res/software/update`.
    // Now the SM Mapper recovers and receives the response message and publishes the status on `c8y/s/us`.
    // The subscriber that was waiting for the response on `c8/s/us` receives the response and validates it.

    // Create a subscriber to receive messages on `tedge/commands/req/software/update` topic.
    let mut sw_update_req_sub = get_subscriber(
        "tedge/commands/req/software/update",
        "software_update_request",
    )
    .await;

    // Create a subscriber to receive messages on `"c8y/s/us` topic.
    let mut sw_update_res_sub = get_subscriber(
        "c8y/s/us",
        "mapper_publishes_software_update_response_to_c8y_cloud",
    )
    .await;

    // Start SM Mapper
    let sm_mapper = start_sm_mapper().await?;
    let _ = publish_a_fake_jwt_token().await;

    // Prepare and publish a software update smartrest request on `c8y/s/ds`.
    let smartrest = r#"528,external_id,nodered,1.0.0::debian,,install"#;
    let _ = publish(&Topic::new("c8y/s/ds").unwrap(), smartrest.to_string()).await;

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
    match tokio::time::timeout(TEST_TIMEOUT_MS, sw_update_req_sub.next()).await {
        Ok(Some(msg)) => {
            dbg!(&msg.payload_str().unwrap());
            if msg
                .payload_str()
                .unwrap()
                .contains(&remove_whitespace(expected_update_list))
            {
                // Stop the SM Mapper
                sm_mapper.abort();
                assert!(sm_mapper.await.unwrap_err().is_cancelled());

                // Prepare and publish the response `successful`.
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
                let _ = publish(
                    &Topic::new("tedge/commands/res/software/update").unwrap(),
                    json_response.to_string(),
                )
                .await;
            }
        }
        _ => {
            panic!("No software update message received after a second.");
        }
    }

    // Restart SM Mapper
    let sm_mapper = start_sm_mapper().await?;
    let _ = publish_a_fake_jwt_token().await;

    let mut received_status_successful = false;

    // Validate the response that is received on 'c8y/s/us'
    // Wait till the mapper starts and receives the messages
    for _ in 0..10 {
        // Loop 10 times, because it needs time to receive the messages
        match tokio::time::timeout(TEST_TIMEOUT_MS, sw_update_res_sub.next()).await {
            Ok(Some(msg)) => {
                dbg!(&msg.payload_str().unwrap());
                match msg.payload_str().unwrap() {
                    "503,c8y_SoftwareUpdate\n" => {
                        received_status_successful = true;
                        break;
                    }

                    _ => {}
                }
                continue;
            }
            _ => panic!("No software update message received after a second."),
        }
    }
    sm_mapper.abort();
    Ok(assert!(received_status_successful))
}

#[tokio::test]
#[cfg_attr(not(feature = "mosquitto-available"), ignore)]
#[serial]
async fn mapper_publishes_software_update_request_with_wrong_action() {
    // The test assures SM Mapper correctly receives software update request smartrest message on `c8y/s/ds`
    // Then the SM Mapper finds out that wrong action as part of the update request.
    // Then SM Mapper publishes an operation status message as executing `501,c8y_SoftwareUpdate'
    // Then SM Mapper publishes an operation status message as failed `502,c8y_SoftwareUpdate,Action remove is not recognized. It must be install or delete.` on `c8/s/us`.
    // Then the subscriber that subscribed for messages on `c8/s/us` receives these messages and verifies them.

    // Create a subscriber to receive messages on `c8y/s/us` topic.
    let mut subscriber =
        get_subscriber("c8y/s/us", "mapper_publishes_software_update_failure").await;

    let _sm_mapper = start_sm_mapper().await;

    // Prepare and publish a c8_SoftwareUpdate smartrest request on `c8y/s/ds` that contains a wrong action `remove`, that is not known by c8y.
    let smartrest = r#"528,external_id,nodered,1.0.0::debian,,remove"#;
    let _ = publish(&Topic::new("c8y/s/ds").unwrap(), smartrest.to_string()).await;

    let mut received_status_failed = false;
    let mut received_response_failed = false;

    for _ in 0..2 {
        // Expect thin-edge json message on `c8y/s/us` with expected payload.
        match tokio::time::timeout(TEST_TIMEOUT_MS, subscriber.next()).await {
            Ok(Some(msg)) => {
                dbg!(&msg.payload_str().unwrap());
                match msg.payload_str().unwrap() {
                    "501,c8y_SoftwareUpdate" => {
                       received_status_failed = true;
                    },
                    "502,c8y_SoftwareUpdate,\"Action remove is not recognized. It must be install or delete.\"" => {
                        received_response_failed = true;
                    },
                    _ => {}
                }
            }
            _ => {}
        }
        if received_status_failed && received_response_failed {
            break;
        } else {
            continue;
        }
    }

    assert!(received_status_failed);
    assert!(received_response_failed);
}

fn create_tedge_config() -> TEdgeConfig {
    // Create a config file in a temporary directory.
    let temp_dir = tempfile::tempdir().unwrap();
    let content = r#"
        [mqtt]
        port=55555
        [c8y]
        url='test.c8y.com'
        "#;
    let mut file = tempfile::NamedTempFile::new_in(&temp_dir).unwrap();
    let _write_file = file.write_all(content.as_bytes()).unwrap();
    let path_buf = temp_dir.path().join("test");
    let _persist_file = file.persist(&path_buf);

    // Create tedge_config.
    let tedge_config_file_path = path_buf;
    let tedge_config_root_path = tedge_config_file_path.parent().unwrap().to_owned();
    dbg!(&tedge_config_file_path);
    let config_location = TEdgeConfigLocation {
        tedge_config_root_path,
        tedge_config_file_path,
    };

    tedge_config::TEdgeConfigRepository::new(config_location)
        .load()
        .unwrap()
}

async fn publish(topic: &Topic, payload: String) {
    let client = Client::connect(
        "sm_c8y_integration_test_publisher",
        &mqtt_client::Config::default().with_port(MQTT_TEST_PORT),
    )
    .await
    .unwrap();

    let () = client
        .publish(mqtt_client::Message::new(topic, payload))
        .await
        .unwrap();
}

fn remove_whitespace(s: &str) -> String {
    let mut s = String::from(s);
    s.retain(|c| !c.is_whitespace());
    s
}

async fn get_subscriber(pattern: &str, client_name: &str) -> Box<dyn MqttMessageStream> {
    let topic_filter = TopicFilter::new(pattern).unwrap();
    let subscriber = Client::connect(
        client_name,
        &mqtt_client::Config::default()
            .with_port(MQTT_TEST_PORT)
            .clean_session(),
    )
    .await
    .unwrap();

    // Obtain subscribe stream
    subscriber.subscribe(topic_filter).await.unwrap()
}

async fn start_sm_mapper() -> Result<JoinHandle<()>, anyhow::Error> {
    let tedge_config = create_tedge_config();
    let mqtt_config = mqtt_client::Config::default().with_port(MQTT_TEST_PORT);
    let mqtt_client = Client::connect("SM-C8Y-Mapper-Test", &mqtt_config).await?;
    let sm_mapper = CumulocitySoftwareManagement::new(mqtt_client, tedge_config);

    let mut topic_filter = TopicFilter::new(r#"tedge/commands/res/software/list"#)?;
    topic_filter.add(r#"tedge/commands/res/software/update"#)?;
    topic_filter.add(r#"c8y/s/ds"#)?;
    let messages = sm_mapper.client.subscribe(topic_filter).await?;
    let mapper_task = tokio::spawn(async move {
        let _ = sm_mapper.run(messages).await;
    });
    Ok(mapper_task)
}

async fn publish_a_fake_jwt_token() {
    let _ = publish(&Topic::new("c8y/s/dat").unwrap(), "71,1111".into()).await;
}
