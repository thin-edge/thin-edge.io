use crate::sm_c8y_mapper::error::SMCumulocityMapperError;
use crate::sm_c8y_mapper::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use crate::sm_c8y_mapper::json_c8y::C8yUpdateSoftwareListResponse;
use crate::sm_c8y_mapper::mapper::CumulocitySoftwareManagement;
use c8y_smartrest::smartrest_deserializer::SmartRestJwtResponse;
use mqtt_client::Client;
use mqtt_tests::test_mqtt_server::MqttProcessHandler;
use mqtt_tests::with_timeout::{Maybe, WithTimeout};
use serial_test::serial;
use std::{io::Write, time::Duration};
use tedge_config::{ConfigRepository, TEdgeConfig, TEdgeConfigLocation};
use tokio::task::JoinHandle;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(1000);

#[tokio::test]
#[serial]
async fn mapper_publishes_a_software_list_request() {
    // The test assures the mapper publishes request for software list on `tedge/commands/req/software/list`.
    let broker = mqtt_tests::test_mqtt_broker();

    let mut messages = broker
        .messages_published_on("tedge/commands/req/software/list")
        .await;

    // Start the SM Mapper
    let sm_mapper = start_sm_mapper(broker.port).await;

    // Expect on `tedge/commands/req/software/list` a software list request.
    let msg = messages
        .recv()
        .with_timeout(TEST_TIMEOUT_MS)
        .await
        .expect_or("No message received after a second.");
    dbg!(&msg);
    assert!(&msg.contains(r#"{"id":"#));

    sm_mapper.unwrap().abort();
}

#[tokio::test]
#[serial]
async fn mapper_publishes_a_supported_operation_and_a_pending_operations_onto_c8y_topic() {
    // The test assures the mapper publishes smartrest messages 114 and 500 on `c8y/s/us` which shall be send over to the cloud if bridge connection exists.
    let broker = mqtt_tests::test_mqtt_broker();
    let mut messages = broker.messages_published_on("c8y/s/us").await;

    // Start SM Mapper
    let sm_mapper = start_sm_mapper(broker.port).await;

    // Expect both 114 and 500 messages has been received on `c8y/s/us`, if no msg received for the timeout the test fails.
    mqtt_tests::assert_received(
        &mut messages,
        TEST_TIMEOUT_MS,
        vec!["118,software-management\n", "500\n"],
    )
    .await;
    sm_mapper.unwrap().abort();
}

#[tokio::test]
#[serial]
async fn mapper_publishes_software_update_request() {
    // The test assures SM Mapper correctly receives software update request smartrest message on `c8y/s/ds`
    // and converts it to thin-edge json message published on `tedge/commands/req/software/update`.
    let broker = mqtt_tests::test_mqtt_broker();
    let mut messages = broker
        .messages_published_on("tedge/commands/req/software/update")
        .await;

    let sm_mapper = start_sm_mapper(broker.port).await;

    // Prepare and publish a software update smartrest request on `c8y/s/ds`.
    let smartrest = r#"528,external_id,nodered,1.0.0::debian,,install"#;
    let _ = broker.publish("c8y/s/ds", smartrest).await.unwrap();
    let _ = publish_a_fake_jwt_token(&broker).await;

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
    let msg = messages
        .recv()
        .with_timeout(TEST_TIMEOUT_MS)
        .await
        .expect_or("No message received after a second.");
    assert!(&msg.contains("{\"id\":\""));
    assert!(&msg.contains(&remove_whitespace(expected_update_list)));
    sm_mapper.unwrap().abort();
}

#[tokio::test]
#[serial]
async fn mapper_publishes_software_update_status_onto_c8y_topic() {
    // The test assures SM Mapper correctly receives software update response message on `tedge/commands/res/software/update`
    // and publishes status of the operation `501` on `c8y/s/us`
    let broker = mqtt_tests::test_mqtt_broker();

    let mut messages = broker.messages_published_on("c8y/s/us").await;

    // Start SM Mapper
    let sm_mapper = start_sm_mapper(broker.port).await;
    let _ = publish_a_fake_jwt_token(&broker).await;

    mqtt_tests::assert_received(
        &mut messages,
        TEST_TIMEOUT_MS,
        vec!["118,software-management\n", "500\n"],
    )
    .await;

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
    let msg = messages
        .recv()
        .with_timeout(TEST_TIMEOUT_MS)
        .await
        .expect_or("No message received after a second.");
    assert_eq!(&msg, "501,c8y_SoftwareUpdate\n");

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
    let msg = messages
        .recv()
        .with_timeout(TEST_TIMEOUT_MS)
        .await
        .expect_or("No message received after a second.");
    assert_eq!(&msg, "503,c8y_SoftwareUpdate,\n");

    sm_mapper.unwrap().abort();
}

#[tokio::test]
#[serial]
async fn mapper_publishes_software_update_failed_status_onto_c8y_topic() {
    let broker = mqtt_tests::test_mqtt_broker();
    let mut messages = broker.messages_published_on("c8y/s/us").await;

    // Start SM Mapper
    let sm_mapper = start_sm_mapper(broker.port).await;
    let _ = publish_a_fake_jwt_token(&broker).await;
    mqtt_tests::assert_received(
        &mut messages,
        TEST_TIMEOUT_MS,
        vec!["118,software-management\n", "500\n"],
    )
    .await;

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
    let msg = messages
        .recv()
        .with_timeout(TEST_TIMEOUT_MS)
        .await
        .expect_or("No message received after a second.");
    assert_eq!(
        &msg,
        "502,c8y_SoftwareUpdate,\"Partial failure: Couldn\'t install collectd and nginx\"\n"
    );

    sm_mapper.unwrap().abort();
}

#[tokio::test]
#[ignore]
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
    let sm_mapper = start_sm_mapper(broker.port).await?;
    mqtt_tests::assert_received(
        &mut responses,
        TEST_TIMEOUT_MS,
        vec![
            "114,c8y_SoftwareUpdate,c8y_LogfileRequest\n",
            "118,software-management\n",
            "500\n",
        ],
    )
    .await;

    // Prepare and publish a software update smartrest request on `c8y/s/ds`.
    let smartrest = r#"528,external_id,nodered,1.0.0::debian,,install"#;
    let _ = broker.publish("c8y/s/ds", smartrest).await.unwrap();
    let _ = publish_a_fake_jwt_token(&broker).await;

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
    let msg = requests
        .recv()
        .with_timeout(TEST_TIMEOUT_MS)
        .await
        .expect_or("No message received after a second.");
    assert!(msg.contains(&remove_whitespace(expected_update_list)));

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
    let sm_mapper = start_sm_mapper(broker.port).await?;

    // FIXME. Uncommenting this makes the test pass
    //        Meaning the mapper misses the message that has been sent when it was down.
    // let _ = broker.publish(
    //     "tedge/commands/res/software/update",
    //     &remove_whitespace(json_response),
    // )
    // .await
    // .unwrap();

    // Validate that the mapper process the response and forward it on 'c8y/s/us'
    // Expect init messages followed by a 503 (success)
    mqtt_tests::assert_received(
        &mut responses,
        TEST_TIMEOUT_MS * 5,
        vec![
            "114,c8y_SoftwareUpdate,c8y_LogfileRequest\n",
            "118,software-management\n",
            "500\n",
            "503,c8y_SoftwareUpdate\n",
        ],
    )
    .await;

    sm_mapper.abort();
    Ok(())
}

#[tokio::test]
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

    let _sm_mapper = start_sm_mapper(broker.port).await;
    mqtt_tests::assert_received(
        &mut messages,
        TEST_TIMEOUT_MS,
        vec!["118,software-management\n", "500\n"],
    )
    .await;

    // Prepare and publish a c8_SoftwareUpdate smartrest request on `c8y/s/ds` that contains a wrong action `remove`, that is not known by c8y.
    let smartrest = r#"528,external_id,nodered,1.0.0::debian,,remove"#;
    let _ = broker.publish("c8y/s/ds", smartrest).await.unwrap();

    // Expect a 501 (executing) followed by a 502 (failed)
    mqtt_tests::assert_received(
        &mut messages,
        TEST_TIMEOUT_MS,
        vec![
        "501,c8y_SoftwareUpdate",
        "502,c8y_SoftwareUpdate,\"Parameter remove is not recognized. It must be install or delete.\"",
    ],
    )
    .await;
}

#[tokio::test]
#[serial_test::serial]
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
    let mqtt_config = mqtt_client::Config::default().with_port(broker.port);
    let mqtt_client = Client::connect("JWT-Requester-Test", &mqtt_config)
        .await
        .unwrap();
    let http_client = reqwest::ClientBuilder::new().build().unwrap();
    let http_proxy =
        JwtAuthHttpProxy::new(mqtt_client, http_client, "test.tenant.com", "test-device");

    // ... fetches and returns these JWT tokens.
    let jwt_token = http_proxy.get_jwt_token().await;

    // `get_jwt_token` should return `Ok` and the value of token should be as set above `1111`.
    assert!(jwt_token.is_ok());
    assert_eq!(jwt_token.unwrap().token(), "1111");
}

fn create_tedge_config(mqtt_port: u16) -> TEdgeConfig {
    // Create a config file in a temporary directory.
    let temp_dir = tempfile::tempdir().unwrap();
    let content = format!(
        r#"
        [mqtt]
        port={}
        [c8y]
        url='test.c8y.com'
        "#,
        mqtt_port
    );
    let mut file = tempfile::NamedTempFile::new_in(&temp_dir).unwrap();
    let _write_file = file.write_all(content.as_bytes()).unwrap();
    let path_buf = temp_dir.path().join("test");
    let _persist_file = file.persist(&path_buf);

    // Create tedge_config.
    let tedge_config_file_path = path_buf;
    let tedge_config_root_path = tedge_config_file_path.parent().unwrap().to_owned();
    let config_location = TEdgeConfigLocation {
        tedge_config_root_path,
        tedge_config_file_path,
    };

    tedge_config::TEdgeConfigRepository::new(config_location)
        .load()
        .unwrap()
}

fn remove_whitespace(s: &str) -> String {
    let mut s = String::from(s);
    s.retain(|c| !c.is_whitespace());
    s
}

async fn start_sm_mapper(mqtt_port: u16) -> Result<JoinHandle<()>, anyhow::Error> {
    let mqtt_config = mqtt_client::Config::default().with_port(mqtt_port);

    let mqtt_client = Client::connect("SM-C8Y-Mapper-Test", &mqtt_config).await?;
    let http_proxy = FakeC8YHttpProxy {};
    let mut sm_mapper = CumulocitySoftwareManagement::new(mqtt_client, http_proxy);
    let messages = sm_mapper.subscribe().await?;

    let mapper_task = tokio::spawn(async move {
        let _ = sm_mapper.run(messages).await;
    });
    Ok(mapper_task)
}

async fn publish_a_fake_jwt_token(broker: &MqttProcessHandler) {
    let _ = broker.publish("c8y/s/dat", "71,1111").await.unwrap();
}

struct FakeC8YHttpProxy {}

#[async_trait::async_trait]
impl C8YHttpProxy for FakeC8YHttpProxy {
    async fn init(&mut self) -> Result<(), SMCumulocityMapperError> {
        Ok(())
    }

    fn url_is_in_my_tenant_domain(&self, _url: &str) -> bool {
        true
    }

    async fn get_jwt_token(&self) -> Result<SmartRestJwtResponse, SMCumulocityMapperError> {
        Ok(SmartRestJwtResponse::try_new("71,fake-token")?)
    }

    async fn send_software_list_http(
        &self,
        _c8y_software_list: &C8yUpdateSoftwareListResponse,
    ) -> Result<(), SMCumulocityMapperError> {
        Ok(())
    }

    async fn upload_log_binary(
        &self,
        _log_content: &str,
    ) -> Result<String, SMCumulocityMapperError> {
        Ok("fake/upload/url".into())
    }
}
