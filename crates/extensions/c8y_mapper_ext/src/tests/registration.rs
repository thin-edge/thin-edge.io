use super::*;

#[tokio::test]
async fn mapper_publishes_init_messages_on_startup() {
    // Start SM Mapper
    let (_ttd, mut mqtt) = spawn_mapper().await;

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "114"), ("c8y/s/us", "500")]).await;
}

#[tokio::test]
async fn mapper_converts_agent_twin_to_c8y_agent_fragment() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    let version = env!("CARGO_PKG_VERSION");

    // Simulate tedge-agent publishing the agent twin fragment
    publish(
        &mut mqtt,
        "te/device/main///twin/agent",
        json!({
            "name": "thin-edge.io",
            "url": "https://thin-edge.io",
            "version": version
        })
        .to_string(),
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/inventory/managedObjects/update/test-device",
            json!({
                "c8y_Agent" : {
                    "name": "thin-edge.io",
                    "url": "https://thin-edge.io",
                    "version": version
                }
            })
            .to_string()
            .as_str(),
        )],
    )
    .await;
}

#[tokio::test]
async fn child_device_registration_mapping() {
    let ttd = TempTedgeDir::new();
    let test_handle =
        spawn_c8y_mapper_actor_with_config(&ttd, test_mapper_config(&ttd), true).await;
    let mut avail = test_handle.avail.with_timeout(TEST_TIMEOUT_MS);
    let mut mqtt = test_handle.mqtt.with_timeout(TEST_TIMEOUT_MS);

    publish(
        &mut mqtt,
        "te/device/child1//",
        r#"{ "@type": "child-device", "type": "RaspberryPi", "name": "Child1" }"#,
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "101,test-device:device:child1,Child1,RaspberryPi,false",
        )],
    )
    .await;

    tedge_mqtt_ext::test_helpers::assert_received_contains_str(
        &mut avail,
        [(
            "te/device/child1//",
            r#"{"@id":"test-device:device:child1","@type":"child-device","name":"Child1","type":"RaspberryPi"}"#
        )],
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/child2//",
        r#"{ "@type": "child-device", "@parent": "device/child1//" }"#,
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child1",
            "101,test-device:device:child2,test-device:device:child2,thin-edge.io-child,false",
        )],
    )
    .await;

    tedge_mqtt_ext::test_helpers::assert_received_contains_str(
        &mut avail,
        [(
            "te/device/child2//",
            r#"{"@id":"test-device:device:child2","@parent":"device/child1//","@type":"child-device"}"#
        )],
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/child3//",
        r#"{ "@type": "child-device", "@id": "child3", "@parent": "device/child2//" }"#,
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child2",
            "101,child3,child3,thin-edge.io-child,false",
        )],
    )
    .await;

    tedge_mqtt_ext::test_helpers::assert_received_contains_str(
        &mut avail,
        [(
            "te/device/child3//",
            r#"{"@id":"child3","@parent":"device/child2//","@type":"child-device"}"#,
        )],
    )
    .await;
}

#[tokio::test]
async fn custom_topic_scheme_registration_mapping() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    // Child device with custom scheme
    publish(
        &mut mqtt,
        "te/custom///",
        r#"{ "@type": "child-device", "type": "RaspberryPi", "name": "Child1" }"#,
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "101,test-device:custom,Child1,RaspberryPi,false",
        )],
    )
    .await;

    publish(
        &mut mqtt,
        "te/custom/child1//",
        r#"{ "@type": "child-device", "type": "RaspberryPi", "name": "Child1" }"#,
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "101,test-device:custom:child1,Child1,RaspberryPi,false",
        )],
    )
    .await;

    // Service with custom scheme
    publish(
        &mut mqtt,
        "te/custom/service/collectd/",
        r#"{ "@type": "service", "type": "systemd", "name": "Collectd" }"#,
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "102,test-device:custom:service:collectd,systemd,Collectd,up",
        )],
    )
    .await;
}

#[tokio::test]
async fn service_registration_mapping() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    // Create a direct child device: child1 and a nested child device: child2
    publish(
        &mut mqtt,
        "te/device/child1//",
        r#"{ "@type": "child-device" }"#,
    )
    .await;
    publish(
        &mut mqtt,
        "te/device/child2//",
        r#"{ "@type": "child-device", "@parent": "device/child1//" }"#,
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/main/service/collectd",
        r#"{ "@type": "service", "type": "systemd", "name": "Collectd" }"#,
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us",
            "102,test-device:device:main:service:collectd,systemd,Collectd,up",
        )],
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/child1/service/collectd",
        r#"{ "@type": "service", "type": "systemd", "name": "Collectd" }"#,
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child1",
            "102,test-device:device:child1:service:collectd,systemd,Collectd,up",
        )],
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/child2/service/collectd",
        r#"{ "@type": "service", "type": "systemd", "name": "Collectd" }"#,
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:child2",
            "102,test-device:device:child2:service:collectd,systemd,Collectd,up",
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_publishes_supported_software_types() {
    let ttd = TempTedgeDir::new();
    let test_handle = spawn_c8y_mapper_actor(&ttd, true).await;
    let TestHandle { mqtt, http, .. } = test_handle;
    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // Simulate software_list capability message
    publish(
        &mut mqtt,
        "te/device/main///cmd/software_list",
        json!({"types": ["apt", "docker"]}).to_string(),
    )
    .await;

    assert_received_includes_json(
        &mut mqtt,
        [(
            "c8y/inventory/managedObjects/update/test-device",
            json!({"c8y_SupportedSoftwareTypes":["apt","docker"]}),
        )],
    )
    .await;
}
