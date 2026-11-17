use super::*;

#[tokio::test]
async fn custom_operation_without_timeout_successful() {
    // The test assures SM Mapper correctly receives custom operation on `c8y/s/ds`
    // and executes the custom operation successfully, no timeout given here.

    let ttd = TempTedgeDir::new();

    let cmd_file = ttd.path().join("command");
    //create custom operation file
    create_custom_op_file(&ttd, cmd_file.as_path(), None, None);
    //create command
    let content = r#"#!/bin/sh
    for i in $(seq 1 2)
    do
        sleep 1
    done
    echo "Executed successfully without timeout"
    "#;
    create_custom_cmd(cmd_file.as_path(), content);

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // Simulate c8y_Command SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(&"c8y".try_into().unwrap()),
        "511,test-device,c8y_Command",
    ))
    .await
    .expect("Send failed");

    let _ = tokio::time::timeout(TEST_TIMEOUT_MS, async {
        loop {
            mqtt.recv().await;
        }
    })
    .await;

    // Expect `501` smartrest message on `c8y/s/us`.
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_Command")]).await;

    // Expect `503` smartrest message on `c8y/s/us`.
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "503,c8y_Command,\"Executed successfully without timeout\n\"",
        )],
    )
    .await;

    // assert the signterm is handled
    let expected_content = "command \"511,test-device,c8y_Command\"
Exit status: 0 (OK)

stderr (EMPTY)

stdout <<EOF
Executed successfully without timeout
EOF
";

    assert_command_exec_log_content(ttd, expected_content);
}

#[tokio::test]
async fn custom_operation_with_timeout_successful() {
    // The test assures SM Mapper correctly receives custom operation on `c8y/s/ds`
    // and executes the custom operation within the timeout period

    let ttd = TempTedgeDir::new();
    let cmd_file = ttd.path().join("command");
    //create custom operation file
    create_custom_op_file(&ttd, cmd_file.as_path(), Some(5), Some(2));
    //create command
    let content = r#"#!/bin/sh
    for i in $(seq 1 2)
    do
        sleep 1
    done
    echo "Successfully Executed"
    "#;
    create_custom_cmd(cmd_file.as_path(), content);

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // Simulate c8y_Command SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(&"c8y".try_into().unwrap()),
        "511,test-device,c8y_Command",
    ))
    .await
    .expect("Send failed");

    // Expect `501` smartrest message on `c8y/s/us`.
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_Command")]).await;

    // Expect `503` smartrest message on `c8y/s/us`.
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "503,c8y_Command,\"Successfully Executed\n\"")],
    )
    .await;

    // assert the signterm is handled
    let expected_content = "command \"511,test-device,c8y_Command\"
Exit status: 0 (OK)

stderr (EMPTY)

stdout <<EOF
Successfully Executed
EOF
";

    assert_command_exec_log_content(ttd, expected_content);
}

#[tokio::test]
async fn custom_operation_timeout_sigterm() {
    // The test assures SM Mapper correctly receives custom operation on `c8y/s/ds`
    // and executes the custom operation, it will timeout because it will not complete before given timeout
    // sigterm is sent to stop the custom operation

    let ttd = TempTedgeDir::new();
    let cmd_file = ttd.path().join("command");
    //create custom operation file
    create_custom_op_file(&ttd, cmd_file.as_path(), Some(2), Some(2));
    //create command
    // Block on `sleep & wait` rather than a foreground `sleep`:
    // the `wait` builtin is interrupted by a trapped signal
    // (a foreground `sleep` would defer it until the sleep completes).
    let content = r#"#!/bin/sh
    trap 'echo received SIGTERM; exit 124' TERM
    for i in $(seq 1 10)
    do
        echo "main $i"
        sleep 5 >/dev/null 2>&1 &
        wait
    done
    "#;
    create_custom_cmd(cmd_file.as_path(), content);

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // Simulate c8y_Command SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(&"c8y".try_into().unwrap()),
        "511,test-device,c8y_Command",
    ))
    .await
    .expect("Send failed");

    // Expect `501` smartrest message on `c8y/s/us`.
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_Command")]).await;

    // Expect `501` smartrest message on `c8y/s/us`.
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "502,c8y_Command,operation failed due to timeout: duration=2s",
        )],
    )
    .await;

    // assert the signterm is handled
    let expected_content = "command \"511,test-device,c8y_Command\"
Exit status: 124 (ERROR)

stderr <<EOF
operation failed due to timeout: duration=2s
EOF

stdout <<EOF
main 1
received SIGTERM
EOF
";

    assert_command_exec_log_content(ttd, expected_content);
}

#[tokio::test]
async fn custom_operation_fails_when_its_command_cannot_be_killed() {
    let ttd = TempTedgeDir::new();
    let cmd_file = create_unkillable_command(&ttd);
    create_custom_op_file(&ttd, cmd_file.as_path(), Some(1), Some(1));

    let config = config_waiting_one_second_after_kill(&ttd);
    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(&"c8y".try_into().unwrap()),
        "511,test-device,c8y_Command",
    ))
    .await
    .expect("Send failed");

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_Command")]).await;
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "502,c8y_Command,\"failed to kill the process")],
    )
    .await;
}

#[tokio::test]
async fn custom_operation_timeout_sigkill() {
    // The test assures SM Mapper correctly receives custom operation on `c8y/s/ds`
    // and executes the custom operation, it will timeout because it will not complete before given timeout
    // sigterm sent first, still the operation did not stop, so sigkill will be sent to stop the operation

    let ttd = TempTedgeDir::new();

    let cmd_file = ttd.path().join("command");
    //create custom operation file
    create_custom_op_file(&ttd, cmd_file.as_path(), Some(2), Some(2));
    //create command
    // Block on `sleep & wait` rather than a foreground `sleep`:
    // the `wait` builtin is interrupted by a trapped signal
    // (a foreground `sleep` would defer it until the sleep completes).
    let content = r#"#!/bin/sh
    trap 'echo ignore SIGTERM' TERM
    for i in $(seq 1 50)
    do
        echo "main $i"
        sleep 5 >/dev/null 2>&1 &
        wait
    done
    "#;
    create_custom_cmd(cmd_file.as_path(), content);

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // Simulate c8y_Command SmartREST request
    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(&"c8y".try_into().unwrap()),
        "511,test-device,c8y_Command",
    ))
    .await
    .expect("Send failed");

    // Expect `501` smartrest message on `c8y/s/us`.
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_Command")]).await;

    // Expect `502` smartrest message on `c8y/s/us`.
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "502,c8y_Command,operation failed due to timeout: duration=2s",
        )],
    )
    .await;

    // assert the signterm is handled
    let expected_content = "command \"511,test-device,c8y_Command\"
Killed by signal: 9

stderr <<EOF
operation failed due to timeout: duration=2s
EOF

stdout <<EOF
main 1
ignore SIGTERM
main 2
EOF
";

    assert_command_exec_log_content(ttd, expected_content);
}

#[tokio::test]
async fn json_custom_operation_status_update_with_operation_id() {
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
    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_Command": {
                     "text": "do something"
                 },
            "externalSource":{
           "externalId":"test-device",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "504,1234")]).await;
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "506,1234,\"do something\n\"")]).await;
}

#[tokio::test]
async fn json_custom_operation_receives_cloud_profile_env() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(
            r#"[exec]
            command = "sh -c 'printf %s \"$TEDGE_CLOUD_PROFILE\"'"
            on_fragment = "c8y_Command"
            "#,
        );

    let config = C8yMapperConfig {
        cloud_profile: Some(ProfileName::try_from("profile-a".to_string()).unwrap()),
        smartrest_use_operation_id: true,
        ..test_mapper_config(&ttd)
    };
    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
            "status":"PENDING",
            "id": "1234",
            "c8y_Command": {},
            "externalSource":{
                "externalId":"test-device",
                "type":"c8y_Serial"
            }
        })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "504,1234")]).await;
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "506,1234,profile-a")]).await;
}

#[tokio::test]
async fn custom_operation_csv_result_is_reported_without_escape_sequences() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(
            r#"[exec]
            command = "printf 'foo\\033,\\033[1mbar\\033[0m'"
            on_fragment = "c8y_Command"
            result_format = "csv"
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
    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
            "status":"PENDING",
            "id": "1234",
            "c8y_Command": {},
            "externalSource":{
                "externalId":"test-device",
                "type":"c8y_Serial"
            }
        })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    // The comma following the escape character is kept as a CSV delimiter
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "504,1234")]).await;
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "506,1234,foo,bar")]).await;
}

#[tokio::test]
async fn custom_operation_result_escape_sequences_do_not_count_toward_the_size_limit() {
    let ttd = TempTedgeDir::new();
    // 1818 lines of 11 bytes, i.e. 19998 bytes, but only 3636 bytes without the escape sequences
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(
            r#"[exec]
            command = '''sh -c 'for i in $(seq 1818); do printf "\033[31mX\033[0m\n"; done' '''
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
    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
            "status":"PENDING",
            "id": "1234",
            "c8y_Command": {},
            "externalSource":{
                "externalId":"test-device",
                "type":"c8y_Serial"
            }
        })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "504,1234")]).await;
    let expected = format!("506,1234,\"{}\"", "X\n".repeat(1818));
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", expected.as_str())]).await;
}

#[tokio::test]
async fn json_custom_operation_without_cloud_profile_receives_no_env() {
    let ttd = TempTedgeDir::new();
    // The script wraps the profile in BEGIN/END markers so an empty value is
    // distinguishable from a leaked one under the substring-based assertion.
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(
            r#"[exec]
            command = "sh -c 'printf BEGIN%sEND \"$TEDGE_CLOUD_PROFILE\"'"
            on_fragment = "c8y_Command"
            "#,
        );

    let config = C8yMapperConfig {
        cloud_profile: None,
        smartrest_use_operation_id: true,
        ..test_mapper_config(&ttd)
    };
    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);
    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
            "status":"PENDING",
            "id": "1234",
            "c8y_Command": {},
            "externalSource":{
                "externalId":"test-device",
                "type":"c8y_Serial"
            }
        })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

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
async fn json_custom_operation_status_update_with_operation_name() {
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
        smartrest_use_operation_id: false,
        ..test_mapper_config(&ttd)
    };
    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // Simulate c8y_Command SmartREST request
    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_Command": {
                     "text": "do something"
                 },
            "externalSource":{
           "externalId":"test-device",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_Command")]).await;
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "503,c8y_Command,\"do something\n\"")],
    )
    .await;
}

#[tokio::test]
async fn json_custom_operation_skip_status_update() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(
            r#"[exec]
            command = "echo ${.payload.c8y_Command.text}"
            on_fragment = "c8y_Command"
            skip_status_update = true
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
    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_Command": {
                     "text": "do something"
                 },
            "externalSource":{
           "externalId":"TST_haul_searing_set",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    // No MQTT message should be sent related to the operation
    assert_received_not_contains_str(&mut mqtt, [("c8y/s/us", "1234")]).await;
}

#[tokio::test]
async fn json_custom_operation_status_update_with_custom_topic() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(
            r#"[exec]
            topic = "${.bridge.topic_prefix}/custom/operation/one"
            command = "echo ${.payload.c8y_Command.text}"
            on_fragment = "c8y_Command"
            "#,
        );

    let config = C8yMapperConfig {
        smartrest_use_operation_id: false,
        ..test_mapper_config(&ttd)
    };
    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // Simulate c8y_Command SmartREST request
    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/custom/operation/one"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_Command": {
                     "text": "do something"
                 },
            "externalSource":{
           "externalId":"test-device",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_Command")]).await;
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "503,c8y_Command,\"do something\n\"")],
    )
    .await;
}

#[tokio::test]
async fn mapper_converts_custom_operation_for_main_device() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command.template")
        .with_raw_content(
            r#"[exec]
            topic = "c8y/devicecontrol/notifications"
            on_fragment = "c8y_Command"

            [exec.workflow]
            operation = "command"
            input = "${.payload.c8y_Command}"
            output = "${.payload.result}"
            "#,
        );

    let config = test_mapper_config(&ttd);

    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // indicate that main device supports the operation
    let capability_message =
        MqttMessage::new(&Topic::new_unchecked("te/device/main///cmd/command"), "{}");

    mqtt.send(capability_message).await.unwrap();

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "114,c8y_Command")]).await;

    assert!(ttd.path().join("operations/c8y/c8y_Command").is_symlink());

    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_Command": {
                     "text": "do something"
                 },
            "externalSource":{
           "externalId":"test-device",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/main///cmd/command/c8y-mapper-1234",
            json!({
                "status": "init",
                "text": "do something",
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

#[tokio::test]
async fn mapper_converts_custom_operation_with_combined_input() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_CombinedInput.template")
        .with_raw_content(
            r#"[exec]
            topic = "c8y/devicecontrol/notifications"
            on_fragment = "c8y_CombinedInput"

            [exec.workflow]
            operation = "command"
            input.x = "${.payload.c8y_CombinedInput.inner.x}"
            input.y = "${.payload.c8y_CombinedInput.inner.y}"
            input.z = { foo = "bar" }
            "#,
        );

    let config = test_mapper_config(&ttd);

    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // indicate that main device supports the operation
    let capability_message =
        MqttMessage::new(&Topic::new_unchecked("te/device/main///cmd/command"), "{}");

    mqtt.send(capability_message).await.unwrap();

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "114,c8y_CombinedInput")]).await;

    assert!(ttd
        .path()
        .join("operations/c8y/c8y_CombinedInput")
        .is_symlink());

    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_CombinedInput": {
                     "text": "do something",
                     "inner": {
                        "x": "x value",
                        "y": 42,
                        "z": "z unused value",
                     },
                 },
            "externalSource":{
           "externalId":"test-device",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/main///cmd/command/c8y-mapper-1234",
            json!({
                "status": "init",
                "x": "x value",
                "y": 42,
                "z": {
                    "foo": "bar"
                },
                "c8y-mapper": {
                    "on_fragment": "c8y_CombinedInput"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_converts_custom_operation_for_main_device_without_workflow_input() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command.template")
        .with_raw_content(
            r#"[exec]
            topic = "c8y/devicecontrol/notifications"
            on_fragment = "c8y_Command"

            [exec.workflow]
            operation = "command"
            "#,
        );

    let config = test_mapper_config(&ttd);

    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // indicate that main device supports the operation
    let capability_message =
        MqttMessage::new(&Topic::new_unchecked("te/device/main///cmd/command"), "{}");

    mqtt.send(capability_message).await.unwrap();

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "114,c8y_Command")]).await;

    assert!(ttd.path().join("operations/c8y/c8y_Command").is_symlink());

    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_Command": {
                     "text": "do something"
                 },
            "externalSource":{
           "externalId":"test-device",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/main///cmd/command/c8y-mapper-1234",
            json!({
                "status": "init",
                "c8y-mapper": {
                    "on_fragment": "c8y_Command"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_converts_custom_operation_for_main_device_with_invalid_workflow_input() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command.template")
        .with_raw_content(
            r#"[exec]
            topic = "c8y/devicecontrol/notifications"
            on_fragment = "c8y_Command"

            [exec.workflow]
            operation = "command"
            input = "invalid input"
            "#,
        );

    let config = test_mapper_config(&ttd);

    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // indicate that main device supports the operation
    let capability_message =
        MqttMessage::new(&Topic::new_unchecked("te/device/main///cmd/command"), "{}");

    mqtt.send(capability_message).await.unwrap();

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "114,c8y_Command")]).await;

    assert!(ttd.path().join("operations/c8y/c8y_Command").is_symlink());

    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_Command": {
                     "text": "do something"
                 },
            "externalSource":{
           "externalId":"test-device",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    // message should not be sent due to incorrect payload
    assert!(mqtt.recv().await.is_none());
}

#[tokio::test]
async fn mapper_converts_custom_operation_for_child_device() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command.template")
        .with_raw_content(
            r#"[exec]
            topic = "c8y/devicecontrol/notifications"
            on_fragment = "c8y_Command"

            [exec.workflow]
            operation = "command"
            input = "${.payload.c8y_Command}"
            "#,
        );

    let config = test_mapper_config(&ttd);

    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    //register child device
    let reg_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1//"),
        json!({
            "@type":"child-device",
            "@parent":"device/main//",
            "@id":"child1"
        })
        .to_string(),
    );
    mqtt.send(reg_message).await.unwrap();

    // indicate that child device supports the operation
    let capability_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child1///cmd/command"),
        "{}",
    );

    mqtt.send(capability_message).await.unwrap();

    assert_received_contains_str(&mut mqtt, [("c8y/s/us/child1", "114,c8y_Command")]).await;

    assert!(ttd
        .path()
        .join("operations/c8y/child1/c8y_Command")
        .is_symlink());

    let input_message = MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        json!({
                 "status":"PENDING",
                 "id": "1234",
                 "c8y_Command": {
                     "text": "do something"
                 },
            "externalSource":{
           "externalId":"child1",
           "type":"c8y_Serial"
        }
             })
        .to_string(),
    );
    mqtt.send(input_message).await.expect("Send failed");

    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/child1///cmd/command/c8y-mapper-1234",
            json!({
                "status": "init",
                "text": "do something",
                "c8y-mapper": {
                    "on_fragment": "c8y_Command"
                }
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn json_custom_operation_fails_by_id_when_its_command_cannot_be_killed() {
    let ttd = TempTedgeDir::new();
    let cmd_file = create_unkillable_command(&ttd);
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(&format!(
            r#"[exec]
            command = "{cmd_file}"
            on_fragment = "c8y_Command"
            timeout = 1
            forceful_timeout = 1
            "#
        ));

    let config = C8yMapperConfig {
        smartrest_use_operation_id: true,
        ..config_waiting_one_second_after_kill(&ttd)
    };
    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    let operation = json!({
        "status": "PENDING",
        "id": "1234",
        "c8y_Command": {},
        "externalSource": {"externalId": "test-device", "type": "c8y_Serial"}
    });
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked("c8y/devicecontrol/notifications"),
        operation.to_string(),
    ))
    .await
    .expect("Send failed");

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
    let ttd = TempTedgeDir::new();
    let cmd_file = create_unkillable_command(&ttd);
    ttd.dir("operations")
        .dir("c8y")
        .file("c8y_Command")
        .with_raw_content(&format!(
            r#"[exec]
            topic = "c8y/s/ds"
            on_message = "511"
            command = "{cmd_file}"
            timeout = 1
            forceful_timeout = 1
            skip_status_update = true
            "#
        ));

    let config = config_waiting_one_second_after_kill(&ttd);
    let test_handle = spawn_c8y_mapper_actor_with_config(&ttd, config, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    mqtt.send(MqttMessage::new(
        &C8yTopic::downstream_topic(&"c8y".try_into().unwrap()),
        "511,test-device,c8y_Command",
    ))
    .await
    .expect("Send failed");

    // The failure is logged just before it would be published
    wait_for_command_log(&ttd, "failed to kill the process").await;
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

/// Creates a command that cannot be killed on timeout: its `setsid` child leaves the process group
/// killed on timeout and keeps stdout open, so the mapper never sees the command exit
fn create_unkillable_command(ttd: &TempTedgeDir) -> Utf8PathBuf {
    let cmd_file = ttd.path().join("command");
    // The `setsid` child outlives the 3 seconds the mapper waits for the command: 2 timeouts of 1 second and the exit wait
    let content = r#"#!/bin/sh
    setsid sleep 5 &
    sleep 10
    "#;
    create_custom_cmd(cmd_file.as_path(), content);
    cmd_file
}

/// The test configuration of the mapper, giving a killed command one second to exit
fn config_waiting_one_second_after_kill(ttd: &TempTedgeDir) -> C8yMapperConfig {
    C8yMapperConfig {
        custom_operation_exit_wait_after_kill: Duration::from_secs(1),
        ..test_mapper_config(ttd)
    }
}

/// Waits for a command log of the mapper to contain the given text
async fn wait_for_command_log(ttd: &TempTedgeDir, expected: &str) {
    let log_dir = ttd.path();
    let found = tokio::time::timeout(TEST_TIMEOUT_MS, async {
        loop {
            let logged = fs::read_dir(log_dir)
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

pub(super) fn assert_command_exec_log_content(cfg_dir: TempTedgeDir, expected_contents: &str) {
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
