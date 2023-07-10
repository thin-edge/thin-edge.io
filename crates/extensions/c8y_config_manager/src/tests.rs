use crate::actor::OperationTimeout;
use crate::actor::OperationTimer;
use crate::child_device::ChildDeviceRequestPayload;
use crate::child_device::ChildDeviceResponsePayload;
use crate::ConfigManagerBuilder;
use crate::ConfigManagerConfig;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::messages::C8YRestError;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResponse;
use c8y_http_proxy::messages::C8YRestResult;
use c8y_http_proxy::messages::DownloadFile;
use c8y_http_proxy::messages::UploadConfigFile;
use serde_json::json;
use std::net::Ipv4Addr;
use std::time::Duration;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::test_helpers::WithTimeout;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynError;
use tedge_actors::MessageReceiver;
use tedge_actors::NoMessage;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::OperationStatus;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_test_utils::fs::TempTedgeDir;
use tedge_timer_ext::Timeout;
use tedge_utils::file::PermissionEntry;

const TEST_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn test_config_plugin_init() -> Result<(), DynError> {
    let device_id = "tedge-device".to_string();
    let test_config_type = "test-config";
    let test_config_path = "/some/test/config";
    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let (mut mqtt_message_box, mut _c8y_proxy_message_box, mut _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    mqtt_message_box
        .assert_received([
            MqttMessage::new(
                &C8yTopic::SmartRestResponse.to_topic()?,
                format!("119,c8y-configuration-plugin,{test_config_type}"), // Supported config types
            ),
            MqttMessage::new(&C8yTopic::SmartRestResponse.to_topic().unwrap(), "500"), // Get pending operations
        ])
        .await;
    Ok(())
}

#[tokio::test]
async fn test_config_upload_tedge_device() -> Result<(), DynError> {
    let device_id = "tedge-device".to_string();
    let test_config_type = "test-config";
    let test_config_path = "/some/test/config";
    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let (mqtt_message_box, c8y_proxy_message_box, mut _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    let mut mqtt_message_box = mqtt_message_box.with_timeout(TEST_TIMEOUT);
    let mut c8y_proxy_message_box = c8y_proxy_message_box.with_timeout(TEST_TIMEOUT);

    // Skip the initial bootstrap messages
    mqtt_message_box.skip(2).await;

    let c8y_config_upload_msg = MqttMessage::new(
        &Topic::new_unchecked("c8y/s/ds"),
        format!("526,{device_id},{test_config_type}").as_str(),
    );
    mqtt_message_box.send(c8y_config_upload_msg).await?;

    // Assert EXECUTING SmartREST MQTT message
    mqtt_message_box
        .assert_received([MqttMessage::new(
            &C8yTopic::SmartRestResponse.to_topic()?,
            "501,c8y_UploadConfigFile\n",
        )])
        .await;

    // Assert config file upload HTTP request
    c8y_proxy_message_box
        .assert_received([UploadConfigFile {
            config_path: test_config_path.into(),
            config_type: test_config_type.to_string(),
            child_device_id: None,
        }])
        .await;

    // Provide mock config file upload HTTP response to continue
    c8y_proxy_message_box
        .send(Ok(C8YRestResponse::EventId("test-url".to_string())))
        .await?;

    // Assert SUCCESSFUL SmartREST MQTT message
    mqtt_message_box
        .assert_received([MqttMessage::new(
            &C8yTopic::SmartRestResponse.to_topic()?,
            "503,c8y_UploadConfigFile,test-url\n",
        )])
        .await;

    Ok(())
}

#[tokio::test]
async fn test_config_download_tedge_device() -> Result<(), DynError> {
    let device_id = "tedge-device".to_string();
    let test_config_type = "test-config";
    let test_config_path = "/some/test/config";
    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let (mqtt_message_box, c8y_proxy_message_box, mut _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    let mut mqtt_message_box = mqtt_message_box.with_timeout(TEST_TIMEOUT);
    let mut c8y_proxy_message_box = c8y_proxy_message_box.with_timeout(TEST_TIMEOUT);

    // Skip the initial bootstrap messages
    mqtt_message_box.skip(2).await;

    let download_url = "http://test.domain.com";
    mqtt_message_box
        .send(MqttMessage::new(
            &C8yTopic::SmartRestRequest.to_topic().unwrap(),
            format!("524,{device_id},{download_url},{test_config_type}"),
        ))
        .await?;

    // Assert EXECUTING SmartREST MQTT message
    mqtt_message_box
        .assert_received([MqttMessage::new(
            &C8yTopic::SmartRestResponse.to_topic()?,
            "501,c8y_DownloadConfigFile\n",
        )])
        .await;

    // Assert config file upload HTTP request
    c8y_proxy_message_box
        .assert_received([DownloadFile {
            download_url: download_url.into(),
            file_path: test_config_path.into(),
            file_permissions: PermissionEntry::default(),
        }])
        .await;

    // Provide mock config file download HTTP response to continue
    c8y_proxy_message_box
        .send(Ok(C8YRestResponse::Unit(())))
        .await?;

    // Assert SUCCESSFUL SmartREST MQTT message
    mqtt_message_box
        .assert_received([MqttMessage::new(
            &C8yTopic::SmartRestResponse.to_topic()?,
            "503,c8y_DownloadConfigFile,\n",
        )])
        .await;

    Ok(())
}

#[tokio::test]
async fn test_child_device_config_upload_request_mapping() -> Result<(), DynError> {
    let device_id = "tedge-device".to_string();
    let child_device_id = "child-aa";
    let test_config_type = "file_a";
    let test_config_path = "/some/test/config";

    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let (mqtt_message_box, _c8y_proxy_message_box, _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    let mut mqtt_message_box = mqtt_message_box.with_timeout(TEST_TIMEOUT);

    // Skip the initial bootstrap messages
    mqtt_message_box.skip(2).await;

    mqtt_message_box
        .send(MqttMessage::new(
            &Topic::new_unchecked("c8y/s/ds"),
            format!("526,{child_device_id},{test_config_type}").as_str(),
        ))
        .await?;

    let expected_payload = ChildDeviceRequestPayload {
        url: format!(
            "http://127.0.0.1:9876/tedge/file-transfer/{child_device_id}/config_snapshot/{test_config_type}"
        ),
        path: test_config_path.into(),
        config_type: Some(test_config_type.into()),
    };

    mqtt_message_box
        .assert_received([MqttMessage::new(
            &Topic::new_unchecked(&format!(
                "tedge/{child_device_id}/commands/req/config_snapshot"
            )),
            serde_json::to_string(&expected_payload)?,
        )])
        .await;

    Ok(())
}

#[tokio::test]
async fn test_child_device_config_upload_executing_response_mapping() -> Result<(), DynError> {
    let device_id = "tedge-device".to_string();
    let child_device_id = "child-aa";
    let test_config_type = "file_a";
    let test_config_path = "/some/test/config";

    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let (mqtt_message_box, _c8y_proxy_message_box, _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    let mut mqtt_message_box = mqtt_message_box.with_timeout(TEST_TIMEOUT);

    // Skip the initial bootstrap messages
    mqtt_message_box.skip(2).await;

    let c8y_config_upload_msg = MqttMessage::new(
        &Topic::new_unchecked(&format!(
            "tedge/{child_device_id}/commands/res/config_snapshot"
        )),
        serde_json::to_string(&ChildDeviceResponsePayload {
            status: Some(OperationStatus::Executing),
            path: test_config_path.into(),
            config_type: test_config_type.into(),
            reason: None,
        })?,
    );
    mqtt_message_box.send(c8y_config_upload_msg).await?;

    mqtt_message_box
        .with_timeout(TEST_TIMEOUT)
        .assert_received([MqttMessage::new(
            &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
            "501,c8y_UploadConfigFile\n",
        )])
        .await;

    Ok(())
}

#[tokio::test]
async fn test_child_device_config_upload_failed_response_mapping() -> Result<(), DynError> {
    let device_id = "tedge-device".to_string();
    let child_device_id = "child-aa";
    let test_config_type = "file_a";
    let test_config_path = "/some/test/config";

    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let (mqtt_message_box, _c8y_proxy_message_box, _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    let mut mqtt_message_box = mqtt_message_box.with_timeout(TEST_TIMEOUT);

    // Skip the initial bootstrap messages
    mqtt_message_box.skip(2).await;

    let c8y_config_upload_msg = MqttMessage::new(
        &Topic::new_unchecked(&format!(
            "tedge/{child_device_id}/commands/res/config_snapshot"
        )),
        json!({
            "status": "failed",
            "path": test_config_path,
            "type": test_config_type,
            "reason": "upload failed"
        })
        .to_string(),
    );
    mqtt_message_box.send(c8y_config_upload_msg).await?;

    mqtt_message_box
        .with_timeout(TEST_TIMEOUT)
        .assert_received([
            MqttMessage::new(
                &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
                "501,c8y_UploadConfigFile\n",
            ),
            MqttMessage::new(
                &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
                "502,c8y_UploadConfigFile,\"upload failed\"\n",
            ),
        ])
        .await;

    Ok(())
}

// Test invalid config_snapshot response from child is mapped to
// back-to-back EXECUTING and FAILED messages for c8y_UploadConfigFile operation
#[tokio::test]
async fn test_invalid_config_snapshot_response_child_device() -> Result<(), DynError> {
    let device_id = "tedge-device".to_string();
    let child_device_id = "child-aa";
    let test_config_type = "file_a";
    let test_config_path = "/some/test/config";

    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let (mqtt_message_box, _c8y_proxy_message_box, _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    let mut mqtt_message_box = mqtt_message_box.with_timeout(TEST_TIMEOUT);

    // Skip the initial bootstrap messages
    mqtt_message_box.skip(2).await;

    let c8y_config_upload_msg = MqttMessage::new(
        &Topic::new_unchecked(&format!(
            "tedge/{child_device_id}/commands/res/config_snapshot"
        )),
        "invalid json",
    );
    mqtt_message_box.send(c8y_config_upload_msg).await?;

    mqtt_message_box
        .assert_received(
        [
            MqttMessage::new(
                &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
                "501,c8y_UploadConfigFile\n",
            ),
            MqttMessage::new(
                &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
                "502,c8y_UploadConfigFile,\"Failed to parse response from child device with: expected value at line 1 column 1\"\n",
            ),
        ],
    )
    .await;

    Ok(())
}

// No response from the child for a config_snapshot request results in a timeout
// with back-to-back EXECUTING and FAILED messages for c8y_UploadConfigFile operation
#[tokio::test]
async fn test_timeout_on_no_config_snapshot_response_child_device() -> Result<(), DynError> {
    let device_id = "tedge-device".to_string();
    let child_device_id = "child-aa";
    let test_config_type = "file_a";
    let test_config_path = "/some/test/config";

    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let (mqtt_message_box, _c8y_proxy_message_box, mut timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    let mut mqtt_message_box = mqtt_message_box.with_timeout(TEST_TIMEOUT);

    // Skip the initial bootstrap messages
    mqtt_message_box.skip(2).await;

    let c8y_config_upload_msg = MqttMessage::new(
        &Topic::new_unchecked("c8y/s/ds"),
        format!("526,{child_device_id},{test_config_type}").as_str(),
    );
    mqtt_message_box.send(c8y_config_upload_msg).await?;

    // Skip mapped tedge/config_snapshot request
    mqtt_message_box.skip(1).await;

    // Assert the that a SetTimeout request is sent to the TimerActor
    let set_timeout_msg = timer_message_box
        .recv()
        .with_timeout(TEST_TIMEOUT)
        .await?
        .expect("Start timeout message");

    // Send mocked Timeout response simulating a timeout from TimerActor
    timer_message_box
        .send(Timeout::new(set_timeout_msg.event))
        .await?;

    mqtt_message_box
        .assert_received([
            MqttMessage::new(
                &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
                "501,c8y_UploadConfigFile\n",
            ),
            MqttMessage::new(
                &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
                "502,c8y_UploadConfigFile,\"Timeout due to lack of response from child device: child-aa for config type: file_a\"\n",
            ),
        ],
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn test_child_device_successful_config_snapshot_response_mapping() -> Result<(), DynError> {
    let device_id = "tedge-device".to_string();
    let child_device_id = "child-aa";
    let test_config_type = "file_a";
    let test_config_path = "/some/test/config";

    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let (mut mqtt_message_box, mut c8y_proxy_message_box, _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    // Skip the initial bootstrap messages
    for _ in 0..2 {
        let _ = tokio::time::timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await?;
    }

    let c8y_config_upload_msg = MqttMessage::new(
        &Topic::new_unchecked(&format!(
            "tedge/{child_device_id}/commands/res/config_snapshot"
        )),
        json!({
            "status": "successful",
            "path": test_config_path,
            "type": test_config_type,
        })
        .to_string(),
    );
    mqtt_message_box.send(c8y_config_upload_msg).await?;

    c8y_proxy_message_box
        .assert_received([UploadConfigFile {
            config_path: ttd
                .to_path_buf()
                .join("file-transfer")
                .join(child_device_id)
                .join("config_snapshot")
                .join(test_config_type),
            config_type: test_config_type.into(),
            child_device_id: Some(child_device_id.into()),
        }])
        .await;

    // Provide mock config file upload HTTP response to continue
    c8y_proxy_message_box
        .send(Ok(C8YRestResponse::EventId("test-url".to_string())))
        .await?;

    mqtt_message_box
        .with_timeout(TEST_TIMEOUT)
        .assert_received([
            MqttMessage::new(
                &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
                "501,c8y_UploadConfigFile\n",
            ),
            MqttMessage::new(
                &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
                "503,c8y_UploadConfigFile,test-url\n",
            ),
        ])
        .await;

    Ok(())
}

#[tokio::test]
async fn test_child_config_snapshot_successful_response_without_uploaded_file_mapped_failed(
) -> Result<(), DynError> {
    let device_id = "tedge-device".to_string();
    let child_device_id = "child-aa";
    let test_config_type = "file_a";
    let test_config_path = "/some/test/config";

    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let (mut mqtt_message_box, mut c8y_proxy_message_box, _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    // Skip the initial bootstrap messages
    for _ in 0..2 {
        let _ = tokio::time::timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await?;
    }

    let c8y_config_upload_msg = MqttMessage::new(
        &Topic::new_unchecked(&format!(
            "tedge/{child_device_id}/commands/res/config_snapshot"
        )),
        json!({
            "status": "successful",
            "path": test_config_path,
            "type": test_config_type,
        })
        .to_string(),
    );
    mqtt_message_box.send(c8y_config_upload_msg).await?;

    c8y_proxy_message_box
        .assert_received([UploadConfigFile {
            config_path: ttd
                .to_path_buf()
                .join("file-transfer")
                .join(child_device_id)
                .join("config_snapshot")
                .join(test_config_type),
            config_type: test_config_type.into(),
            child_device_id: Some(child_device_id.into()),
        }])
        .await;

    // Provide mock config file upload HTTP response to continue
    c8y_proxy_message_box
        .send(Err(C8YRestError::CustomError("file not found".into())))
        .await?;

    mqtt_message_box
        .with_timeout(TEST_TIMEOUT)
        .assert_received([
            MqttMessage::new(
                &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
                "501,c8y_UploadConfigFile\n",
            ),
            MqttMessage::new(
                &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
                "502,c8y_UploadConfigFile,\"Failed with file not found\"\n",
            ),
        ])
        .await;

    Ok(())
}

#[tokio::test]
async fn test_child_device_config_download_request_mapping() -> Result<(), DynError> {
    let device_id = "tedge-device".to_string();
    let child_device_id = "child-aa";
    let test_config_type = "file_a";
    let test_config_path = "/some/test/config";

    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let (mut mqtt_message_box, mut c8y_proxy_message_box, _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    // Skip the initial bootstrap messages
    for _ in 0..2 {
        let _ = tokio::time::timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await?;
    }

    let download_url = "http://test.domain.com";
    let c8y_config_download_msg = MqttMessage::new(
        &C8yTopic::SmartRestRequest.to_topic().unwrap(),
        format!("524,{child_device_id},{download_url},{test_config_type}"),
    );
    mqtt_message_box.send(c8y_config_download_msg).await?;

    // Assert download request sent to c8y-proxy
    c8y_proxy_message_box
        .assert_received([DownloadFile {
            download_url: download_url.into(),
            file_path: ttd
                .to_path_buf()
                .join("file-transfer")
                .join(child_device_id)
                .join("config_update")
                .join(test_config_type),
            file_permissions: PermissionEntry::default(),
        }])
        .await;

    // Provide mock download response to continue
    c8y_proxy_message_box.send(Ok(().into())).await?;

    let expected_payload = ChildDeviceRequestPayload {
        url: format!(
            "http://127.0.0.1:9876/tedge/file-transfer/{child_device_id}/config_update/{test_config_type}"
        ),
        path: test_config_path.into(),
        config_type: Some(test_config_type.into()),
    };
    mqtt_message_box
        .with_timeout(TEST_TIMEOUT)
        .assert_received([MqttMessage::new(
            &Topic::new_unchecked(&format!(
                "tedge/{child_device_id}/commands/req/config_update"
            )),
            serde_json::to_string(&expected_payload)?,
        )])
        .await;

    Ok(())
}

#[tokio::test]
async fn test_child_device_config_update_executing_response_mapping() -> Result<(), DynError> {
    let device_id = "tedge-device".to_string();
    let child_device_id = "child-aa";
    let test_config_type = "file_a";
    let test_config_path = "/some/test/config";

    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let (mut mqtt_message_box, _c8y_proxy_message_box, _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    // Skip the initial bootstrap messages
    for _ in 0..2 {
        let _ = tokio::time::timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await?;
    }

    let c8y_config_update_msg = MqttMessage::new(
        &Topic::new_unchecked(&format!(
            "tedge/{child_device_id}/commands/res/config_update"
        )),
        json!({
            "status": "executing",
            "path": test_config_path,
            "type": test_config_type,
        })
        .to_string(),
    );
    mqtt_message_box.send(c8y_config_update_msg).await?;

    mqtt_message_box
        .with_timeout(TEST_TIMEOUT)
        .assert_received([MqttMessage::new(
            &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
            "501,c8y_DownloadConfigFile\n",
        )])
        .await;

    Ok(())
}

#[tokio::test]
async fn test_child_device_config_update_successful_response_mapping() -> Result<(), DynError> {
    let device_id = "tedge-device".to_string();
    let child_device_id = "child-aa";
    let test_config_type = "file_a";
    let test_config_path = "/some/test/config";

    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let (mut mqtt_message_box, _c8y_proxy_message_box, _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    // Skip the initial bootstrap messages
    for _ in 0..2 {
        let _ = tokio::time::timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await?;
    }

    let c8y_config_update_msg = MqttMessage::new(
        &Topic::new_unchecked(&format!(
            "tedge/{child_device_id}/commands/res/config_update"
        )),
        json!({
            "status": "successful",
            "path": test_config_path,
            "type": test_config_type,
        })
        .to_string(),
    );
    mqtt_message_box.send(c8y_config_update_msg).await?;

    mqtt_message_box
        .with_timeout(TEST_TIMEOUT)
        .assert_received([
            MqttMessage::new(
                &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
                "501,c8y_DownloadConfigFile\n",
            ),
            MqttMessage::new(
                &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
                "503,c8y_DownloadConfigFile,\n",
            ),
        ])
        .await;

    Ok(())
}

#[tokio::test]
async fn test_child_device_config_update_failed_response_mapping() -> Result<(), DynError> {
    let device_id = "tedge-device".to_string();
    let child_device_id = "child-aa";
    let test_config_type = "file_a";
    let test_config_path = "/some/test/config";

    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let (mut mqtt_message_box, _c8y_proxy_message_box, _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    // Skip the initial bootstrap messages
    for _ in 0..2 {
        let _ = tokio::time::timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await?;
    }

    let c8y_config_update_msg = MqttMessage::new(
        &Topic::new_unchecked(&format!(
            "tedge/{child_device_id}/commands/res/config_update"
        )),
        json!({
            "status": "failed",
            "path": test_config_path,
            "type": test_config_type,
            "reason": "download failed"
        })
        .to_string(),
    );
    mqtt_message_box.send(c8y_config_update_msg).await?;

    mqtt_message_box
        .with_timeout(TEST_TIMEOUT)
        .assert_received([
            MqttMessage::new(
                &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
                "501,c8y_DownloadConfigFile\n",
            ),
            MqttMessage::new(
                &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
                "502,c8y_DownloadConfigFile,\"download failed\"\n",
            ),
        ])
        .await;

    Ok(())
}

#[tokio::test]
async fn test_child_device_config_download_fail_with_broken_url() -> Result<(), DynError> {
    let device_id = "tedge-device".to_string();
    let child_device_id = "child-aa";
    let test_config_type = "file_a";
    let test_config_path = "/some/test/config";

    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let (mut mqtt_message_box, mut c8y_proxy_message_box, _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    // Skip the initial bootstrap messages
    for _ in 0..2 {
        let _ = tokio::time::timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await?;
    }

    let download_url = "bad-url";
    let c8y_config_download_msg = MqttMessage::new(
        &C8yTopic::SmartRestRequest.to_topic().unwrap(),
        format!("524,{child_device_id},{download_url},{test_config_type}"),
    );
    mqtt_message_box.send(c8y_config_download_msg).await?;

    // Assert download request sent to c8y-proxy
    c8y_proxy_message_box
        .assert_received([DownloadFile {
            download_url: download_url.into(),
            file_path: ttd
                .to_path_buf()
                .join("file-transfer")
                .join(child_device_id)
                .join("config_update")
                .join(test_config_type),
            file_permissions: PermissionEntry::default(),
        }])
        .await;

    // Provide mock download response to continue
    c8y_proxy_message_box
        .send(Err(C8YRestError::CustomError("file not found".into())))
        .await?;

    mqtt_message_box
        .with_timeout(TEST_TIMEOUT)
        .assert_received([MqttMessage::new(
                &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
                "501,c8y_DownloadConfigFile\n",
            ),
            MqttMessage::new(
                &C8yTopic::ChildSmartRestResponse(child_device_id.into()).to_topic()?,
                "502,c8y_DownloadConfigFile,\"Downloading the config file update from bad-url failed with Failed with file not found\"\n",
            ),
        ],
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn test_multiline_smartrest_requests() -> Result<(), DynError> {
    let device_id = "tedge-device".to_string();
    let test_config_type = "test-config";
    let test_config_path = "/some/test/config";
    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let (mqtt_message_box, mut c8y_proxy_message_box, mut _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    let mut mqtt_message_box = mqtt_message_box.with_timeout(TEST_TIMEOUT);

    // Task to send dummy C8yHttpResponse
    tokio::spawn(async move {
        loop {
            if let Some(_req) = c8y_proxy_message_box.recv().await {
                c8y_proxy_message_box
                    .send(Ok(C8YRestResponse::EventId("test-url".to_string())))
                    .await
                    .unwrap();
            }
        }
    });

    // Skip the initial bootstrap messages
    mqtt_message_box.skip(2).await;

    mqtt_message_box
        .send(MqttMessage::new(
            &Topic::new_unchecked("c8y/s/ds"),
            format!("526,{device_id},{test_config_type}").as_str(),
        ))
        .await?;

    // Assert EXECUTING and SUCCESSFUL SmartREST MQTT message
    mqtt_message_box
        .assert_received([
            MqttMessage::new(
                &C8yTopic::SmartRestResponse.to_topic()?,
                "501,c8y_UploadConfigFile\n",
            ),
            MqttMessage::new(
                &C8yTopic::SmartRestResponse.to_topic()?,
                "503,c8y_UploadConfigFile,test-url\n",
            ),
        ])
        .await;

    Ok(())
}

async fn spawn_config_manager(
    device_id: &str,
    tedge_temp_dir: &TempTedgeDir,
) -> Result<
    (
        SimpleMessageBox<MqttMessage, MqttMessage>,
        SimpleMessageBox<C8YRestRequest, C8YRestResult>,
        SimpleMessageBox<OperationTimer, OperationTimeout>,
    ),
    DynError,
> {
    let tedge_host = "127.0.0.1";
    let mqtt_port = 1234;
    let tedge_http_port = 9876;

    let config = ConfigManagerConfig::new(
        tedge_temp_dir.to_path_buf(),
        tedge_temp_dir.to_path_buf(),
        tedge_temp_dir.to_path_buf(),
        device_id.to_string(),
        tedge_host.to_string(),
        mqtt_port,
        tedge_host.parse::<Ipv4Addr>().unwrap().into(),
        tedge_http_port,
    );

    let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 5);
    let mut c8y_proxy_builder: SimpleMessageBoxBuilder<C8YRestRequest, C8YRestResult> =
        SimpleMessageBoxBuilder::new("C8Y", 1);
    let mut timer_builder: SimpleMessageBoxBuilder<OperationTimer, OperationTimeout> =
        SimpleMessageBoxBuilder::new("Timer", 5);
    let mut fs_builder: SimpleMessageBoxBuilder<NoMessage, FsWatchEvent> =
        SimpleMessageBoxBuilder::new("FsNotify", 5);

    let config_manager_builder = ConfigManagerBuilder::try_new(
        config,
        &mut mqtt_builder,
        &mut c8y_proxy_builder,
        &mut timer_builder,
        &mut fs_builder,
    )?;

    let mqtt_message_box = mqtt_builder.build();
    let c8y_proxy_message_box = c8y_proxy_builder.build();
    let timer_message_box = timer_builder.build();

    let mut actor = config_manager_builder.build();
    let _join_handle = tokio::spawn(async move { actor.run().await });

    Ok((mqtt_message_box, c8y_proxy_message_box, timer_message_box))
}
