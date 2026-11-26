use super::*;

#[tokio::test]
async fn mapper_publishes_supported_operations() {
    // The test assures tedge-mapper reads/parses the operations from operations directory and
    // correctly publishes the supported operations message on `c8y/s/us`
    // and verifies the supported operations that are published by the tedge-mapper.
    let ttd = TempTedgeDir::new();
    create_thin_edge_operations(&ttd, vec!["c8y_TestOp1", "c8y_TestOp2"]);

    let config = test_mapper_config(&ttd);
    let (_mapper, mut mqtt) = spawn_mapper_with_config(ttd, config).await;

    // Expect smartrest message on `c8y/s/us` with expected payload "114,c8y_TestOp1,c8y_TestOp2"
    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "114,c8y_TestOp1,c8y_TestOp2")]).await;
}

#[tokio::test]
async fn mapper_dynamically_updates_supported_operations_for_tedge_device() {
    // The test assures tedge-mapper checks if there are operations, then it reads and
    // correctly publishes them on to `c8y/s/us`.
    // When mapper is running test adds a new operation into the operations directory, then the mapper discovers the new
    // operation and publishes list of supported operation including the new operation, and verifies the device create message.
    let ttd = TempTedgeDir::new();
    create_thin_edge_operations(&ttd, vec!["c8y_TestOp1", "c8y_TestOp2"]);

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, mut fs, .. } = test_handle;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // Register the device upfront
    publish(
        &mut mqtt,
        "te/device/main/service/tedge-agent",
        r#"{"@type": "service"}"#,
    )
    .await;

    // Simulate tedge-agent health status message
    mqtt.send(
        MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/tedge-agent/status/health"),
            "{\"status\":\"up\"}",
        )
        .with_retain(),
    )
    .await
    .expect("Send failed");

    // Simulate FsEvent for the creation of a new operation file
    fs.send(FsWatchEvent::FileCreated(
        ttd.dir("operations")
            .dir("c8y")
            .file("c8y_TestOp3")
            .std_path()
            .to_path_buf(),
    ))
    .await
    .expect("Send failed");

    // Expect smartrest message on `c8y/s/us` with expected payload "114,c8y_TestOp1,c8y_TestOp2,c8y_TestOp3".
    assert_received_contains_str(
        &mut mqtt,
        [("c8y/s/us", "114,c8y_TestOp1,c8y_TestOp2,c8y_TestOp3")],
    )
    .await;

    // Then the agent start adding it's own set of capabilities
    mqtt.send(
        MqttMessage::new(&Topic::new_unchecked("te/device/main///cmd/restart"), "{}").with_retain(),
    )
    .await
    .expect("Send failed");
    mqtt.send(
        MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/software_update"),
            "{}",
        )
        .with_retain(),
    )
    .await
    .expect("Send failed");

    // Expect an update list of capabilities with agent capabilities
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "114,c8y_Restart,c8y_TestOp1,c8y_TestOp2,c8y_TestOp3",
        )],
    )
    .await;
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "114,c8y_Restart,c8y_SoftwareUpdate,c8y_TestOp1,c8y_TestOp2,c8y_TestOp3",
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_dynamically_updates_supported_operations_for_child_device() {
    // The test assures tedge-mapper reads the operations for the child devices from the operations directory, and then it publishes them on to `c8y/s/us/child1`.
    // When mapper is running test adds a new operation for a child into the operations directory, then the mapper discovers the new
    // operation and publishes list of supported operation for the child device including the new operation, and verifies the device create message.
    let ttd = TempTedgeDir::new();
    create_thin_edge_child_operations(
        &ttd,
        "test-device:device:child1",
        vec!["c8y_ChildTestOp1", "c8y_ChildTestOp2"],
    );

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, mut fs, .. } = test_handle;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // Register the device upfront
    publish(
        &mut mqtt,
        "te/device/child1//",
        r#"{"@type": "child-device"}"#,
    )
    .await;

    // Assert that the creation of the operation file alone doesn't trigger the supported operations update

    // first, if any 114 messages have been sent up until now, ignore them
    mqtt.ignore_received(("c8y/s/us/test-device:device:child1", "114"))
        .await;

    // Then add a new operation for the child device
    // Simulate FsEvent for the creation of a new operation file
    fs.send(FsWatchEvent::FileCreated(
        ttd.dir("operations")
            .dir("c8y")
            .dir("test-device:device:child1")
            .file("c8y_ChildTestOp3")
            .std_path()
            .to_path_buf(),
    ))
    .await
    .expect("Send failed");

    // Finally assert that creating the file did not trigger the update
    assert_received_not_contains_str(&mut mqtt, [("c8y/s/us/test-device:device:child1", "114")])
        .await;

    // Send any command metadata message to trigger the supported operations update
    mqtt.send(
        MqttMessage::new(
            &Topic::new_unchecked("te/device/child1///cmd/restart"),
            "{}",
        )
        .with_retain(),
    )
    .await
    .expect("Send failed");

    // Expect an update list of capabilities with agent capabilities
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child1",
            "114,c8y_ChildTestOp1,c8y_ChildTestOp2,c8y_ChildTestOp3,c8y_Restart",
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_dynamically_updates_supported_operations_for_nested_child_device() {
    // The test assures tedge-mapper reads the operations for the child devices from the operations directory, and then it publishes them on to `c8y/s/us/child1`.
    // When mapper is running test adds a new operation for a child into the operations directory, then the mapper discovers the new
    // operation and publishes list of supported operation for the child device including the new operation, and verifies the device create message.
    let ttd = TempTedgeDir::new();
    create_thin_edge_child_operations(
        &ttd,
        "child11",
        vec!["c8y_ChildTestOp1", "c8y_ChildTestOp2"],
    );

    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, mut fs, .. } = test_handle;
    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // Register nested child device
    mqtt.send(
        MqttMessage::new(
            &Topic::new_unchecked("te/device/child1//"),
            json!({
                "@type":"child-device",
                "@id": "child1",
                "name": "child1"
            })
            .to_string(),
        )
        .with_retain(),
    )
    .await
    .expect("Send failed");
    mqtt.send(
        MqttMessage::new(
            &Topic::new_unchecked("te/device/child11//"),
            json!({
                "@type":"child-device",
                "@id": "child11",
                "name": "child11",
                "@parent": "device/child1//"
            })
            .to_string(),
        )
        .with_retain(),
    )
    .await
    .expect("Send failed");

    assert_received_contains_str(
        &mut mqtt,
        [
            ("c8y/s/us", "101,child1,child1,thin-edge.io-child,false"),
            (
                "c8y/s/us/child1",
                "101,child11,child11,thin-edge.io-child,false",
            ),
        ],
    )
    .await;

    // Add a new operation for the child device
    // Simulate FsEvent for the creation of a new operation file
    fs.send(FsWatchEvent::FileCreated(
        ttd.dir("operations")
            .dir("c8y")
            .dir("child11")
            .file("c8y_ChildTestOp3")
            .std_path()
            .to_path_buf(),
    ))
    .await
    .expect("Send failed");

    // Assert that the creation of the operation file alone doesn't trigger the supported operations update
    assert!(
        mqtt.recv().await.is_none(),
        "No messages expected on operation file creation event"
    );

    // Send any command metadata message to trigger the supported operations update
    mqtt.send(
        MqttMessage::new(
            &Topic::new_unchecked("te/device/child11///cmd/restart"),
            "{}",
        )
        .with_retain(),
    )
    .await
    .expect("Send failed");

    // Expect an update list of capabilities with agent capabilities
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/child11",
            "114,c8y_ChildTestOp1,c8y_ChildTestOp2,c8y_ChildTestOp3,c8y_Restart",
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_converts_config_metadata_to_supported_op_and_types_for_main_device() {
    let (ttd, mut mqtt) = spawn_mapper().await;

    // Simulate config_snapshot cmd metadata message
    publish(
        &mut mqtt,
        "te/device/main///cmd/config_snapshot",
        r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
    )
    .await;

    // Validate SmartREST message is published
    assert_received_contains_str(
        &mut mqtt,
        [
            ("c8y/s/us", "114,c8y_UploadConfigFile"),
            ("c8y/s/us", "119,typeA,typeB,typeC"),
        ],
    )
    .await;

    // Validate if the supported operation file is created
    assert!(ttd
        .path()
        .join("operations/c8y/c8y_UploadConfigFile")
        .exists());

    // Simulate config_update cmd metadata message
    publish(
        &mut mqtt,
        "te/device/main///cmd/config_update",
        r#"{"types" : [ "typeD", "typeE", "typeF" ]}"#,
    )
    .await;

    // Validate SmartREST message is published
    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us",
                "114,c8y_DownloadConfigFile,c8y_UploadConfigFile",
            ),
            ("c8y/s/us", "119,typeD,typeE,typeF"),
        ],
    )
    .await;

    // Validate if the supported operation file is created
    assert!(ttd
        .path()
        .join("operations/c8y/c8y_DownloadConfigFile")
        .exists());
}

#[tokio::test]
async fn mapper_converts_config_cmd_to_supported_op_and_types_for_child_device() {
    let (ttd, mut mqtt) = spawn_mapper().await;

    // Register the device upfront
    publish(
        &mut mqtt,
        "te/device/child1//",
        r#"{"@type": "child-device"}"#,
    )
    .await;

    // Simulate config_snapshot cmd metadata message
    publish(
        &mut mqtt,
        "te/device/child1///cmd/config_snapshot",
        r#"{"types" : [ "typeA", "typeB", "typeC" ]}"#,
    )
    .await;

    // Validate SmartREST message is published
    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us/test-device:device:child1",
                "114,c8y_UploadConfigFile",
            ),
            (
                "c8y/s/us/test-device:device:child1",
                "119,typeA,typeB,typeC",
            ),
        ],
    )
    .await;

    // Validate if the supported operation file is created
    assert!(ttd
        .path()
        .join("operations/c8y/test-device:device:child1/c8y_UploadConfigFile")
        .exists());

    // Sending an updated list of config types
    publish(
        &mut mqtt,
        "te/device/child1///cmd/config_snapshot",
        r#"{"types" : [ "typeB", "typeC", "typeD" ]}"#,
    )
    .await;

    // Assert that the updated config type list does not trigger a duplicate supported ops message
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child1",
            "119,typeB,typeC,typeD",
        )],
    )
    .await;

    // Simulate config_update cmd metadata message
    publish(
        &mut mqtt,
        "te/device/child1///cmd/config_update",
        r#"{"types" : [ "typeD", "typeE", "typeF" ]}"#,
    )
    .await;

    // Validate SmartREST message is published
    assert_received_contains_str(
        &mut mqtt,
        [
            (
                "c8y/s/us/test-device:device:child1",
                "114,c8y_DownloadConfigFile,c8y_UploadConfigFile",
            ),
            (
                "c8y/s/us/test-device:device:child1",
                "119,typeD,typeE,typeF",
            ),
        ],
    )
    .await;

    // Validate if the supported operation file is created
    assert!(ttd
        .path()
        .join("operations/c8y/test-device:device:child1/c8y_DownloadConfigFile")
        .exists());

    // Sending an updated list of config types
    publish(
        &mut mqtt,
        "te/device/child1///cmd/config_update",
        r#"{"types" : [ "typeB", "typeC", "typeD" ]}"#,
    )
    .await;

    // Assert that the updated config type list does not trigger a duplicate supported ops message
    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child1",
            "119,typeB,typeC,typeD",
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_publishes_all_supported_operations_on_signal() {
    let ttd = TempTedgeDir::new();
    ttd.dir("operations").dir("c8y").file("c8y_Restart");
    ttd.dir("operations")
        .dir("c8y")
        .dir("test-device:device:child01");
    ttd.dir("operations")
        .dir("c8y")
        .dir("test-device:device:child02")
        .file("c8y_Restart");
    ttd.dir("operations")
        .dir("c8y")
        .dir("test-device:device:child02")
        .file("c8y_UploadConfigFile");
    ttd.dir("operations")
        .dir("c8y")
        .dir("test-device:device:main:service:collectd")
        .file("c8y_ServiceCommand");

    let config = test_mapper_config(&ttd);
    let (_mapper, mut mqtt) = spawn_mapper_with_config(ttd, config).await;

    // Register the tedge-mapper-c8y service upfront
    publish(&mut mqtt, "te/device/main/service/tedge-mapper-c8y", r#"{"@parent":"device/main//","@type":"service","name":"tedge-mapper-c8y","type":"service"}"#).await;

    // Also register other entities
    publish(
        &mut mqtt,
        "te/device/child01//",
        r#"{"@parent":"device/main//","@type":"child-device"}"#,
    )
    .await;
    publish(
        &mut mqtt,
        "te/device/child02//",
        r#"{"@parent":"device/main//","@type":"child-device"}"#,
    )
    .await;
    publish(
        &mut mqtt,
        "te/device/main/service/collectd",
        r#"{"@parent":"device/main//","@type":"service"}"#,
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/main/service/tedge-mapper-c8y/signal/sync",
        "{}",
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [
            ("c8y/s/us", "114,c8y_Restart"),
            ("c8y/s/us/test-device:device:child01", "114"),
            (
                "c8y/s/us/test-device:device:child02",
                "114,c8y_Restart,c8y_UploadConfigFile",
            ),
            (
                "c8y/s/us/test-device:device:main:service:collectd",
                "114,c8y_ServiceCommand",
            ),
        ],
    )
    .await;
}
