use super::*;

#[tokio::test]
async fn mapper_handles_multiple_modules_in_update_list_sm_requests() {
    // The test assures if Mapper can handle multiple update modules received via JSON over MQTT
    let ttd = TempTedgeDir::new();
    let config = test_mapper_config(&ttd);
    let (_mapper, mut mqtt) = spawn_mapper_with_config(ttd, config).await;

    // Publish multiple modules software update via JSON over MQTT.
    mqtt.send(MqttMessage::new(
        &C8yDeviceControlTopic::topic(&"c8y".try_into().unwrap()),
        json!({
            "id": "123456",
            "c8y_SoftwareUpdate": [
                {
                    "name": "nodered",
                    "action": "install",
                    "version": "1.0.0::debian",
                    "url": ""
                },
                {
                    "name": "rolldice",
                    "action": "install",
                    "version": "2.0.0::debian",
                    "url": ""
                }
            ],
            "externalSource": {
                "externalId": "test-device",
                "type": "c8y_Serial"
            }
        })
        .to_string(),
    ))
    .await
    .expect("Send failed");

    assert_received_includes_json(
        &mut mqtt,
        [(
            "te/device/main///cmd/software_update/c8y-mapper-123456",
            json!({
                "status": "init",
                "updateList": [
                    {
                        "type": "debian",
                        "modules": [
                            {
                                "name": "nodered",
                                "version": "1.0.0",
                                "action": "install"
                            },
                            {
                                "name": "rolldice",
                                "version": "2.0.0",
                                "action": "install"
                            }
                        ]
                    }
                ]
            }),
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_processes_operations_concurrently() {
    let num_operations = 20;

    let mut fts_server = mockito::Server::new_async().await;
    let _mock = fts_server
        .mock(
            "GET",
            "/te/v1/files/test-device/config_snapshot/c8y-mapper-1234",
        )
        // make each download block so it doesn't complete before we submit all operations
        .with_chunked_body(|_w| {
            std::thread::sleep(Duration::from_secs(5));
            Ok(())
        })
        .expect(num_operations)
        .create_async()
        .await;
    let host_port = fts_server.host_with_port();

    let cfg_dir = TempTedgeDir::new();
    let TestHandle { mqtt, http, dl, .. } = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    let mut dl = dl.with_timeout(TEST_TIMEOUT_MS);

    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // simulate many successful operations that needs to be handled by the mapper
    for i in 0..num_operations {
        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked(&format!("te/device/main///cmd/log_upload/c8y-mapper-{i}")),
            json!({
            "status": "successful",
            "tedgeUrl": format!("http://{host_port}/te/v1/files/test-device/log_upload/c8y-mapper-1234"),
            "type": "mosquitto",
            "dateFrom": "2023-11-28T16:33:50+01:00",
            "dateTo": "2023-11-29T16:33:50+01:00",
            "searchText": "ERROR",
            "lines": 1000

        })
                .to_string(),
        ))
            .await.unwrap();

        mqtt.send(MqttMessage::new(
            &Topic::new_unchecked(&format!("te/device/main///cmd/config_snapshot/c8y-mapper-{i}")),
            json!({
            "status": "successful",
            "tedgeUrl": format!("http://{host_port}/te/v1/files/test-device/config_snapshot/c8y-mapper-1234"),
            "type": "path/type/A",
        })
                .to_string(),
        ))
            .await.unwrap();
    }

    for _ in 0..(num_operations * 2) {
        dl.recv()
            .await
            .expect("there should be one DownloadRequest per operation");
    }
}

#[tokio::test]
async fn mapper_processes_other_operations_while_uploads_and_downloads_are_ongoing() {
    let cfg_dir = TempTedgeDir::new();
    let TestHandle {
        mqtt, http, dl, ul, ..
    } = spawn_c8y_mapper_actor(&cfg_dir, true).await;
    let mut dl = dl.with_timeout(TEST_TIMEOUT_MS);
    let mut ul = ul.with_timeout(TEST_TIMEOUT_MS);

    spawn_dummy_c8y_http_proxy(http);

    let mut mqtt = mqtt.with_timeout(TEST_TIMEOUT_MS);

    // simulate many successful operations that needs to be handled by the mapper
    publish(
        &mut mqtt,
        "te/device/main///cmd/log_upload/c8y-mapper-1",
        json!({
            "status": "successful",
            "tedgeUrl": "http://localhost:8888/te/v1/files/test-device/log_upload/c8y-mapper-1",
            "type": "mosquitto",
            "dateFrom": "2023-11-28T16:33:50+01:00",
            "dateTo": "2023-11-29T16:33:50+01:00",
            "searchText": "ERROR",
            "lines": 1000
        })
        .to_string(),
    )
    .await;

    let (download_id, download_request) = dl
        .recv()
        .await
        .expect("DownloadRequest for log_upload should be sent");
    assert_eq!(download_id, "c8y-mapper-1");
    assert_eq!(
        download_request.url,
        "http://localhost:8888/te/v1/files/test-device/log_upload/c8y-mapper-1"
    );

    // here it would be good to assert that upload message hasn't been sent yet, but due to the
    // behaviour of message channels it can't be easily done

    dl.send((
        "c8y-mapper-1".to_string(),
        Ok(DownloadResponse {
            url: "http://localhost:8888/te/v1/files/test-device/log_upload/c8y-mapper-1"
                .to_string(),
            file_path: "whatever".into(),
        }),
    ))
    .await
    .unwrap();

    let (upload_id, _) = ul
        .recv()
        .await
        .expect("UploadRequest for log_upload should be sent");
    assert_eq!(upload_id, "c8y-mapper-1");

    // now that an upload is ongoing, check that downloads can also be triggered
    publish(&mut mqtt, "te/device/main///cmd/config_snapshot/c8y-mapper-2", json!({
            "status": "successful",
            "tedgeUrl": "http://localhost:8888/te/v1/files/test-device/config_snapshot/c8y-mapper-2",
            "type": "typeA",
        })
                .to_string()).await;

    let (config_snapshot_id, _) = dl
        .recv()
        .await
        .expect("DownloadRequest for config snapshot should be sent");
    assert_eq!(config_snapshot_id, "c8y-mapper-2");

    // while download and upload are ongoing, try some other operation that doesn't do download or upload
    publish(
        &mut mqtt,
        "te/device/main///cmd/restart/c8y-mapper-3",
        json!({
            "status": "successful",
        })
        .to_string(),
    )
    .await;

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "503,c8y_Restart")]).await;
}

#[tokio::test]
async fn the_restart_command_of_a_service_is_reported_as_c8y_restart() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    publish(
        &mut mqtt,
        "te/device/main/service/collectd",
        r#"{ "@type": "service" }"#,
    )
    .await;

    publish(
        &mut mqtt,
        "te/device/main/service/collectd/cmd/restart/c8y-mapper-1234",
        json!({"status": "successful"}).to_string(),
    )
    .await;

    assert_received_contains_str(
        &mut mqtt,
        [(
            "c8y/s/us/test-device:device:main:service:collectd",
            "503,c8y_Restart",
        )],
    )
    .await;
}

#[tokio::test]
async fn mapper_doesnt_update_status_of_subworkflow_commands_3048() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    // should hold for any operation type
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked(
            "te/device/rpizero2-d83add42f121///cmd/restart/sub:firmware_update:c8y-mapper-192481",
        ),
        r#"{"logPath":"/var/log/tedge/agent/workflow-firmware_update-c8y-mapper-192481.log","status":"executing"}"#,
    )).await.unwrap();

    while let Some(msg) = dbg!(mqtt.recv().await) {
        assert_ne!(msg.payload_str().unwrap(), "501,c8y_Restart");
    }
}

#[tokio::test]
async fn mapper_doesnt_send_duplicate_operation_status() {
    let (_ttd, mut mqtt) = spawn_mapper().await;

    for _ in 0..3 {
        mqtt.send(
            MqttMessage::new(
                &Topic::new_unchecked("te/device/main///cmd/config_snapshot/c8y-mapper-1"),
                r#"{"status":"executing", "type": "typeA"}"#,
            )
            .with_retain(),
        )
        .await
        .unwrap();
    }

    assert_received_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_UploadConfigFile")]).await;

    // there's no duplicate after the 1st message was removed by the assert above
    assert_received_not_contains_str(&mut mqtt, [("c8y/s/us", "501,c8y_UploadConfigFile")]).await;
}
