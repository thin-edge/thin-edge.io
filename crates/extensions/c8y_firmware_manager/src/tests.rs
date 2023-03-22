use super::*;

use assert_json_diff::assert_json_include;
use mqtt_channel::Topic;
use serde_json::json;
use sha256::digest;
use std::str::from_utf8;
use std::time::Duration;
use tedge_actors::test_helpers;
use tedge_actors::Actor;
use tedge_actors::DynError;
use tedge_actors::ReceiveMessages;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_config::IpAddress;
use tedge_test_utils::fs::TempTedgeDir;
use tokio::time::timeout;

const CHILD_DEVICE_ID: &str = "child-device";
const FIRMWARE_NAME: &str = "fw-name";
const FIRMWARE_VERSION: &str = "fw-version";
const TEDGE_HOST: &str = "127.0.0.1";
const TEDGE_HTTP_PORT: u16 = 8765;
const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);
const DEFAULT_REQUEST_TIMEOUT_SEC: Duration = Duration::from_secs(3600);
const C8Y_CHILD_PUBLISH_TOPIC_NAME: &str = "c8y/s/us/child-device";

// TODO: We don't need mockito???
const DOWNLOAD_URL: &str = "http://test.domain.com";

type MqttMessageBox = SimpleMessageBox<MqttMessage, MqttMessage>;

#[tokio::test]
async fn handle_request_child_device() -> Result<(), DynError> {
    let mut ttd = TempTedgeDir::new();
    let (mut mqtt_message_box, mut _c8y_proxy_message_box, mut _timer_message_box) =
        spawn_firmware_manager(&mut ttd, DEFAULT_REQUEST_TIMEOUT_SEC, true).await?;

    // On startup, SmartREST 500 should be sent by firmware manager.
    let _pending_ops_msg = mqtt_message_box.recv().await;

    // Publish firmware update operation to child device.
    publish_smartrest_firmware_operation(&mut mqtt_message_box).await?;

    // The first MQTT message after the c8y operation published should be firmware update request.
    let (topic_name, received_json) = timeout(TEST_TIMEOUT_MS, mqtt_message_box.recv())
        .await?
        .map(|msg| {
            (
                msg.topic.name,
                serde_json::from_str::<serde_json::Value>(from_utf8(&msg.payload).expect("UTF8"))
                    .expect("JSON"),
            )
        })
        .expect("timeout");

    assert_eq!(
        topic_name,
        format!("tedge/{CHILD_DEVICE_ID}/commands/req/firmware_update")
    );

    let file_cache_key = digest(DOWNLOAD_URL);
    let expected_request_payload = json!({
        "attempt": 1,
        "name": FIRMWARE_NAME,
        "version": FIRMWARE_VERSION,
        "url": format!("http://{TEDGE_HOST}:{TEDGE_HTTP_PORT}/tedge/file-transfer/{CHILD_DEVICE_ID}/firmware_update/{file_cache_key}")
    });
    assert_json_include!(actual: received_json, expected: expected_request_payload);

    Ok(())
}

#[tokio::test]
async fn handle_request_dir_cache_not_found() -> Result<(), DynError> {
    let mut ttd = TempTedgeDir::new();
    ttd.dir("file-transfer");
    ttd.dir("firmware");

    let (mut mqtt_message_box, mut _c8y_proxy_message_box, mut _timer_message_box) =
        spawn_firmware_manager(&mut ttd, DEFAULT_REQUEST_TIMEOUT_SEC, false).await?;

    // On startup, SmartREST 500 should be sent by firmware manager.
    let _pending_ops_msg = mqtt_message_box.recv().await;

    // Publish firmware update operation to child device.
    publish_smartrest_firmware_operation(&mut mqtt_message_box).await?;

    // Assert EXECUTING SmartREST MQTT message and FAILED SmartREST MQTT message due to missing 'cache' directory.
    test_helpers::assert_received(
        &mut mqtt_message_box,
        TEST_TIMEOUT_MS,
        [
            MqttMessage::new(&Topic::new_unchecked(C8Y_CHILD_PUBLISH_TOPIC_NAME), "501,c8y_Firmware\n"),
            MqttMessage::new(&Topic::new_unchecked(C8Y_CHILD_PUBLISH_TOPIC_NAME), format!(
                "502,c8y_Firmware,\"Directory {}/cache is not found. Run 'c8y-firmware-plugin --init' to create the directory.\"\n",
                ttd.path().display()
            )),
        ],
    ).await;

    Ok(())
}

#[tokio::test]
async fn handle_request_dir_firmware_not_found() -> Result<(), DynError> {
    let mut ttd = TempTedgeDir::new();
    ttd.dir("file-transfer");
    ttd.dir("cache");

    let (mut mqtt_message_box, mut _c8y_proxy_message_box, mut _timer_message_box) =
        spawn_firmware_manager(&mut ttd, DEFAULT_REQUEST_TIMEOUT_SEC, false).await?;

    // On startup, SmartREST 500 should be sent by firmware manager.
    let _pending_ops_msg = mqtt_message_box.recv().await;

    // Publish firmware update operation to child device.
    publish_smartrest_firmware_operation(&mut mqtt_message_box).await?;

    // Assert EXECUTING SmartREST MQTT message and FAILED SmartREST MQTT message due to missing 'firmware' directory.
    test_helpers::assert_received(
        &mut mqtt_message_box,
        TEST_TIMEOUT_MS,
        [
            MqttMessage::new(&Topic::new_unchecked(C8Y_CHILD_PUBLISH_TOPIC_NAME), "501,c8y_Firmware\n"),
            MqttMessage::new(&Topic::new_unchecked(C8Y_CHILD_PUBLISH_TOPIC_NAME), format!(
                "502,c8y_Firmware,\"Directory {}/firmware is not found. Run 'c8y-firmware-plugin --init' to create the directory.\"\n",
                ttd.path().display()
            )),
        ],
    ).await;

    Ok(())
}

#[tokio::test]
async fn handle_request_dir_file_transfer_not_found() -> Result<(), DynError> {
    let mut ttd = TempTedgeDir::new();
    ttd.dir("cache");
    ttd.dir("firmware");

    let (mut mqtt_message_box, mut _c8y_proxy_message_box, mut _timer_message_box) =
        spawn_firmware_manager(&mut ttd, DEFAULT_REQUEST_TIMEOUT_SEC, false).await?;

    // On startup, SmartREST 500 should be sent by firmware manager.
    let _pending_ops_msg = mqtt_message_box.recv().await;

    // Publish firmware update operation to child device.
    publish_smartrest_firmware_operation(&mut mqtt_message_box).await?;

    // Assert EXECUTING SmartREST MQTT message and FAILED SmartREST MQTT message due to missing 'file-transfer' directory.
    test_helpers::assert_received(
        &mut mqtt_message_box,
        TEST_TIMEOUT_MS,
        [
            MqttMessage::new(&Topic::new_unchecked(C8Y_CHILD_PUBLISH_TOPIC_NAME), "501,c8y_Firmware\n"),
            MqttMessage::new(&Topic::new_unchecked(C8Y_CHILD_PUBLISH_TOPIC_NAME), format!(
                "502,c8y_Firmware,\"Directory {}/file-transfer is not found. Run 'c8y-firmware-plugin --init' to create the directory.\"\n",
                ttd.path().display()
            )),
        ],
    ).await;

    Ok(())
}

#[tokio::test]
async fn handle_response_successful_child_device() -> Result<(), DynError> {
    let mut ttd = TempTedgeDir::new();
    let (mut mqtt_message_box, mut _c8y_proxy_message_box, mut _timer_message_box) =
        spawn_firmware_manager(&mut ttd, DEFAULT_REQUEST_TIMEOUT_SEC, true).await?;

    // On startup, SmartREST 500 should be sent by firmware manager.
    let _pending_ops_msg = mqtt_message_box.recv().await;

    // Publish firmware update operation to child device.
    publish_smartrest_firmware_operation(&mut mqtt_message_box).await?;

    // The first MQTT message after the c8y operation published should be firmware update request.
    let firmware_update_request = timeout(TEST_TIMEOUT_MS, mqtt_message_box.recv())
        .await?
        .unwrap();
    let operation_id = get_operation_id_from_firmware_update_request(firmware_update_request);

    // Publish a successful RESPONSE from child device
    publish_firmware_update_response("successful", &operation_id, &mut mqtt_message_box).await?;

    // Assert EXECUTING SmartREST message, installed firmware SmartREST message, SUCCESSFUL SmartREST message
    test_helpers::assert_received(
        &mut mqtt_message_box,
        TEST_TIMEOUT_MS,
        [
            MqttMessage::new(
                &Topic::new_unchecked(C8Y_CHILD_PUBLISH_TOPIC_NAME),
                "501,c8y_Firmware\n",
            ),
            MqttMessage::new(
                &Topic::new_unchecked(C8Y_CHILD_PUBLISH_TOPIC_NAME),
                format!("115,{FIRMWARE_NAME},{FIRMWARE_VERSION},{DOWNLOAD_URL}"),
            ),
            MqttMessage::new(
                &Topic::new_unchecked(C8Y_CHILD_PUBLISH_TOPIC_NAME),
                "503,c8y_Firmware,\n",
            ),
        ],
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn handle_response_executing_and_failed_child_device() -> Result<(), DynError> {
    let mut ttd = TempTedgeDir::new();
    let (mut mqtt_message_box, mut _c8y_proxy_message_box, mut _timer_message_box) =
        spawn_firmware_manager(&mut ttd, DEFAULT_REQUEST_TIMEOUT_SEC, true).await?;

    // On startup, SmartREST 500 should be sent by firmware manager.
    let _pending_ops_msg = mqtt_message_box.recv().await;

    // Publish firmware update operation to child device.
    publish_smartrest_firmware_operation(&mut mqtt_message_box).await?;

    // The first MQTT message after the c8y operation published should be firmware update request.
    let firmware_update_request = timeout(TEST_TIMEOUT_MS, mqtt_message_box.recv())
        .await?
        .unwrap();
    let operation_id = get_operation_id_from_firmware_update_request(firmware_update_request);

    // Publish a executing RESPONSE from child device
    publish_firmware_update_response("executing", &operation_id, &mut mqtt_message_box).await?;

    // Assert EXECUTING SmartREST MQTT message
    test_helpers::assert_received(
        &mut mqtt_message_box,
        TEST_TIMEOUT_MS,
        [MqttMessage::new(
            &Topic::new_unchecked(C8Y_CHILD_PUBLISH_TOPIC_NAME),
            "501,c8y_Firmware\n",
        )],
    )
    .await;

    // Publish a failed RESPONSE from child device
    publish_firmware_update_response("failed", &operation_id, &mut mqtt_message_box).await?;

    // Assert FAILED SmartREST message
    test_helpers::assert_received(
        &mut mqtt_message_box,
        TEST_TIMEOUT_MS,
        [MqttMessage::new(
            &Topic::new_unchecked(C8Y_CHILD_PUBLISH_TOPIC_NAME),
            "502,c8y_Firmware,\"No failure reason provided by child device.\"\n",
        )],
    )
    .await;

    Ok(())
}

// TODO: This test behaviour should be reconsidered once we get an operation ID from c8y.
#[tokio::test]
async fn ignore_response_with_invalid_status_child_device() -> Result<(), DynError> {
    let mut ttd = TempTedgeDir::new();
    let (mut mqtt_message_box, mut _c8y_proxy_message_box, mut _timer_message_box) =
        spawn_firmware_manager(&mut ttd, DEFAULT_REQUEST_TIMEOUT_SEC, true).await?;

    // On startup, SmartREST 500 should be sent by firmware manager.
    let _pending_ops_msg = mqtt_message_box.recv().await;

    // Publish firmware update operation to child device.
    publish_smartrest_firmware_operation(&mut mqtt_message_box).await?;

    // The first MQTT message after the c8y operation published should be firmware update request.
    let firmware_update_request = timeout(TEST_TIMEOUT_MS, mqtt_message_box.recv())
        .await?
        .unwrap();
    let operation_id = get_operation_id_from_firmware_update_request(firmware_update_request);

    // Publish an invalid RESPONSE from child device
    publish_firmware_update_response("invalid", &operation_id, &mut mqtt_message_box).await?;

    // No message is expected since the invalid status is reported as response.
    let result = timeout(TEST_TIMEOUT_MS, mqtt_message_box.recv()).await;
    assert!(result.is_err());

    Ok(())
}

#[tokio::test]
async fn handle_response_with_invalid_operation_id_child_device() -> Result<(), DynError> {
    let mut ttd = TempTedgeDir::new();
    let (mut mqtt_message_box, mut _c8y_proxy_message_box, mut _timer_message_box) =
        spawn_firmware_manager(&mut ttd, DEFAULT_REQUEST_TIMEOUT_SEC, true).await?;

    // On startup, SmartREST 500 should be sent by firmware manager.
    let _pending_ops_msg = mqtt_message_box.recv().await;

    // Publish firmware update operation to child device.
    publish_smartrest_firmware_operation(&mut mqtt_message_box).await?;

    // Ignore the first MQTT message after the c8y operation published (firmware update request).
    let _firmware_update_request_message = mqtt_message_box.recv().await;

    // Publish an invalid RESPONSE from child device
    publish_firmware_update_response("successful", "invalid_op_id", &mut mqtt_message_box).await?;

    // No message is expected since the invalid status is reported as response.
    let result = timeout(TEST_TIMEOUT_MS, mqtt_message_box.recv()).await;
    assert!(result.is_err());

    Ok(())
}

// FIXME: Timeout panics.
#[ignore]
#[tokio::test]
async fn handle_request_timeout_child_device() -> Result<(), DynError> {
    let mut ttd = TempTedgeDir::new();
    let (mut mqtt_message_box, mut _c8y_proxy_message_box, mut timer_message_box) =
        spawn_firmware_manager(&mut ttd, Duration::from_secs(1), true).await?;

    // On startup, SmartREST 500 should be sent by firmware manager.
    let _pending_ops_msg = mqtt_message_box.recv().await;

    // Publish firmware update operation to child device.
    publish_smartrest_firmware_operation(&mut mqtt_message_box).await?;

    // The first MQTT message after the c8y operation published should be firmware update request.
    let firmware_update_request = timeout(TEST_TIMEOUT_MS, mqtt_message_box.recv())
        .await?
        .unwrap();
    // Consider: Can get operation ID from SetTimeout.
    let operation_id = get_operation_id_from_firmware_update_request(firmware_update_request);

    let set_timeout_message = timer_message_box.recv().await.unwrap();
    dbg!(&set_timeout_message);

    // FIXME: timeout doesn't fire?
    // let timeout_message = timer_message_box.recv().await.unwrap();
    // dbg!(timeout_message);

    // Assert EXECUTING SmartREST message, FAILED SmartREST message
    let expected_failure_text =
        format!("Child device child-device did not respond within the timeout interval of 1sec. Operation ID={operation_id}");
    test_helpers::assert_received(
        &mut mqtt_message_box,
        TEST_TIMEOUT_MS,
        [
            MqttMessage::new(
                &Topic::new_unchecked(C8Y_CHILD_PUBLISH_TOPIC_NAME),
                "501,c8y_Firmware\n",
            ),
            MqttMessage::new(
                &Topic::new_unchecked(C8Y_CHILD_PUBLISH_TOPIC_NAME),
                format!("502,c8y_Firmware,\"{expected_failure_text}\""),
            ),
        ],
    )
    .await;

    Ok(())
}

fn get_operation_id_from_firmware_update_request(mqtt_message: MqttMessage) -> String {
    serde_json::from_str::<serde_json::Value>(from_utf8(&mqtt_message.payload).expect("UTF8"))
        .expect("Deserialize JSON")
        // received_json
        .get("id")
        .expect("'id' field exists")
        .as_str() // Cannot use "to_string()" directly as it will include `\`.
        .expect("string")
        .to_owned()
}

async fn publish_smartrest_firmware_operation(
    mqtt_message_box: &mut MqttMessageBox,
) -> Result<(), DynError> {
    let c8y_firmware_update_msg = MqttMessage::new(
        &Topic::new_unchecked("c8y/s/ds"),
        format!("515,{CHILD_DEVICE_ID},{FIRMWARE_NAME},{FIRMWARE_VERSION},{DOWNLOAD_URL}"),
    );
    mqtt_message_box.send(c8y_firmware_update_msg).await?;
    Ok(())
}

async fn publish_firmware_update_response(
    status: &str,
    operation_id: &str,
    mqtt_message_box: &mut MqttMessageBox,
) -> Result<(), DynError> {
    let firmware_update_response = MqttMessage::new(
        &Topic::new_unchecked(&format!(
            "tedge/{CHILD_DEVICE_ID}/commands/res/firmware_update"
        )),
        json!({
            "status": status,
            "id": operation_id,
        })
        .to_string(),
    );
    mqtt_message_box.send(firmware_update_response).await?;
    Ok(())
}

async fn spawn_firmware_manager(
    tmp_dir: &mut TempTedgeDir,
    timeout_sec: Duration,
    create_dir: bool,
) -> Result<
    (
        SimpleMessageBox<MqttMessage, MqttMessage>,
        SimpleMessageBox<C8YRestRequest, C8YRestResult>,
        SimpleMessageBox<OperationSetTimeout, OperationTimeout>,
    ),
    DynError,
> {
    if create_dir {
        create_required_directories(tmp_dir);
    }

    let device_id = "parent-device";
    let tedge_host: IpAddress = TEDGE_HOST.try_into().unwrap();

    let config = FirmwareManagerConfig::new(
        device_id.to_string(),
        tedge_host.into(),
        TEDGE_HTTP_PORT,
        tmp_dir.to_path_buf(),
        tmp_dir.to_path_buf(),
        timeout_sec,
    );

    let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 5);
    let mut c8y_proxy_builder: SimpleMessageBoxBuilder<C8YRestRequest, C8YRestResult> =
        SimpleMessageBoxBuilder::new("C8Y", 1);
    let mut timer_builder: SimpleMessageBoxBuilder<OperationSetTimeout, OperationTimeout> =
        SimpleMessageBoxBuilder::new("Timer", 5);

    let mut firmware_manager_builder = FirmwareManagerBuilder::new(config);

    firmware_manager_builder.with_c8y_http_proxy(&mut c8y_proxy_builder)?;
    firmware_manager_builder.set_connection(&mut mqtt_builder);
    firmware_manager_builder.set_connection(&mut timer_builder);

    let mqtt_message_box = mqtt_builder.build();
    let c8y_proxy_message_box = c8y_proxy_builder.build();
    let timer_message_box = timer_builder.build();

    let (actor, message_box) = firmware_manager_builder.build();
    let _join_handle = tokio::spawn(async move { actor.run(message_box).await });

    Ok((mqtt_message_box, c8y_proxy_message_box, timer_message_box))
}

fn create_required_directories(tmp_dir: &mut TempTedgeDir) {
    tmp_dir.dir("cache");
    tmp_dir.dir("file-transfer");
    tmp_dir.dir("firmware");
}
