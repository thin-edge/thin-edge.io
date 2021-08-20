#[cfg(test)]
#[cfg(feature = "integration-test")]
#[cfg(feature = "mosquitto-available")]

mod tests {
    use crate::component::TEdgeComponent;
    use mqtt_client::{Client, MqttClient, Topic, TopicFilter};
    use once_cell::sync::OnceCell;
    use serial_test::serial;
    use std::io::Write;
    use std::process::{Child, Stdio};
    use std::time::Duration;
    use tedge_config::{ConfigRepository, TEdgeConfig, TEdgeConfigLocation};

    const MQTT_TEST_PORT: u16 = 56666;
    static SERVER: OnceCell<Child> = OnceCell::new();

    #[tokio::test]
    #[serial]
    async fn mapper_publishes_a_software_list_request() {
        // Make sure the server is running.
        let _start_mosquitto = SERVER.get_or_init(spawn_mqtt_server);

        // Create a subscriber
        let topic_filter = TopicFilter::new("tedge/commands/req/software/list").unwrap();
        let subscriber = Client::connect(
            "mapper_publishes_a_software_list_request",
            &mqtt_client::Config::default().with_port(MQTT_TEST_PORT),
        )
        .await
        .unwrap();

        // Obtain subscribe stream
        let mut received = subscriber.subscribe(topic_filter).await.unwrap();

        // Start Mapper
        let config = create_tedge_config();
        let _mapper_task = tokio::spawn(async {
            crate::sm_c8y_mapper::mapper::CumulocitySoftwareManagementMapper::new()
                .start(config)
                .await
        });

        // The first message that arrives on tedge/commands/req/software/list should be software list request.
        match tokio::time::timeout(Duration::from_millis(1000), received.next()).await {
            Ok(Some(msg)) => {
                dbg!(&msg.payload_str().unwrap());
                assert!(&msg.payload_str().unwrap().contains("{\"id\":\""))
            }
            _ => panic!("No message received after a second."),
        }
    }

    #[tokio::test]
    #[serial]
    async fn mapper_publishes_a_supported_operation_and_a_pending_operations_onto_c8y_topic() {
        // Make sure the server is running.
        let _start_mosquitto = SERVER.get_or_init(spawn_mqtt_server);

        // Create a subscriber
        let topic_filter = TopicFilter::new("c8y/s/us").unwrap();
        let subscriber = Client::connect(
            "mapper_publishes_a_supported_operation_and_a_pending_operations_onto_c8y_topic",
            &mqtt_client::Config::default().with_port(MQTT_TEST_PORT),
        )
        .await
        .unwrap();

        // Obtain subscribe stream
        let mut received = subscriber.subscribe(topic_filter).await.unwrap();

        // Start Mapper
        let config = create_tedge_config();
        let _mapper_task = tokio::spawn(async {
            crate::sm_c8y_mapper::mapper::CumulocitySoftwareManagementMapper::new()
                .start(config)
                .await
        });

        // This needs to capture 2 msgs therefore we need to call next at least 2 times.
        let mut received_supported_operation = false;
        let mut received_pending_operation_request = false;
        loop {
            match tokio::time::timeout(Duration::from_millis(1000), received.next()).await {
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
    #[serial]
    async fn mapper_publishes_software_list_onto_c8y_topic() {
        // Make sure the server is running.
        let _start_mosquitto = SERVER.get_or_init(spawn_mqtt_server);

        // Create a subscriber
        let topic_filter = TopicFilter::new("c8y/s/us").unwrap();
        let subscriber = Client::connect(
            "mapper_publishes_software_list_onto_c8y_topic",
            &mqtt_client::Config::default().with_port(MQTT_TEST_PORT),
        )
        .await
        .unwrap();

        // Obtain subscribe stream
        let mut received = subscriber.subscribe(topic_filter).await.unwrap();

        // Start Mapper
        let config = create_tedge_config();
        let _mapper_task = tokio::spawn(async {
            crate::sm_c8y_mapper::mapper::CumulocitySoftwareManagementMapper::new()
                .start(config)
                .await
        });

        // Publish a software list response instead of agent
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

        // SmartREST 114 and 500 are also published on c8y/s/us. Therefore, a loop is required.
        // Loop will be not be forever because timeout causes a panic.
        loop {
            match tokio::time::timeout(Duration::from_millis(1000), received.next()).await {
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
    #[serial]
    async fn mapper_publishes_software_update_request() {
        // Make sure the server is running.
        let _start_mosquitto = SERVER.get_or_init(spawn_mqtt_server);

        // Create a subscriber
        let topic_filter = TopicFilter::new("tedge/commands/req/software/update").unwrap();
        let subscriber = Client::connect(
            "mapper_publishes_software_update_request",
            &mqtt_client::Config::default().with_port(MQTT_TEST_PORT),
        )
        .await
        .unwrap();

        // Obtain subscribe stream
        let mut received = subscriber.subscribe(topic_filter).await.unwrap();

        // Start Mapper
        let config = create_tedge_config();
        let _mapper_task = tokio::spawn(async {
            crate::sm_c8y_mapper::mapper::CumulocitySoftwareManagementMapper::new()
                .start(config)
                .await
        });

        // Publish a software update response instead of agent
        let smartrest = r#"528,external_id,nodered,1.0.0::debian,,install"#;
        let _ = publish(&Topic::new("c8y/s/ds").unwrap(), smartrest.to_string()).await;

        let update_list = r#"
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

        // The first message that arrives on tedge/commands/req/software/update should be software update request.
        match tokio::time::timeout(Duration::from_millis(1000), received.next()).await {
            Ok(Some(msg)) => {
                dbg!(&msg.payload_str().unwrap());
                assert!(&msg.payload_str().unwrap().contains("{\"id\":\""));
                assert!(&msg
                    .payload_str()
                    .unwrap()
                    .contains(&remove_whitespace(update_list)));
            }
            _ => {
                panic!("No message received after a second.");
            }
        }
    }

    #[tokio::test]
    #[serial]
    async fn mapper_publishes_software_update_status_and_software_list_onto_c8y_topic() {
        // Make sure the server is running.
        let _start_mosquitto = SERVER.get_or_init(spawn_mqtt_server);

        // Create a subscriber
        let topic_filter = TopicFilter::new("c8y/s/us").unwrap();
        let subscriber = Client::connect(
            "mapper_publishes_software_update_status_and_software_list_onto_c8y_topic",
            &mqtt_client::Config::default().with_port(MQTT_TEST_PORT),
        )
        .await
        .unwrap();

        // Obtain subscribe stream
        let mut received = subscriber.subscribe(topic_filter).await.unwrap();

        // Start Mapper
        let config = create_tedge_config();
        let _mapper_task = tokio::spawn(async {
            crate::sm_c8y_mapper::mapper::CumulocitySoftwareManagementMapper::new()
                .start(config)
                .await
        });

        // Publish a software update response `executing`
        let json_response = r#"{
            "id": "123",
            "status": "executing"
        }"#;
        let _ = publish(
            &Topic::new("tedge/commands/res/software/update").unwrap(),
            json_response.to_string(),
        )
        .await;

        // SmartREST 114 and 500 are also published on c8y/s/us. Therefore, a loop is required.
        // Loop will be not be forever because timeout causes a panic.
        loop {
            match tokio::time::timeout(Duration::from_millis(1000), received.next()).await {
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

        // Publish a software update response `successful`
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

        // This needs to capture 2 msgs therefore we need to call next at least 2 times.
        let mut received_status_successful = false;
        let mut received_software_list = false;
        for _ in 0..2 {
            match tokio::time::timeout(Duration::from_millis(1000), received.next()).await {
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

        // Publish a software update response `failed`
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

        // This needs to capture 2 msgs therefore we need to call next at least 2 times.
        let mut received_status_failed = false;
        let mut received_software_list = false;
        for _ in 0..2 {
            match tokio::time::timeout(Duration::from_millis(1000), received.next()).await {
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

    fn spawn_mqtt_server() -> Child {
        std::process::Command::new("mosquitto")
            .args(&["-p", MQTT_TEST_PORT.to_string().as_str()])
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .unwrap()
    }

    fn create_tedge_config() -> TEdgeConfig {
        // Create a config file in a temporary directory
        let temp_dir = tempfile::tempdir().unwrap();
        let content = r#"
        [mqtt]
        port=56666
        "#;
        let mut file = tempfile::NamedTempFile::new_in(&temp_dir).unwrap();
        let _write_file = file.write_all(content.as_bytes()).unwrap();
        let path_buf = temp_dir.path().join("test");
        let _persist_file = file.persist(&path_buf);

        // Create tedge_config
        let tedge_config_file_path = path_buf;
        let tedge_config_root_path = tedge_config_file_path.parent().unwrap().to_owned();
        dbg!(&tedge_config_file_path);
        let config_location = TEdgeConfigLocation {
            tedge_config_root_path,
            tedge_config_file_path,
        };
        let tedge_config = tedge_config::TEdgeConfigRepository::new(config_location)
            .load()
            .unwrap();

        tedge_config
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
}
