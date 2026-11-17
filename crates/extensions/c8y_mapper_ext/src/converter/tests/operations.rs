use super::*;
use test_case::test_case;

#[tokio::test]
async fn test_execute_operation_is_not_blocked() {
    let tmp_dir = TempTedgeDir::new();
    let (converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    let operation = |name: &str, timeout: u64| {
        let mut operation = toml::from_str::<Operation>(&format!(
            "[exec]\ncommand = \"sleep 5\"\ntimeout = {timeout}\nforceful_timeout = 1"
        ))
        .unwrap();
        operation.name = name.to_owned();
        operation
    };

    let now = Instant::now();
    converter
        .execute_operation(
            ShellScript::from_str("sleep 5").unwrap(),
            &operation("sleep_ten", 10),
            None,
        )
        .await
        .unwrap();
    converter
        .execute_operation(
            ShellScript::from_str("sleep 5").unwrap(),
            &operation("sleep_twenty", 20),
            None,
        )
        .await
        .unwrap();

    // a result between now and elapsed that is not 0 probably means that the operations are
    // blocking and that you probably removed a tokio::spawn handle (;
    assert_eq!(now.elapsed().as_secs(), 0);
}

#[test_case(false ; "while in progress")]
#[test_case(true ; "after completion")]
#[tokio::test]
async fn operations_are_deduplicated(completed: bool) {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    let operation = restart_operation();
    assert_eq!(
        converter.try_convert(&operation).await.unwrap(),
        vec![restart_command()]
    );
    if completed {
        let local_completion = MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/restart/c8y-mapper-16574089"),
            "",
        );
        assert_eq!(
            converter.try_convert(&local_completion).await.unwrap(),
            vec![]
        );
    }

    assert_eq!(converter.try_convert(&operation).await.unwrap(), vec![]);
}

#[tokio::test]
async fn a_repeated_operation_does_not_hide_the_other_operations_of_its_message() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
    let repeated = restart_operation();
    converter.try_convert(&repeated).await.unwrap();

    let new = json!({
        "id": "16574090",
        "status": "PENDING",
        "c8y_Restart": {},
        "externalSource": {"externalId": "test-device", "type": "c8y_Serial"}
    });
    let both = MqttMessage::new(
        &repeated.topic,
        format!("{}\n{new}", repeated.payload_str().unwrap()),
    );

    assert_eq!(
        converter.try_convert(&both).await.unwrap(),
        vec![MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/restart/c8y-mapper-16574090"),
            json!({"status":"init"}).to_string(),
        )
        .with_qos(QoS::AtLeastOnce)
        .with_retain()]
    );
}

#[tokio::test]
async fn custom_operations_are_deduplicated() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_converter_with_custom_operation(&tmp_dir);

    let operation = custom_operation();
    assert!(
        matches!(
            dbg!(converter.try_convert(&operation).await.unwrap().as_slice()),
            [MqttMessage { topic, .. }, ..] if topic.to_string() == "c8y/s/us"
        ),
        "Initial operation delivery produces outgoing message"
    );
    assert_eq!(
        converter.try_convert(&operation).await.unwrap(),
        vec![],
        "Operation redelivery is ignored by converter"
    );
}

#[tokio::test]
async fn custom_operations_are_not_deduplicated_before_registration() {
    // We could potentially receive a custom operation before we have
    // registered a handler for it. If we then register a suitable handler
    // and the operation is redelivered to the mapper, this should be
    // processed
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_converter_with_custom_operation(&tmp_dir);
    let before_registration = SupportedOperations {
        device_id: converter.supported_operations.device_id.clone(),
        base_ops_dir: converter.supported_operations.base_ops_dir.clone(),
        operations_by_xid: <_>::default(),
    };
    let after_registration =
        std::mem::replace(&mut converter.supported_operations, before_registration);

    let operation = custom_operation();
    assert_eq!(
        converter.try_convert(&operation).await.unwrap(),
        vec![],
        "Operation is ignored before the operation is registered"
    );

    converter.supported_operations = after_registration;

    assert!(
        matches!(
            dbg!(converter.try_convert(&operation).await.unwrap().as_slice()),
            [MqttMessage { topic, .. }, ..] if topic.to_string() == "c8y/s/us"
        ),
        "First delivery after registration produces outgoing message"
    );
}

#[tokio::test]
async fn te_topic_operations_do_not_have_time_based_expiry() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    tokio::time::pause();

    let operation = restart_operation();
    let expected_message = restart_command();

    assert_eq!(
        converter.try_convert(&operation).await.unwrap().first(),
        Some(&expected_message),
        "First delivery after registration produces outgoing message"
    );
    assert_eq!(converter.commands.active_count(), 1);

    // Converter should disable time-based expiry when the `te` topic
    // message is processed
    converter.try_convert(&expected_message).await.unwrap();

    tokio::time::advance(Duration::from_secs(24 * 3600)).await;

    let random_message = MqttMessage::new(&Topic::new_unchecked("c8y/s/ds"), "510,test");

    // Trigger the converter since it performs cache eviction only when it's converting c8y messages
    converter.try_convert(&random_message).await.unwrap();
    assert_eq!(converter.commands.active_count(), 1);
}

#[test_case(60, 1 ; "not evicted from cache prematurely")]
#[test_case(24 * 3600, 0 ; "not cached indefinitely")]
#[tokio::test]
async fn custom_operation_ids_expire(elapsed_secs: u64, expected_active_commands: usize) {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_converter_with_custom_operation(&tmp_dir);

    tokio::time::pause();

    assert!(
        matches!(
            dbg!(converter.try_convert(&custom_operation()).await.unwrap().as_slice()),
            [MqttMessage { topic, .. }, ..] if topic.to_string() == "c8y/s/us"
        ),
        "First delivery after registration produces outgoing message"
    );
    assert_eq!(converter.commands.active_count(), 1);

    tokio::time::advance(Duration::from_secs(elapsed_secs)).await;

    // Trigger the converter since it performs cache eviction only when it's converting c8y messages
    let random_message = MqttMessage::new(&Topic::new_unchecked("c8y/s/ds"), "510,test");
    converter.try_convert(&random_message).await.unwrap();
    assert_eq!(converter.commands.active_count(), expected_active_commands);
}

#[tokio::test]
async fn active_commands_is_populated_with_existing_commands_from_retain_messages() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    let existing_pending_operation = restart_command();
    converter
        .try_convert(&existing_pending_operation)
        .await
        .unwrap();

    let operation = restart_operation();

    assert_eq!(
        converter.try_convert(&operation).await.unwrap().as_slice(),
        [],
        "Existing tedge operation should trigger de-duplication"
    );
}

#[tokio::test]
async fn smartrest_message_on_builtin_topic_is_not_an_unsupported_topic() {
    let (capture, _guard) = tedge_test_utils::tracing::TracingCapture::default().start();

    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    let message = MqttMessage::new(&Topic::new_unchecked("c8y/s/ds"), "510,test-device");
    converter.try_convert(&message).await.unwrap();

    capture.assert_no_errors();
}

#[tokio::test]
async fn handle_operations_for_child_device() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    // The child has first to declare its capabilities
    let mqtt_schema = MqttSchema::default();
    let child = EntityTopicId::default_child_device("childId").unwrap();
    let child_capability = SoftwareUpdateCommand::capability_message(&mqtt_schema, &child);

    register_source_entities(&child_capability.topic.name, &mut converter).await;

    // A request to a child is forwarded to that child using its registered mapping: external id <=> topic identifier
    let device_cmd_channel = mqtt_schema.topics(
        EntityFilter::Entity(&child),
        ChannelFilter::Command(OperationType::SoftwareUpdate),
    );
    let mqtt_message = MqttMessage::new(
        &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
        json!({
            "id": "123456",
            "c8y_SoftwareUpdate": [
                {
                    "name": "software_a",
                    "action": "install",
                    "version": "version_a",
                    "url": "url_a"
                }
            ],
            "externalSource": {
                "externalId": "test-device:device:childId",
                "type": "c8y_Serial"
            }
        })
        .to_string(),
    );
    let command = converter
        .convert_device_control_operations(&mqtt_message)
        .await
        .unwrap()
        .get(0)
        .unwrap()
        .clone();

    assert!(device_cmd_channel.accept(&command));
    assert_eq!(
        serde_json::from_slice::<Value>(command.payload_bytes()).unwrap(),
        json!({
            "status":"init",
            "updateList":[
                { "type":"default",
                  "modules":[
                    {"name":"software_a","version":"version_a","url":"url_a","action":"install"}
                  ]}
            ]
        })
    );
}

#[tokio::test]
async fn an_invalid_operation_of_a_child_device_is_failed_on_the_smartrest_topic_of_the_child_device(
) {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
    register_source_entities("te/device/childId//", &mut converter).await;

    assert_messages_matching(
            &converter
                .convert(&MqttMessage::new(
                    &Topic::new_unchecked("c8y/devicecontrol/notifications"),
                    json!({
                        "id": "123456",
                        "c8y_SoftwareUpdate": [
                            {"name": "nodered", "action": "remove", "version": "1.0.0::debian"}
                        ],
                        "externalSource": {"externalId": "test-device:device:childId", "type": "c8y_Serial"},
                    })
                    .to_string(),
                ))
                .await,
            [
                (
                    "c8y/s/us/test-device:device:childId",
                    "501,c8y_SoftwareUpdate".into(),
                ),
                (
                    "c8y/s/us/test-device:device:childId",
                    "502,c8y_SoftwareUpdate,Parameter remove is not recognized. It must be install or delete.".into(),
                ),
            ],
        );
}

/// A `c8y_Restart` operation with the ID 16574089 for the main device
fn restart_operation() -> MqttMessage {
    c8y_operation("c8y/devicecontrol/notifications", "c8y_Restart")
}

/// The restart command created by the mapper for [`restart_operation`]
fn restart_command() -> MqttMessage {
    MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/restart/c8y-mapper-16574089"),
        json!({"status":"init"}).to_string(),
    )
    .with_qos(QoS::AtLeastOnce)
    .with_retain()
}

/// A converter supporting the `my_op` custom operation, received on `my/custom/topic`
fn create_converter_with_custom_operation(
    tmp_dir: &TempTedgeDir,
) -> (CumulocityConverter, FakeServerBox<HttpRequest, HttpResult>) {
    let custom_op = r#"exec.topic = "my/custom/topic"
        exec.on_fragment = "my_op"
        exec.command = "/etc/tedge/operations/command ${.payload.my_op.text}""#;
    tmp_dir
        .dir("operations")
        .dir("c8y")
        .file("my_op")
        .with_raw_content(custom_op);
    create_c8y_converter(tmp_dir)
}

/// A `my_op` custom operation with the ID 16574089 for the main device
fn custom_operation() -> MqttMessage {
    c8y_operation("my/custom/topic", "my_op")
}

/// An operation with the ID 16574089 for the main device, with the given fragment, on the given topic
fn c8y_operation(topic: &str, fragment: &str) -> MqttMessage {
    let mut operation = json!({
        "id": "16574089",
        "status": "PENDING",
        "description": "do something",
        "externalSource": {"externalId": "test-device", "type": "c8y_Serial"}
    });
    operation[fragment] = json!({});
    MqttMessage::new(&Topic::new_unchecked(topic), operation.to_string())
}
