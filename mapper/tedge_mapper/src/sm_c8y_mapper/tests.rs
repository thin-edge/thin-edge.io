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
async fn mapper_publishes_a_software_list_request() -> Result<(), anyhow::Error> {
    // The test assures the mapper publishes request for software list on `tedge/commands/req/software/list`.

    // Create a subscriber to receive messages on the bus.
    let mut subscriber = get_subscriber(
        "tedge/commands/req/software/list",
        "mapper_publishes_a_software_list_request",
    )
    .await;

    // Start SM Mapper
    let _mapper = start_sm_mapper().await;

    // Expect message that arrives on `tedge/commands/req/software/list` is software list request.
    match tokio::time::timeout(TEST_TIMEOUT_MS, subscriber.next()).await {
        Ok(Some(msg)) => {
            dbg!(&msg.payload_str().unwrap());
            assert!(&msg.payload_str().unwrap().contains("{\"id\":\""))
        }
        _ => panic!("No message received after a second."),
    }

    Ok(())
}

#[tokio::test]
#[cfg_attr(not(feature = "mosquitto-available"), ignore)]
#[serial]
async fn mapper_publishes_a_supported_operation_and_a_pending_operations_onto_c8y_topic() {
    // The test assures the mapper publishes smartrest messages 114 and 500 on `c8y/s/us` which shall be send over to the cloud if bridge connection exists.

    // Create a subscriber to receive messages on `c8y/s/us` topic.
    let mut c8y_subscriber = get_subscriber("c8y/s/us", "wait_on_c8y_s_us").await;

    // Create a subscriber to receive messages on `tedge/commands/req/software/list` topic.
    let mut agent_subscriber =
        get_subscriber("tedge/commands/req/software/list", "wait_on_req_list").await;

    // Start SM Mapper
    let _mapper = start_sm_mapper().await;

    // Expect thin-edge json message on `tedge/commands/req/software/list` with expected payload.
    match tokio::time::timeout(TEST_TIMEOUT_MS, agent_subscriber.next()).await {
        Ok(Some(msg)) => {
            dbg!(&msg.payload_str().unwrap());
            // Prepare and publish a software update response `successful`.
            let json_response = r#"{
                            "id":"123",
                            "status":"successful",
                            "currentSoftwareList":[
                                {"type":"apt","modules":[
                                    {"name":"m","url":"https://foobar.io/m.epl"}
                                ]}
                        ]}"#;

            // Publish the response
            let _ = publish(
                &Topic::new("tedge/commands/res/software/list").unwrap(),
                json_response.to_string(),
            )
            .await;
        }
        _ => {
            panic!("Start list request not received");
        }
    }

    // Expect both 114 and 500 messages has been received on `c8y/s/us`, if no msg received for the timeout the test fails.
    let mut received_supported_operation = false;
    let mut received_pending_operation_request = false;
    loop {
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

    let _sm_mapper = start_sm_mapper().await;

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
}

#[tokio::test]
#[cfg_attr(not(feature = "mosquitto-available"), ignore)]
#[serial]
async fn mapper_publishes_software_update_status_onto_c8y_topic() {
    // The test assures SM Mapper correctly receives software update response message on `tedge/commands/res/software/update`
    // and publishes status of the operation `501` on `c8y/s/us`
    // and converts it to smartrest messages published on `c8y/s/us`.

    // Create a subscriber to receive messages on `c8y/s/us` topic.
    let mut subscriber = get_subscriber(
        "c8y/s/us",
        "mapper_publishes_software_update_status_and_software_list_onto_c8y_topic",
    )
    .await;

    // Start SM Mapper
    let _sm_mapper = start_sm_mapper().await;

    // Prepare and publish a software list response message on `tedge/commands/res/software/update`.
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

    // Expect `116` and `503` messages with correct payload have been received on `c8y/s/us`, if no msg received for the timeout the test fails.
    let mut received_status_successful = false;

    for _ in 0..2 {
        match tokio::time::timeout(TEST_TIMEOUT_MS, subscriber.next()).await {
            Ok(Some(msg)) => {
                dbg!(&msg.payload_str().unwrap());
                match msg.payload_str().unwrap() {
                    "503,c8y_SoftwareUpdate\n" => {
                        received_status_successful = true;
                        // After receiving successful message publish response with a custom 'token' on topic `c8y/s/dat`.
                        let _ =
                            publish(&Topic::new("c8y/s/dat").unwrap(), "71,1111".to_string()).await;
                        break;
                    }
                    _ => {}
                }
                continue;
            }
            _ => panic!("No update operation result message received after a second."),
        }
    }
    assert!(received_status_successful);

    // Publish a software update response `failed`.
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

    for _ in 0..3 {
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
    assert!(received_status_failed);
}

#[tokio::test]
#[cfg_attr(not(feature = "mosquitto-available"), ignore)]
#[serial]
async fn mapper_fails_during_sw_update_recovers_and_process_response() -> Result<(), anyhow::Error>
{
    // To run this test successfully follow below steps
    // Step 1: Add ` self.c8y_internal_id = "test".into();` at the beginning of the init function in sm_mapper_c8y/mapper.rs
    // Step 2: Connect to the c8y cloud, `sudo tedge connect c8y` and stop tedge-mapper-sm-c8y and tedge-agent services.

    // Here the tedge-agent is mocked, all the requests are answered by local subscribers.

    // The test assures SM Mapper correctly receives software update request smartrest message on `c8y/s/ds`
    // and converts it to thin-edge json message published on `tedge/commands/req/software/update`.
    // SM Mapper fails before receiving the rsponse for the request.
    // Meanwhile the opeartion response was published on `tedge/commands/req/software/update`.
    // Now the SM Mapper recovers and receives the response message and publishes it on `c8y/s/us`
    // The subscriber that was waiting for the response on `c8/s/us` receives the response and validates it.

    // Todo: Mock the c8y cloud to get internal_id.

    // Create a subscriber to receive messages on `tedge/commands/req/software/update` topic.
    let mut sw_update_req_sub = get_subscriber(
        "tedge/commands/req/software/update",
        "mapper_publishes_software_update_request",
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

    // Prepare and publish a software update response `successful`.
    let json_response = r#"{
        "id":"123",
        "status":"successful",
        "currentSoftwareList":[
            {"type":"apt","modules":[
                {"name":"m","url":"https://foobar.io/m.epl"}
            ]}
        ]}"#;

    // Prepare and publish a software update smartrest request on `c8y/s/ds`.
    let smartrest = r#"528, external_id,nodered,1.0.0::debian,,install"#;
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
                //let _ = sm_mapper.await;
                assert!(sm_mapper.await.unwrap_err().is_cancelled());

                // Publish the response
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
    let _sm_mapper = start_sm_mapper().await?;

    let mut received_status_successful = false;

    // Validate the response that is received on 'c8y/s/us'
    for _ in 0..8 {
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

    Ok(assert!(received_status_successful))
}

fn create_tedge_config() -> TEdgeConfig {
    // Create a config file in a temporary directory.
    let temp_dir = tempfile::tempdir().unwrap();
    let content = r#"
        [mqtt]
        port=55555
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
        &mqtt_client::Config::default().with_port(MQTT_TEST_PORT),
    )
    .await
    .unwrap();

    // Obtain subscribe stream
    subscriber.subscribe(topic_filter).await.unwrap()
}

async fn start_sm_mapper() -> Result<JoinHandle<()>, anyhow::Error> {
    let tedge_config = create_tedge_config();
    let mqtt_config = mqtt_client::Config::default().with_port(MQTT_TEST_PORT);
    let mqtt_client = Client::connect("SM-C8Y-Mapper", &mqtt_config).await?;
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
