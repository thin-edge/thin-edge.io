use crate::component::TEdgeComponent;
use mqtt_client::{Client, MqttClient, MqttMessageStream, Topic, TopicFilter};
use serial_test::serial;
use std::io::Write;
use std::time::Duration;
use tedge_config::{ConfigRepository, TEdgeConfig, TEdgeConfigLocation};

const MQTT_TEST_PORT: u16 = 55555;
const TEST_TIMEOUT_MS: Duration = Duration::from_millis(2000);

#[tokio::test]
#[cfg_attr(not(feature = "mosquitto-available"), ignore)]
#[serial]
async fn mapper_publishes_a_software_list_request() {
    // The test assures the mapper publishes request for software list on `tedge/commands/req/software/list`.

    // Create a subscriber to receive messages on the bus.
    let mut received = get_subscriber(
        "tedge/commands/req/software/list",
        "mapper_publishes_a_software_list_request",
    )
    .await;

    // Start SM Mapper
    let config = create_tedge_config();
    let _mapper_task = tokio::spawn(async {
        crate::sm_c8y_mapper::mapper::CumulocitySoftwareManagementMapper::new()
            .start(config)
            .await
    });

    // Expect message that arrives on `tedge/commands/req/software/list` is software list request.
    match tokio::time::timeout(TEST_TIMEOUT_MS, received.next()).await {
        Ok(Some(msg)) => {
            dbg!(&msg.payload_str().unwrap());
            assert!(&msg.payload_str().unwrap().contains("{\"id\":\""))
        }
        _ => panic!("No message received after a second."),
    }
}

#[tokio::test]
#[cfg_attr(not(feature = "mosquitto-available"), ignore)]
#[serial]
async fn mapper_publishes_a_supported_operation_and_a_pending_operations_onto_c8y_topic() {
    // The test assures the mapper publishes smartrest messages 114 and 500 on `c8y/s/us` which shall be send over to the cloud if bridge connection exists.

    // Create a subscriber to receive messages on `c8y/s/us` topic.
    let mut received = get_subscriber(
        "c8y/s/us",
        "mapper_publishes_a_supported_operation_and_a_pending_operations_onto_c8y_topic",
    )
    .await;

    // Start SM Mapper
    let config = create_tedge_config();
    let _mapper_task = tokio::spawn(async {
        crate::sm_c8y_mapper::mapper::CumulocitySoftwareManagementMapper::new()
            .start(config)
            .await
    });

    // Expect both 114 and 500 messages has been received on `c8y/s/us`, if no msg received for the timeout the test fails.
    let mut received_supported_operation = false;
    let mut received_pending_operation_request = false;
    loop {
        match tokio::time::timeout(TEST_TIMEOUT_MS, received.next()).await {
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
async fn mapper_publishes_software_list_onto_c8y_topic() {
    // The test assures SM Mapper correctly receives software list message on `tedge/commands/res/software/list`
    // and converts it to smartrest message published on `c8y/s/us`.

    // Create a subscriber to receive messages on `c8y/s/us` topic.
    let mut received =
        get_subscriber("c8y/s/us", "mapper_publishes_software_list_onto_c8y_topic").await;

    // Start SM Mapper
    let config = create_tedge_config();
    let _mapper_task = tokio::spawn(async {
        crate::sm_c8y_mapper::mapper::CumulocitySoftwareManagementMapper::new()
            .start(config)
            .await
    });

    // Prepare and publish a software list response message on `tedge/commands/res/software/list`.
    let json = r#"{
            "id":"123",
            "status":"successful",
            "currentSoftwareList":[
                {"type":"apt","modules":[
                    {"name":"m","url":"https://foobar.io/m.epl"}
                ]}
            ]}"#;

    let _ = publish(
        &Topic::new("tedge/commands/res/software/list").unwrap(),
        json.to_string(),
    )
    .await;

    // Expect `116` message with correct payload has been received on `c8y/s/us`, if no msg received for the timeout the test fails.
    loop {
        match tokio::time::timeout(TEST_TIMEOUT_MS, received.next()).await {
            Ok(Some(msg)) => {
                dbg!(&msg.payload_str().unwrap());
                match msg.payload_str().unwrap() {
                    "116,m,::apt,https://foobar.io/m.epl\n" => break,
                    _ => continue,
                }
            }
            _ => panic!("No message received after a second."),
        }
    }
}

#[tokio::test]
#[cfg_attr(not(feature = "mosquitto-available"), ignore)]
#[serial]
async fn mapper_publishes_software_update_request() {
    // The test assures SM Mapper correctly receives software update request smartrest message on `c8y/s/ds`
    // and converts it to thin-edge json message published on `tedge/commands/req/software/update`.

    // Create a subscriber to receive messages on `c8y/s/us` topic.
    let mut received = get_subscriber(
        "tedge/commands/req/software/update",
        "mapper_publishes_software_update_request",
    )
    .await;

    // Start SM Mapper
    let config = create_tedge_config();
    let _mapper_task = tokio::spawn(async {
        crate::sm_c8y_mapper::mapper::CumulocitySoftwareManagementMapper::new()
            .start(config)
            .await
    });

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
    match tokio::time::timeout(TEST_TIMEOUT_MS, received.next()).await {
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
async fn mapper_publishes_software_update_status_and_software_list_onto_c8y_topic() {
    // The test assures SM Mapper correctly receives software update response message on `tedge/commands/res/software/update`
    // and publishes status of the operation `501` on `c8y/s/us`
    // and converts it to smartrest messages published on `c8y/s/us`.

    // Create a subscriber to receive messages on `c8y/s/us` topic.
    let mut received = get_subscriber(
        "c8y/s/us",
        "mapper_publishes_software_update_status_and_software_list_onto_c8y_topic",
    )
    .await;

    // Start SM Mapper
    let config = create_tedge_config();
    let _mapper_task = tokio::spawn(async {
        crate::sm_c8y_mapper::mapper::CumulocitySoftwareManagementMapper::new()
            .start(config)
            .await
    });

    // Prepare and publish a software list response message on `tedge/commands/res/software/list`.
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
        match tokio::time::timeout(TEST_TIMEOUT_MS, received.next()).await {
            Ok(Some(msg)) => {
                dbg!(&msg.payload_str().unwrap());
                match msg.payload_str().unwrap() {
                    "501,c8y_SoftwareUpdate\n" => break,
                    _ => continue,
                }
            }
            _ => panic!("No message received after a second."),
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
    let mut received_software_list = false;
    for _ in 0..2 {
        match tokio::time::timeout(TEST_TIMEOUT_MS, received.next()).await {
            Ok(Some(msg)) => {
                dbg!(&msg.payload_str().unwrap());
                match msg.payload_str().unwrap() {
                    "503,c8y_SoftwareUpdate\n" => received_status_successful = true,
                    "116,m,::apt,https://foobar.io/m.epl\n" => received_software_list = true,
                    _ => {}
                }
                continue;
            }
            _ => panic!("No message received after a second."),
        }
    }
    assert!(received_status_successful);
    assert!(received_software_list);

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

    // Expect `116` and `502` messages with correct payload have been received on `c8y/s/us`, if no msg received for the timeout the test fails.
    let mut received_status_failed = false;
    let mut received_software_list = false;
    for _ in 0..2 {
        match tokio::time::timeout(TEST_TIMEOUT_MS, received.next()).await {
            Ok(Some(msg)) => {
                dbg!(&msg.payload_str().unwrap());
                match msg.payload_str().unwrap() {
                        "502,c8y_SoftwareUpdate,\"Partial failure: Couldn\'t install collectd and nginx\"\n" => received_status_failed = true,
                        "116,nginx,1.21.0::docker,\n" => received_software_list = true,
                        _ => {}
                    }
                continue;
            }
            _ => panic!("No message received after a second."),
        }
    }
    assert!(received_status_failed);
    assert!(received_software_list);
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
