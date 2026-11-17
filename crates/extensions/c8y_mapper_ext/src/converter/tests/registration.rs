use super::*;
use rstest::rstest;
use test_case::test_case;

#[rstest]
#[case::with_the_default_fragments(
    false,
    "101,test-device:device:child1,child1,thin-edge.io-child,false"
)]
#[case::with_the_device_fragment(
    true,
    "101,test-device:device:child1,child1,thin-edge.io-child,true"
)]
#[tokio::test]
async fn convert_child_device_registration(
    #[case] create_with_device_marker: bool,
    #[case] expected: &'static str,
) {
    let tmp_dir = TempTedgeDir::new();
    let mut config = c8y_converter_config(&tmp_dir);
    config.smartrest_child_device_create_with_device_marker = create_with_device_marker;
    let (mut converter, _http_proxy) = create_c8y_converter_from_config(config);

    let in_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1//"),
        json!({
            "@type":"child-device",
            "name":"child1"
        })
        .to_string(),
    );
    let UpdateOutcome::Inserted(entities) = converter
        .process_entity_metadata_message(&in_message)
        .await
        .unwrap()
    else {
        panic!("Expected insert outcome");
    };

    assert_eq!(entities.len(), 1);

    let messages = converter.convert(&in_message).await;

    assert_messages_present(&messages, [("c8y/s/us", expected.into())]);
}

#[test_case("device/main//", "test-device")]
#[test_case(
    "device/main/service/tedge-agent",
    "test-device:device:main:service:tedge-agent"
)]
#[test_case("device/child1//", "test-device:device:child1")]
#[test_case(
    "device/child1/service/collectd",
    "test-device:device:child1:service:collectd"
)]
#[test_case("custom_name///", "test-device:custom_name")]
#[tokio::test]
async fn entity_topic_id_to_c8y_external_id_mapping(entity_topic_id: &str, c8y_external_id: &str) {
    let entity_topic_id = EntityTopicId::from_str(entity_topic_id).unwrap();
    assert_eq!(
        CumulocityConverter::map_to_c8y_external_id(&entity_topic_id, &"test-device".into()),
        c8y_external_id.into()
    );
}

#[test_case("bad+name1", '+')]
#[test_case("bad/name2", '/')]
#[test_case("bad#name3", '#')]
#[test_case("my/very#bad+name", '/')]
fn sanitize_c8y_external_id(input_id: &str, invalid_char: char) {
    assert_eq!(
        CumulocityConverter::validate_external_id(input_id),
        Err(InvalidExternalIdError {
            external_id: input_id.into(),
            invalid_char,
        })
    );
}

#[tokio::test]
async fn duplicate_registration_messages_not_mapped_2311() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);

    let in_topic = "te/device/main/service/my_measurement_service";
    register_source_entities(in_topic, &mut converter).await;

    let reg_message = MqttMessage::new(&Topic::new_unchecked(in_topic), r#"{"@type": "service"}"#);

    // when converting a registration message the same as the previous one, no additional registration messages should be produced
    let outcome = converter
        .process_entity_metadata_message(&reg_message)
        .await
        .unwrap();
    assert_eq!(outcome, UpdateOutcome::Unchanged);
}

#[rstest]
#[case::empty("", "service")]
#[case::set("systemd", "systemd")]
#[tokio::test]
async fn the_health_status_of_a_service_registered_without_type_has_the_default_service_type(
    #[case] service_type: &str,
    #[case] expected_type: &str,
) {
    let tmp_dir = TempTedgeDir::new();
    let mut config = c8y_converter_config(&tmp_dir);
    let tedge_config = TEdgeConfig::load_toml_str(&format!("service.type = \"{service_type}\""));
    config.service = tedge_config.service.clone();

    let mut converter = MeaConverter::<HealthStatusConverter>::from_c8y_config(config);

    let service_health_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main/service/service1/status/health"),
        serde_json::to_string(&json!({"status": "up"})).unwrap(),
    );

    converter
        .register_source_entities(&service_health_message.topic.name)
        .await;

    let output = converter.convert(&service_health_message).await;
    let service_creation_message = output
        .into_iter()
        .find(|m| m.topic == C8yTopic::upstream_topic(&"c8y".try_into().unwrap()))
        .expect("service creation message should be present");

    let mut smartrest_fields = service_creation_message.payload_str().unwrap().split(',');

    assert_eq!(smartrest_fields.next().unwrap(), "102");
    assert_eq!(
        smartrest_fields.next().unwrap(),
        format!(
            "{}:device:main:service:service1",
            converter.c8y_converter.config.device_id
        )
    );
    assert_eq!(smartrest_fields.next().unwrap(), expected_type);
    assert_eq!(smartrest_fields.next().unwrap(), "service1");
    assert_eq!(smartrest_fields.next().unwrap(), "up");
}

#[tokio::test]
async fn the_health_status_of_a_child_device_service_registered_without_type_has_the_type_service()
{
    let tmp_dir = TempTedgeDir::new();
    let mut config = c8y_converter_config(&tmp_dir);
    config.service = TEdgeConfig::load_toml_str("service.type = \"systemd\"")
        .service
        .clone();
    let mut converter = MeaConverter::<HealthStatusConverter>::from_c8y_config(config);

    let service_health_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1/service/service1/status/health"),
        json!({"status": "up"}).to_string(),
    );
    converter
        .register_source_entities(&service_health_message.topic.name)
        .await;

    let output = converter.convert(&service_health_message).await;
    let service_creation_message = output
        .iter()
        .find_map(|m| m.payload_str().ok().filter(|p| p.starts_with("102,")))
        .expect("service creation message should be present");

    let smartrest_fields: Vec<&str> = service_creation_message.split(',').collect();
    assert_eq!(smartrest_fields[2], "service");
}

#[tokio::test]
async fn early_messages_cached_and_processed_only_after_registration() {
    let tmp_dir = TempTedgeDir::new();
    let mut config = c8y_converter_config(&tmp_dir);
    config.bridge_config.c8y_prefix = "custom-c8y-prefix".try_into().unwrap();

    let (mut converter, _http_proxy) = create_c8y_converter_from_config(config);

    // Publish some measurements that are only cached and not converted
    for i in 0..3 {
        let measurement_message = MqttMessage::new(
            &Topic::new_unchecked("te/custom/child1///m/environment"),
            json!({ "temperature": i }).to_string(),
        );
        let mapped_messages = converter.convert(&measurement_message).await;
        assert!(
            mapped_messages.is_empty(),
            "Expected the early telemetry messages to be cached and not mapped"
        )
    }

    // Publish a twin message which is also cached
    let twin_message = MqttMessage::new(
        &Topic::new_unchecked("te/custom/child1///twin/foo"),
        r#"5.6789"#,
    );
    let mapped_messages = converter.convert(&twin_message).await;
    assert!(
        mapped_messages.is_empty(),
        "Expected the early twin messages to be cached and not mapped"
    );

    // Publish the registration message which will trigger the conversion of cached messages as well
    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/custom/child1//"),
        json!({"@type": "child-device", "@id": "child1", "name": "child1"}).to_string(),
    );

    let UpdateOutcome::Inserted(entities) = converter
        .process_entity_metadata_message(&reg_message)
        .await
        .unwrap()
    else {
        panic!("Expected insert outcome");
    };

    let messages = registered_entities_into_mqtt_messages(entities);

    // Assert that the registration message, the twin updates and the cached measurement messages are converted
    assert_messages_matching(
        &messages,
        [
            (
                "te/custom/child1//",
                json!({
                    "@id":"child1",
                    "@type":"child-device",
                    "name":"child1"
                })
                .into(),
            ),
            ("te/custom/child1///twin/foo", "5.6789".into()),
            (
                "te/custom/child1///m/environment",
                json!({ "temperature": 0 }).into(),
            ),
            (
                "te/custom/child1///m/environment",
                json!({ "temperature": 1 }).into(),
            ),
            (
                "te/custom/child1///m/environment",
                json!({ "temperature": 2 }).into(),
            ),
        ],
    );
}

#[tokio::test]
async fn early_child_device_registrations_processed_only_after_parent_registration() {
    let tmp_dir = TempTedgeDir::new();
    let (mut converter, _http_proxy) = create_c8y_converter(&tmp_dir);
    let registrations = [
        ("te/device/child0//", child_device("child0", "main")),
        ("te/device/child00//", child_device("child00", "child0")),
        ("te/device/child000//", child_device("child000", "child00")),
    ];

    // Publish great-grand-child registration, then grand-child registration, before child
    for (topic, registration) in registrations.iter().skip(1).rev() {
        let reg_message = MqttMessage::new(&Topic::new_unchecked(topic), registration.to_string());
        let outcome = converter
            .process_entity_metadata_message(&reg_message)
            .await
            .unwrap();
        assert_eq!(outcome, UpdateOutcome::Unchanged);
    }

    // Register the immediate child device which will trigger the conversion of cached messages as well
    let (topic, registration) = &registrations[0];
    let reg_message = MqttMessage::new(&Topic::new_unchecked(topic), registration.to_string());
    let UpdateOutcome::Inserted(entities) = converter
        .process_entity_metadata_message(&reg_message)
        .await
        .unwrap()
    else {
        panic!("Expected insert outcome");
    };
    let messages = registered_entities_into_mqtt_messages(entities);
    assert_messages_matching(
        &messages,
        registrations.map(|(topic, registration)| (topic, registration.into())),
    );
}

#[tokio::test]
async fn custom_command_capability_is_declared_as_the_c8y_operation_of_its_template() {
    let tmp_dir = TempTedgeDir::new();
    tmp_dir
        .dir("operations")
        .dir("c8y")
        .file("c8y_Operation.template")
        .with_raw_content(
            r#"[exec]
            on_fragment = "c8y_Operation"

            [exec.workflow]
            operation = "my_operation"
            "#,
        );

    let config = c8y_converter_config(&tmp_dir);

    let (mut converter, _http_proxy) = create_c8y_converter_from_config(config);

    // main device command
    let main_device = EntityTopicId::default_main_device();

    let operation_topic = converter.mqtt_schema.topic_for(
        &main_device,
        &Channel::CommandMetadata {
            operation: OperationType::Custom("my_operation".to_string()),
        },
    );
    let operation_msg = MqttMessage::new(&operation_topic, "{}");

    let msgs = converter.convert(&operation_msg).await;
    assert_messages_matching(&msgs, [("c8y/s/us", "114,c8y_Operation".into())]);

    // child device command
    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child0//"),
        json!({
            "@type": "child-device",
            "@id": "child0",
            "name": "child0",
            "@parent": "device/main//",
        })
        .to_string(),
    );

    converter
        .process_entity_metadata_message(&reg_message)
        .await
        .unwrap();

    let child_device = EntityTopicId::default_child_device("child0").unwrap();
    let operation_topic = converter.mqtt_schema.topic_for(
        &child_device,
        &Channel::CommandMetadata {
            operation: OperationType::Custom("my_operation".to_string()),
        },
    );
    let operation_msg = MqttMessage::new(&operation_topic, "{}");

    let msgs = converter.convert(&operation_msg).await;
    assert_messages_matching(&msgs, [("c8y/s/us/child0", "114,c8y_Operation".into())]);
}

fn registered_entities_into_mqtt_messages(entities: Vec<RegisteredEntityData>) -> Vec<MqttMessage> {
    let mut messages = vec![];
    for entity in entities {
        messages.push(entity.reg_message.to_mqtt_message(&MqttSchema::default()));
        for data_message in entity.data_messages {
            messages.push(data_message);
        }
    }
    messages
}

/// The registration of a child device of the given parent device
fn child_device(id: &str, parent: &str) -> serde_json::Value {
    json!({
        "@type": "child-device",
        "@id": id,
        "name": id,
        "@parent": format!("device/{parent}//"),
    })
}
