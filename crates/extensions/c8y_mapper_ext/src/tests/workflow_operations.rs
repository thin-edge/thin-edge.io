//! Cumulocity operations converted by the mapper to thin-edge commands, as configured by operation templates

use super::*;
use rstest::rstest;

#[rstest]
#[case::main_device(
    "main",
    r#"[exec]
    topic = "c8y/devicecontrol/notifications"
    on_fragment = "c8y_Command"

    [exec.workflow]
    operation = "command"
    input = "${.payload.c8y_Command}"
    output = "${.payload.result}"
    "#,
    json!({ "c8y_Command": { "text": "do something" } }),
    json!({
        "status": "init",
        "text": "do something",
        "c8y-mapper": {
            "on_fragment": "c8y_Command",
            "output": "${.payload.result}"
        }
    })
)]
#[case::child_device(
    "child1",
    r#"[exec]
    topic = "c8y/devicecontrol/notifications"
    on_fragment = "c8y_Command"

    [exec.workflow]
    operation = "command"
    input = "${.payload.c8y_Command}"
    "#,
    json!({ "c8y_Command": { "text": "do something" } }),
    json!({
        "status": "init",
        "text": "do something",
        "c8y-mapper": { "on_fragment": "c8y_Command" }
    })
)]
#[case::combined_input(
    "main",
    r#"[exec]
    topic = "c8y/devicecontrol/notifications"
    on_fragment = "c8y_CombinedInput"

    [exec.workflow]
    operation = "command"
    input.x = "${.payload.c8y_CombinedInput.inner.x}"
    input.y = "${.payload.c8y_CombinedInput.inner.y}"
    input.z = { foo = "bar" }
    "#,
    json!({ "c8y_CombinedInput": {
        "text": "do something",
        "inner": { "x": "x value", "y": 42, "z": "z unused value" },
    } }),
    json!({
        "status": "init",
        "x": "x value",
        "y": 42,
        "z": { "foo": "bar" },
        "c8y-mapper": { "on_fragment": "c8y_CombinedInput" }
    })
)]
#[case::without_workflow_input(
    "main",
    r#"[exec]
    topic = "c8y/devicecontrol/notifications"
    on_fragment = "c8y_Command"

    [exec.workflow]
    operation = "command"
    "#,
    json!({ "c8y_Command": { "text": "do something" } }),
    json!({
        "status": "init",
        "c8y-mapper": { "on_fragment": "c8y_Command" }
    })
)]
#[tokio::test]
async fn mapper_converts_custom_operation_to_command(
    #[case] device: &str,
    #[case] template: &str,
    #[case] operation: serde_json::Value,
    #[case] expected_command: serde_json::Value,
) {
    let (_mapper, mut mqtt) = send_workflow_operation(device, template, operation).await;

    assert_received_includes_json(
        &mut mqtt,
        [(
            format!("te/device/{device}///cmd/command/c8y-mapper-1234"),
            expected_command,
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_converts_custom_operation_for_main_device_with_invalid_workflow_input() {
    let (_mapper, mut mqtt) = send_workflow_operation(
        "main",
        r#"[exec]
        topic = "c8y/devicecontrol/notifications"
        on_fragment = "c8y_Command"

        [exec.workflow]
        operation = "command"
        input = "invalid input"
        "#,
        json!({ "c8y_Command": { "text": "do something" } }),
    )
    .await;

    // message should not be sent due to incorrect payload
    assert!(mqtt.recv().await.is_none());
}

#[tokio::test]
async fn mapper_maps_c8y_command_to_the_builtin_shell_execute_command() {
    let ttd = TempTedgeDir::new();
    let config = test_mapper_config(&ttd);

    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // The built-in operation template has been deployed by the mapper
    assert!(ttd
        .path()
        .join("operations/c8y/c8y_Command.template")
        .is_file());

    // The agent indicates that the main device supports the shell_execute command
    let capability_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/shell_execute"),
        "{}",
    );
    mqtt.send(capability_message).await.unwrap();

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "114,c8y_Command")]).await;

    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
            "status": "PENDING",
            "id": "1234",
            "c8y_Command": {
                "text": "echo hello"
            },
            "externalSource": {
                "externalId": "test-device",
                "type": "c8y_Serial"
            }
        })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/main///cmd/shell_execute/c8y-mapper-1234",
            json!({
                "status": "init",
                "command": "echo hello",
                "c8y-mapper": {
                    "on_fragment": "c8y_Command",
                    "output": "${.payload.result}"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn shell_execute_is_not_mapped_to_c8y_when_disabled() {
    let ttd = TempTedgeDir::new();
    let mut config = test_mapper_config(&ttd);
    config.capabilities.shell_execute = false;

    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // No operation template is deployed when the feature is disabled
    assert!(!ttd
        .path()
        .join("operations/c8y/c8y_Command.template")
        .exists());

    // The shell_execute capability is ignored, while the other capabilities are still registered
    let capability_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/shell_execute"),
        "{}",
    );
    mqtt.send(capability_message).await.unwrap();
    let capability_message =
        MqttMessage::new(&Topic::new_unchecked("te/device/main///cmd/restart"), "{}");
    mqtt.send(capability_message).await.unwrap();

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "114,c8y_Restart")]).await;
}

#[tokio::test]
async fn c8y_command_is_ignored_when_shell_execute_is_disabled() {
    // The operation template and file are deployed, as they would be on a device
    // where the feature has been disabled after having been used
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command.template")
        .with_raw_content(crate::shell_execute::SHELL_EXECUTE_TEMPLATE);
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(crate::shell_execute::SHELL_EXECUTE_TEMPLATE);

    let mut config = test_mapper_config(&ttd);
    config.capabilities.shell_execute = false;

    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
            "status": "PENDING",
            "id": "1234",
            "c8y_Command": {
                "text": "echo hello"
            },
            "externalSource": {
                "externalId": "test-device",
                "type": "c8y_Serial"
            }
        })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    // The operation is ignored: no shell_execute command is created.
    // A restart operation sent afterwards is still converted, proving the mapper is alive
    // and that the assertion below is not just observing a slow mapper.
    let restart_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
            "status": "PENDING",
            "id": "5678",
            "c8y_Restart": {},
            "externalSource": {
                "externalId": "test-device",
                "type": "c8y_Serial"
            }
        })
        .to_string(),
    );
    mqtt.send(restart_message).await.expect("Send failed");

    let mut seen = Vec::new();
    loop {
        let message = mqtt
            .recv()
            .await
            .expect("the restart command to be received");
        let topic = message.topic.name.clone();
        assert!(
            !topic.contains("cmd/shell_execute"),
            "the disabled shell_execute command has been triggered: {seen:?}"
        );
        if topic.contains("cmd/restart/") {
            break;
        }
        seen.push(topic);
    }
}

/// Sends the given Cumulocity operation, with the ID 1234, to a device supporting the `command` command,
/// mapping the operation to this command with the given operation template.
/// The name of the operation is the key of its single fragment.
async fn send_workflow_operation(
    device: &str,
    template: &str,
    mut operation: serde_json::Value,
) -> (
    TestMapper,
    TestMqttBox<TimedMessageBox<MockMqttBoxUnbuffered>>,
) {
    let c8y_operation = operation
        .as_object()
        .and_then(|fragments| fragments.keys().next())
        .expect("an operation with a single fragment")
        .clone();
    let c8y_operation = c8y_operation.as_str();
    let dir = TempTedgeDir::new();
    dir.dir("operations")
        .dir("c8y")
        .file(&format!("{c8y_operation}.template"))
        .with_raw_content(template);

    let config = test_mapper_config(&dir);
    let (mapper, mut mqtt) = spawn_mapper_with_config(dir, config).await;

    let external_id = declare_command_capability(&mapper, &mut mqtt, device, c8y_operation).await;

    operation["status"] = "PENDING".into();
    operation["id"] = "1234".into();
    operation["externalSource"] = json!({
        "externalId": external_id,
        "type": "c8y_Serial"
    });
    publish(
        &mut mqtt,
        "c8y/devicecontrol/notifications",
        operation.to_string(),
    )
    .await;

    (mapper, mqtt)
}

/// Declares the support of the `command` command by the given device,
/// checking that the given Cumulocity operation is then supported by the device,
/// and returns the external id of the device
async fn declare_command_capability(
    mapper: &TestMapper,
    mqtt: &mut TestMqttBox<TimedMessageBox<MockMqttBoxUnbuffered>>,
    device: &str,
    c8y_operation: &str,
) -> String {
    let (external_id, smartrest_topic, operation_link) = if device == "main" {
        (
            "test-device",
            "c8y/s/us".to_string(),
            c8y_operation.to_string(),
        )
    } else {
        let registration = json!({
            "@type":"child-device",
            "@parent":"device/main//",
            "@id": device
        });
        publish(
            mqtt,
            &format!("te/device/{device}//"),
            registration.to_string(),
        )
        .await;
        (
            device,
            format!("c8y/s/us/{device}"),
            format!("{device}/{c8y_operation}"),
        )
    };

    publish(mqtt, &format!("te/device/{device}///cmd/command"), "{}").await;
    let supported_operation = format!("114,{c8y_operation}");
    assert_received_contains_str(
        mqtt,
        [(smartrest_topic.as_str(), supported_operation.as_str())],
    )
    .await;
    assert!(mapper
        .path()
        .join(format!("operations/c8y/{operation_link}"))
        .is_symlink());

    external_id.to_string()
}
