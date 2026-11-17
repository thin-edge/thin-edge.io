use super::*;
use test_case::test_case;

#[test_case(
    None,
    r#"#!/bin/sh
    for i in $(seq 1 2)
    do
        sleep 1
    done
    echo "Executed successfully without timeout"
    "#,
    "503,c8y_Command,\"Executed successfully without timeout\n\"",
    "command \"511,test-device,c8y_Command\"
Exit status: 0 (OK)

stderr (EMPTY)

stdout <<EOF
Executed successfully without timeout
EOF
"
    ; "successful without timeout"
)]
#[test_case(
    Some((5, 2)),
    r#"#!/bin/sh
    for i in $(seq 1 2)
    do
        sleep 1
    done
    echo "Successfully Executed"
    "#,
    "503,c8y_Command,\"Successfully Executed\n\"",
    "command \"511,test-device,c8y_Command\"
Exit status: 0 (OK)

stderr (EMPTY)

stdout <<EOF
Successfully Executed
EOF
"
    ; "successful within the timeout"
)]
// Block on `sleep & wait` rather than a foreground `sleep`:
// the `wait` builtin is interrupted by a trapped signal
// (a foreground `sleep` would defer it until the sleep completes).
#[test_case(
    Some((2, 2)),
    r#"#!/bin/sh
    trap 'echo received SIGTERM; exit 124' TERM
    for i in $(seq 1 10)
    do
        echo "main $i"
        sleep 5 >/dev/null 2>&1 &
        wait
    done
    "#,
    "502,c8y_Command,operation failed due to timeout: duration=2s",
    "command \"511,test-device,c8y_Command\"
Exit status: 124 (ERROR)

stderr <<EOF
operation failed due to timeout: duration=2s
EOF

stdout <<EOF
main 1
received SIGTERM
EOF
"
    ; "stopped by sigterm on timeout"
)]
#[test_case(
    Some((2, 2)),
    r#"#!/bin/sh
    trap 'echo ignore SIGTERM' TERM
    for i in $(seq 1 50)
    do
        echo "main $i"
        sleep 5 >/dev/null 2>&1 &
        wait
    done
    "#,
    "502,c8y_Command,operation failed due to timeout: duration=2s",
    "command \"511,test-device,c8y_Command\"
Killed by signal: 9

stderr <<EOF
operation failed due to timeout: duration=2s
EOF

stdout <<EOF
main 1
ignore SIGTERM
main 2
EOF
"
    ; "killed by sigkill when ignoring sigterm"
)]
#[tokio::test]
async fn custom_operation_runs_its_script(
    timeouts: Option<(i64, i64)>,
    script: &str,
    expected_result: &str,
    expected_log: &str,
) {
    let (mapper, mut mqtt) = run_smartrest_command(timeouts, script, |config| config).await;

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_Command")]).await;
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", expected_result)]).await;
    assert_command_exec_log_content(&mapper, expected_log);
}

#[tokio::test]
async fn custom_operation_fails_when_its_command_cannot_be_killed() {
    let (_mapper, mut mqtt) =
        run_smartrest_command(Some((1, 1)), UNKILLABLE_SCRIPT, wait_one_second_after_kill).await;

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_Command")]).await;
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "502,c8y_Command,\"failed to kill the process")],
    )
    .await;
}

#[tokio::test]
async fn json_custom_operation_fails_by_id_when_its_command_cannot_be_killed() {
    let dir = TempTedgeDir::new();
    create_unkillable_operation(&dir, r#"on_fragment = "c8y_Command""#);
    let (_mapper, mut mqtt) = publish_c8y_command(
        dir,
        |config| C8yMapperConfig {
            smartrest_use_operation_id: true,
            ..wait_one_second_after_kill(config)
        },
        "c8y/devicecontrol/notifications",
        json!({}),
    )
    .await;

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "504,1234")]).await;
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "505,1234,failed to kill the process")],
    )
    .await;
}

#[tokio::test]
async fn custom_operation_skipping_status_updates_is_not_failed_when_its_command_cannot_be_killed()
{
    let dir = TempTedgeDir::new();
    create_unkillable_operation(
        &dir,
        r#"topic = "c8y/s/ds"
on_message = "511"
skip_status_update = true"#,
    );
    let config = wait_one_second_after_kill(test_mapper_config(&dir));
    let (mapper, mut mqtt) = spawn_mapper_with_config(dir, config).await;
    publish(&mut mqtt, "c8y/s/ds", "511,test-device,c8y_Command").await;

    // The failure is logged just before it would be published
    wait_for_command_log(&mapper, "failed to kill the process").await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_received_not_contains_str(
        &mut mqtt,
        [
            ("c8y/s/us", "501,c8y_Command"),
            ("c8y/s/us", "502,c8y_Command"),
        ],
    )
    .await;
}

#[test_case(
    "c8y/devicecontrol/notifications",
    r#"[exec]
    command = "echo ${.payload.c8y_Command.text}"
    on_fragment = "c8y_Command"
    "#,
    true,
    ["504,1234", "506,1234,\"do something\n\""]
    ; "with operation id"
)]
#[test_case(
    "c8y/devicecontrol/notifications",
    r#"[exec]
    command = "echo ${.payload.c8y_Command.text}"
    on_fragment = "c8y_Command"
    "#,
    false,
    ["501,c8y_Command", "503,c8y_Command,\"do something\n\""]
    ; "with operation name"
)]
#[test_case(
    "c8y/custom/operation/one",
    r#"[exec]
    topic = "${.bridge.topic_prefix}/custom/operation/one"
    command = "echo ${.payload.c8y_Command.text}"
    on_fragment = "c8y_Command"
    "#,
    false,
    ["501,c8y_Command", "503,c8y_Command,\"do something\n\""]
    ; "with custom topic"
)]
#[tokio::test]
async fn json_custom_operation_status_update(
    topic: &str,
    operation_file: &str,
    use_operation_id: bool,
    [executing, successful]: [&str; 2],
) {
    let (_mapper, mut mqtt) = send_c8y_command(
        operation_file,
        |config| C8yMapperConfig {
            smartrest_use_operation_id: use_operation_id,
            ..config
        },
        topic,
        json!({ "text": "do something" }),
    )
    .await;

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", executing)]).await;
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", successful)]).await;
}

#[tokio::test]
async fn json_custom_operation_receives_cloud_profile_env() {
    let (_mapper, mut mqtt) = send_c8y_command(
        r#"[exec]
        command = "sh -c 'printf %s \"$TEDGE_CLOUD_PROFILE\"'"
        on_fragment = "c8y_Command"
        "#,
        |config| C8yMapperConfig {
            cloud_profile: Some(ProfileName::try_from("profile-a".to_string()).unwrap()),
            smartrest_use_operation_id: true,
            ..config
        },
        "c8y/devicecontrol/notifications",
        json!({}),
    )
    .await;

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "504,1234")]).await;
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "506,1234,profile-a")]).await;
}

#[tokio::test]
async fn custom_operation_csv_result_is_reported_without_escape_sequences() {
    let (_mapper, mut mqtt) = send_c8y_command(
        r#"[exec]
        command = "printf 'foo\\033,\\033[1mbar\\033[0m'"
        on_fragment = "c8y_Command"
        result_format = "csv"
        "#,
        |config| C8yMapperConfig {
            smartrest_use_operation_id: true,
            ..config
        },
        "c8y/devicecontrol/notifications",
        json!({}),
    )
    .await;

    // The comma following the escape character is kept as a CSV delimiter
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "504,1234")]).await;
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "506,1234,foo,bar")]).await;
}

#[tokio::test]
async fn custom_operation_result_escape_sequences_do_not_count_toward_the_size_limit() {
    // 1818 lines of 11 bytes, i.e. 19998 bytes, but only 3636 bytes without the escape sequences
    let (_mapper, mut mqtt) = send_c8y_command(
        r#"[exec]
        command = '''sh -c 'for i in $(seq 1818); do printf "\033[31mX\033[0m\n"; done' '''
        on_fragment = "c8y_Command"
        "#,
        |config| C8yMapperConfig {
            smartrest_use_operation_id: true,
            ..config
        },
        "c8y/devicecontrol/notifications",
        json!({}),
    )
    .await;

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "504,1234")]).await;
    let expected = format!("506,1234,\"{}\"", "X\n".repeat(1818));
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", expected.as_str())]).await;
}

#[tokio::test]
async fn json_custom_operation_without_cloud_profile_receives_no_env() {
    // The script wraps the profile in BEGIN/END markers so an empty value is
    // distinguishable from a leaked one under the substring-based assertion.
    let (_mapper, mut mqtt) = send_c8y_command(
        r#"[exec]
        command = "sh -c 'printf BEGIN%sEND \"$TEDGE_CLOUD_PROFILE\"'"
        on_fragment = "c8y_Command"
        "#,
        |config| C8yMapperConfig {
            cloud_profile: None,
            smartrest_use_operation_id: true,
            ..config
        },
        "c8y/devicecontrol/notifications",
        json!({}),
    )
    .await;

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "504,1234")]).await;
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "506,1234,BEGINEND")]).await;
}

#[tokio::test]
async fn json_custom_operation_status_multiple_operations_in_one_mqtt_message() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(
            r#"[exec]
            command = "echo ${.payload.c8y_Command.text}"
            on_fragment = "c8y_Command"
            "#,
        );

    let config = C8yMapperConfig {
        smartrest_use_operation_id: true,
        ..test_mapper_config(&ttd)
    };
    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // Simulate c8y_Command SmartREST request
    let operation_1 = json!({
             "status":"PENDING",
             "id": "111",
             "c8y_Command": {
                 "text": "do something \"1\""
             },
        "externalSource":{
       "externalId":"test-device",
       "type":"c8y_Serial"
    }
         })
    .to_string();
    let operation_2 = json!({
             "status":"PENDING",
             "id": "222",
             "c8y_Command": {
                 "text": "do something \"2\""
             },
        "externalSource":{
       "externalId":"test-device",
       "type":"c8y_Serial"
    }
         })
    .to_string();
    let operation_3 = json!({
             "status":"PENDING",
             "id": "333",
             "c8y_Command": {
                 "text": "do something \"3\""
             },
        "externalSource":{
       "externalId":"test-device",
       "type":"c8y_Serial"
    }
         })
    .to_string();

    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        [operation_1, operation_2, operation_3].join("\n"),
    );
    mqtt.send(input_message).await.expect("Send failed");

    let mut messages = vec![];
    for _ in 0..6 {
        messages.push(mqtt.recv().await.unwrap());
    }

    assert_received_contains_str(
        &mut mqtt,
        [
            ("c8y/s/us", "504,111"),
            ("c8y/s/us", "504,222"),
            ("c8y/s/us", "504,333"),
        ],
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [
            ("c8y/s/us", "506,111,\"do something \"\"1\"\"\n\""),
            ("c8y/s/us", "506,222,\"do something \"\"2\"\"\n\""),
            ("c8y/s/us", "506,333,\"do something \"\"3\"\"\n\""),
        ],
    )
    .await;
}

#[tokio::test]
async fn json_custom_operation_skip_status_update() {
    let (_mapper, mut mqtt) = send_c8y_command(
        r#"[exec]
        command = "echo ${.payload.c8y_Command.text}"
        on_fragment = "c8y_Command"
        skip_status_update = true
        "#,
        |config| C8yMapperConfig {
            smartrest_use_operation_id: true,
            ..config
        },
        "c8y/devicecontrol/notifications",
        json!({ "text": "do something" }),
    )
    .await;

    // No MQTT message should be sent related to the operation
    assert_received_not_contains_str(&mut mqtt, [("c8y/s/us", "1234")]).await;
}

#[test_case(
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
    ; "main device"
)]
#[test_case(
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
    ; "child device"
)]
#[test_case(
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
    ; "combined input"
)]
#[test_case(
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
    ; "without workflow input"
)]
#[tokio::test]
async fn mapper_converts_custom_operation_to_command(
    device: &str,
    template: &str,
    operation: serde_json::Value,
    expected_command: serde_json::Value,
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

pub(super) fn assert_command_exec_log_content(cfg_dir: &TempTedgeDir, expected_contents: &str) {
    let paths = fs::read_dir(cfg_dir.path().join("agent")).unwrap();
    for path in paths {
        let mut file =
            File::open(path.unwrap().path()).expect("Unable to open the command exec log file");
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .expect("Unable to read the file");
        assert!(
            contents.contains(expected_contents),
            "command exec log did not contain expected content.\n--- expected ---\n{expected_contents}\n--- actual ---\n{contents}"
        );
    }
}

pub(super) fn create_custom_op_file(
    ttd: &TempTedgeDir,
    cmd_file: &Utf8Path,
    graceful_timeout: Option<i64>,
    forceful_timeout: Option<i64>,
) {
    let custom_op_file = ttd.dir("operations").dir("c8y").file("c8y_Command");
    let mut custom_content = toml::map::Map::new();
    custom_content.insert("name".into(), toml::Value::String("c8y_Command".into()));
    custom_content.insert("topic".into(), toml::Value::String("c8y/s/ds".into()));
    custom_content.insert("on_message".into(), toml::Value::String("511".into()));
    if let Some(timeout) = graceful_timeout {
        custom_content.insert("timeout".into(), toml::Value::Integer(timeout));
    }
    if let Some(timeout) = forceful_timeout {
        custom_content.insert("forceful_timeout".into(), toml::Value::Integer(timeout));
    }
    custom_content.insert("command".into(), toml::Value::String(cmd_file.to_string()));

    let mut map = toml::map::Map::new();
    map.insert("exec".into(), toml::Value::Table(custom_content));
    custom_op_file.with_toml_content(map);
}

pub(super) fn create_custom_cmd(custom_cmd: &Utf8Path, content: &str) {
    with_exec_permission(custom_cmd, content);
}

/// Sends a `c8y_Command` operation, with the ID 1234 and the given fragment, on the given topic,
/// to a mapper with a `c8y_Command` operation file of the given content
async fn send_c8y_command(
    operation_file: &str,
    configure: impl FnOnce(C8yMapperConfig) -> C8yMapperConfig,
    topic: &str,
    c8y_command: serde_json::Value,
) -> (
    TestMapper,
    TestMqttBox<TimedMessageBox<MockMqttBoxUnbuffered>>,
) {
    let dir = TempTedgeDir::new();
    dir.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(operation_file);
    publish_c8y_command(dir, configure, topic, c8y_command).await
}

/// Sends a `c8y_Command` operation, with the ID 1234 and the given fragment, on the given topic,
/// to a mapper using the operation files of the given directory
async fn publish_c8y_command(
    dir: TempTedgeDir,
    configure: impl FnOnce(C8yMapperConfig) -> C8yMapperConfig,
    topic: &str,
    c8y_command: serde_json::Value,
) -> (
    TestMapper,
    TestMqttBox<TimedMessageBox<MockMqttBoxUnbuffered>>,
) {
    let config = configure(test_mapper_config(&dir));
    let (mapper, mut mqtt) = spawn_mapper_with_config(dir, config).await;

    let operation = json!({
        "status": "PENDING",
        "id": "1234",
        "c8y_Command": c8y_command,
        "externalSource": {
            "externalId": "test-device",
            "type": "c8y_Serial"
        }
    });
    publish(&mut mqtt, topic, operation.to_string()).await;

    (mapper, mqtt)
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
            &mut mqtt,
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

    publish(
        &mut mqtt,
        &format!("te/device/{device}///cmd/command"),
        "{}",
    )
    .await;
    let supported_operation = format!("114,{c8y_operation}");
    assert_received_contains_str(
        &mut mqtt,
        [(smartrest_topic.as_str(), supported_operation.as_str())],
    )
    .await;
    assert!(mapper
        .path()
        .join(format!("operations/c8y/{operation_link}"))
        .is_symlink());

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

/// Sends a SmartREST `c8y_Command` operation to a mapper running the given script for it,
/// with the given graceful and forceful timeouts
async fn run_smartrest_command(
    timeouts: Option<(i64, i64)>,
    script: &str,
    configure: impl FnOnce(C8yMapperConfig) -> C8yMapperConfig,
) -> (
    TestMapper,
    TestMqttBox<TimedMessageBox<MockMqttBoxUnbuffered>>,
) {
    let dir = TempTedgeDir::new();
    let cmd_file = dir.path().join("command");
    let (graceful_timeout, forceful_timeout) = timeouts.unzip();
    create_custom_op_file(&dir, cmd_file.as_path(), graceful_timeout, forceful_timeout);
    create_custom_cmd(cmd_file.as_path(), script);

    let config = configure(test_mapper_config(&dir));
    let (mapper, mut mqtt) = spawn_mapper_with_config(dir, config).await;
    publish(&mut mqtt, "c8y/s/ds", "511,test-device,c8y_Command").await;
    (mapper, mqtt)
}

/// A script that cannot be killed on timeout: its `setsid` child leaves the process group
/// killed on timeout and keeps stdout open, so the mapper never sees the script exit.
/// The child outlives the 3 seconds the mapper waits: 2 timeouts of 1 second and the exit wait.
const UNKILLABLE_SCRIPT: &str = r#"#!/bin/sh
    setsid sleep 5 &
    sleep 10
    "#;

/// Creates a `c8y_Command` operation file running [`UNKILLABLE_SCRIPT`] with timeouts of one
/// second, triggered by the given `[exec]` entries
fn create_unkillable_operation(dir: &TempTedgeDir, trigger: &str) {
    let cmd_file = dir.path().join("command");
    create_custom_cmd(cmd_file.as_path(), UNKILLABLE_SCRIPT);
    dir.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(&format!(
            "[exec]\n{trigger}\ncommand = \"{cmd_file}\"\ntimeout = 1\nforceful_timeout = 1\n"
        ));
}

/// Gives a killed command one second to exit
fn wait_one_second_after_kill(config: C8yMapperConfig) -> C8yMapperConfig {
    C8yMapperConfig {
        custom_operation_exit_wait_after_kill: Duration::from_secs(1),
        ..config
    }
}

/// Waits for a command log of the mapper to contain the given text
async fn wait_for_command_log(dir: &TempTedgeDir, expected: &str) {
    let found = tokio::time::timeout(TEST_TIMEOUT_MS, async {
        loop {
            let logged = fs::read_dir(dir.path())
                .into_iter()
                .flatten()
                .flatten()
                .any(|entry| {
                    fs::read_to_string(entry.path()).is_ok_and(|log| log.contains(expected))
                });
            if logged {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(found.is_ok(), "No command log contains {expected:?}");
}
