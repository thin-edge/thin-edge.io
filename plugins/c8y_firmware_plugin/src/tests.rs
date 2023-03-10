use super::*;
use crate::FirmwareManager;
use assert_json_diff::assert_json_include;
use c8y_api::smartrest::error::SMCumulocityMapperError;
use futures::channel::mpsc::UnboundedReceiver;
use futures::channel::mpsc::UnboundedSender;
use futures::SinkExt;
use mqtt_tests::with_timeout::WithTimeout;
use mqtt_tests::StreamExt;
use serde_json::json;
use sha256::digest;
use tedge_test_utils::fs::TempTedgeDir;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);
const DEFAULT_REQUEST_TIMEOUT_SEC: Duration = Duration::from_secs(3600);

const CHILD_DEVICE_ID: &str = "child-device";
const FIRMWARE_NAME: &str = "fw-name";
const FIRMWARE_VERSION: &str = "fw-version";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn handle_request_child_device() -> anyhow::Result<()> {
    let mut tmp_dir = TempTedgeDir::new();
    let broker = mqtt_tests::test_mqtt_broker();

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        DEFAULT_REQUEST_TIMEOUT_SEC,
        true,
        None,
    )
    .await?;

    // Mock download endpoint for the plugin to download a firmware from the cloud
    let mock_http_server_host = mockito::server_url();
    let cloud_firmware_url = format!("{mock_http_server_host}/some/cloud/url");
    let file_cache_key = digest(cloud_firmware_url.clone());

    // Subscribe tedge request endpoint for firmware update
    let mut tedge_command_messages = broker
        .messages_published_on(&format!(
            "tedge/{CHILD_DEVICE_ID}/commands/req/firmware_update"
        ))
        .await;

    // Publish a c8y_Firmware operation to the plugin
    broker
        .publish(
            "c8y/s/ds",
            format!(
                "515,{CHILD_DEVICE_ID},{FIRMWARE_NAME},{FIRMWARE_VERSION},{cloud_firmware_url}"
            )
            .as_str(),
        )
        .await?;

    // Check if the received message payload contains some expected fields and value.
    let received_message = tedge_command_messages
        .next()
        .with_timeout(TEST_TIMEOUT_MS)
        .await?
        .expect("No message received.");
    let received_json: serde_json::Value = serde_json::from_str(&received_message)?;
    let expected_request_payload = json!({
        "attempt": 1,
        "name": FIRMWARE_NAME,
        "version": FIRMWARE_VERSION,
        "url": format!("{mock_http_server_host}/tedge/file-transfer/{CHILD_DEVICE_ID}/firmware_update/{file_cache_key}")
    });

    assert_json_include!(actual: received_json, expected: expected_request_payload);

    // Assert that the downloaded file is present in the cache
    assert!(tmp_dir
        .to_path_buf()
        .join("cache")
        .join(&file_cache_key)
        .exists());

    // Assert that the downloaded file is present in the file-transfer repo
    assert!(tmp_dir
        .to_path_buf()
        .join("file-transfer")
        .join(CHILD_DEVICE_ID)
        .join("firmware_update")
        .join(&file_cache_key)
        .exists());

    // Assert that the operation file is created in persistence store
    assert!(tmp_dir
        .to_path_buf()
        .join("firmware")
        .read_dir()?
        .next()
        .is_some());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn handle_request_file_download_failed() -> anyhow::Result<()> {
    let mut tmp_dir = TempTedgeDir::new();
    let broker = mqtt_tests::test_mqtt_broker();

    // Mock DownloadManager sending timeout response
    let (req_sndr, mut req_rcvr) = mpsc::unbounded::<DownloadRequest>();
    let (mut res_sndr, res_rcvr) = mpsc::unbounded::<DownloadResponse>();
    tokio::spawn(async move {
        if let Some(req) = req_rcvr.next().await {
            let response =
                DownloadResponse::new(&req.id, Err(SMCumulocityMapperError::RequestTimeout));

            let _ = res_sndr.send(response).await;
        }
    });

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        DEFAULT_REQUEST_TIMEOUT_SEC,
        true,
        Some((req_sndr, res_rcvr)),
    )
    .await?;

    // Mock download endpoint for the plugin to download a firmware from the cloud
    let mock_http_server_host = mockito::server_url();
    let cloud_firmware_url = format!("{mock_http_server_host}/some/cloud/url");

    // Subscribe SmartREST endpoint for child device
    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{CHILD_DEVICE_ID}").as_str())
        .await;

    // Publish a c8y_Firmware operation to the plugin
    broker
        .publish(
            "c8y/s/ds",
            format!(
                "515,{CHILD_DEVICE_ID},{FIRMWARE_NAME},{FIRMWARE_VERSION},{cloud_firmware_url}"
            )
            .as_str(),
        )
        .await?;

    // Assert the c8y_Firmware operation status mapping to EXECUTING(501) and FAILED(502)
    // The failure reason depends on what the mocked http client's download_file() returns.
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &[
            "501,c8y_Firmware",
            &format!(
                "502,c8y_Firmware,\"Download from {cloud_firmware_url} failed with Request timed out\"",
            ),
        ],
    )
    .await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn handle_request_dir_cache_not_found() -> anyhow::Result<()> {
    let mut tmp_dir = TempTedgeDir::new();
    tmp_dir.dir("file-transfer");
    tmp_dir.dir("firmware");

    let broker = mqtt_tests::test_mqtt_broker();

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        DEFAULT_REQUEST_TIMEOUT_SEC,
        false,
        None,
    )
    .await?;

    // Mock download endpoint for the plugin to download a firmware from the cloud
    let mock_http_server_host = mockito::server_url();
    let cloud_firmware_url = format!("{mock_http_server_host}/some/cloud/url");

    // Subscribe SmartREST endpoint for child device
    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{CHILD_DEVICE_ID}").as_str())
        .await;

    // Publish a c8y_Firmware operation to the plugin
    broker
        .publish(
            "c8y/s/ds",
            format!(
                "515,{CHILD_DEVICE_ID},{FIRMWARE_NAME},{FIRMWARE_VERSION},{cloud_firmware_url}"
            )
            .as_str(),
        )
        .await?;

    // Assert the c8y_Firmware operation status mapping to EXECUTING(501) and FAILED(502)
    let expected_failure_text =
        format!("Directory {}/cache is not found. Run 'c8y-firmware-plugin --init' to create the directory.", tmp_dir.path().display());
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &[
            "501,c8y_Firmware",
            &format!("502,c8y_Firmware,\"{}\"", expected_failure_text),
        ],
    )
    .await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn handle_request_dir_file_transfer_not_found() -> anyhow::Result<()> {
    let mut tmp_dir = TempTedgeDir::new();
    tmp_dir.dir("cache");
    tmp_dir.dir("firmware");

    let broker = mqtt_tests::test_mqtt_broker();

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        DEFAULT_REQUEST_TIMEOUT_SEC,
        false,
        None,
    )
    .await?;

    // Mock download endpoint for the plugin to download a firmware from the cloud
    let mock_http_server_host = mockito::server_url();
    let cloud_firmware_url = format!("{mock_http_server_host}/some/cloud/url");

    // Subscribe SmartREST endpoint for child device
    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{CHILD_DEVICE_ID}").as_str())
        .await;

    // Publish a c8y_Firmware operation to the plugin
    broker
        .publish(
            "c8y/s/ds",
            format!(
                "515,{CHILD_DEVICE_ID},{FIRMWARE_NAME},{FIRMWARE_VERSION},{cloud_firmware_url}"
            )
            .as_str(),
        )
        .await?;

    // Assert the c8y_Firmware operation status mapping to EXECUTING(501) and FAILED(502)
    let expected_failure_text =
        format!("Directory {}/file-transfer is not found. Run 'c8y-firmware-plugin --init' to create the directory.", tmp_dir.path().display());
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &[
            "501,c8y_Firmware",
            &format!("502,c8y_Firmware,\"{}\"", expected_failure_text),
        ],
    )
    .await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn handle_request_dir_firmware_not_found() -> anyhow::Result<()> {
    let mut tmp_dir = TempTedgeDir::new();
    tmp_dir.dir("cache");
    tmp_dir.dir("file-transfer");

    let broker = mqtt_tests::test_mqtt_broker();

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        DEFAULT_REQUEST_TIMEOUT_SEC,
        false,
        None,
    )
    .await?;

    // Mock download endpoint for the plugin to download a firmware from the cloud
    let mock_http_server_host = mockito::server_url();
    let cloud_firmware_url = format!("{mock_http_server_host}/some/cloud/url");

    // Subscribe SmartREST endpoint for child device
    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{CHILD_DEVICE_ID}").as_str())
        .await;

    // Publish a c8y_Firmware operation to the plugin
    broker
        .publish(
            "c8y/s/ds",
            format!(
                "515,{CHILD_DEVICE_ID},{FIRMWARE_NAME},{FIRMWARE_VERSION},{cloud_firmware_url}"
            )
            .as_str(),
        )
        .await?;

    // Assert the c8y_Firmware operation status mapping to EXECUTING(501) and FAILED(502)
    let expected_failure_text =
        format!("Directory {}/firmware is not found. Run 'c8y-firmware-plugin --init' to create the directory.", tmp_dir.path().display());
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &[
            "501,c8y_Firmware",
            &format!("502,c8y_Firmware,\"{}\"", expected_failure_text),
        ],
    )
    .await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn handle_request_timeout_child_device() -> anyhow::Result<()> {
    let mut tmp_dir = TempTedgeDir::new();
    let broker = mqtt_tests::test_mqtt_broker();

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        Duration::from_secs(1),
        true,
        None,
    )
    .await?;

    // Mock download endpoint for the plugin to download a firmware from the cloud
    let mock_http_server_host = mockito::server_url();
    let cloud_firmware_url = format!("{mock_http_server_host}/some/cloud/url");

    // Subscribe SmartREST endpoint for child device
    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{CHILD_DEVICE_ID}").as_str())
        .await;

    // Subscribe tedge request endpoint for firmware update
    let mut tedge_command_messages = broker
        .messages_published_on(&format!(
            "tedge/{CHILD_DEVICE_ID}/commands/req/firmware_update"
        ))
        .await;

    // Publish a c8y_Firmware operation to the plugin
    broker
        .publish(
            "c8y/s/ds",
            format!(
                "515,{CHILD_DEVICE_ID},{FIRMWARE_NAME},{FIRMWARE_VERSION},{cloud_firmware_url}"
            )
            .as_str(),
        )
        .await?;

    // Firmware update REQUEST should be received.
    let received_message = tedge_command_messages
        .next()
        .with_timeout(TEST_TIMEOUT_MS)
        .await?
        .expect("No message received.");
    let received_json: serde_json::Value = serde_json::from_str(&received_message)?;
    let operation_id = received_json.get("id").unwrap().as_str().unwrap();

    // Assert the c8y_Firmware operation status mapping to EXECUTING(501) and FAILED(502)
    let expected_failure_text =
        format!("Child device child-device did not respond within the timeout interval of 1sec. Operation ID={operation_id}");
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &[
            "501,c8y_Firmware",
            &format!("502,c8y_Firmware,\"{}\"", expected_failure_text),
        ],
    )
    .await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn handle_response_successful_child_device() -> anyhow::Result<()> {
    let mut tmp_dir = TempTedgeDir::new();
    let broker = mqtt_tests::test_mqtt_broker();

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        DEFAULT_REQUEST_TIMEOUT_SEC,
        true,
        None,
    )
    .await?;

    // Mock download endpoint for the plugin to download a firmware from the cloud
    let mock_http_server_host = mockito::server_url();
    let cloud_firmware_url = format!("{mock_http_server_host}/some/cloud/url");

    // Subscribe tedge request endpoint for firmware update
    let mut tedge_command_messages = broker
        .messages_published_on(&format!(
            "tedge/{CHILD_DEVICE_ID}/commands/req/firmware_update"
        ))
        .await;

    // Publish a c8y_Firmware operation to the plugin.
    broker
        .publish(
            "c8y/s/ds",
            format!(
                "515,{CHILD_DEVICE_ID},{FIRMWARE_NAME},{FIRMWARE_VERSION},{cloud_firmware_url}"
            )
            .as_str(),
        )
        .await?;

    // Firmware update REQUEST should be received.
    let received_message = tedge_command_messages
        .next()
        .with_timeout(TEST_TIMEOUT_MS)
        .await?
        .expect("No message received.");
    let received_json: serde_json::Value = serde_json::from_str(&received_message)?;
    let operation_id = received_json.get("id").unwrap().as_str().unwrap();

    // Subscribe SmartREST endpoint for child device
    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{CHILD_DEVICE_ID}").as_str())
        .await;

    // Publish a successful RESPONSE from child device
    broker
        .publish(
            &format!("tedge/{CHILD_DEVICE_ID}/commands/res/firmware_update"),
            &json!({
                "status": "successful",
                "id": operation_id,
            })
            .to_string(),
        )
        .await?;

    // Assert the c8y_Firmware operation status mapping to EXECUTING(501), SUCCESSFUL(503), and Set Firmware(115)
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &[
            "501,c8y_Firmware",
            format!("115,{FIRMWARE_NAME},{FIRMWARE_VERSION},{cloud_firmware_url}").as_str(),
            "503,c8y_Firmware",
        ],
    )
    .await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn handle_response_executing_and_failed_child_device() -> anyhow::Result<()> {
    let mut tmp_dir = TempTedgeDir::new();
    let broker = mqtt_tests::test_mqtt_broker();

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        DEFAULT_REQUEST_TIMEOUT_SEC,
        true,
        None,
    )
    .await?;

    // Mock download endpoint for the plugin to download a firmware from the cloud
    let mock_http_server_host = mockito::server_url();
    let cloud_firmware_url = format!("{mock_http_server_host}/some/cloud/url");

    // Subscribe tedge request endpoint for firmware update
    let mut tedge_command_messages = broker
        .messages_published_on(&format!(
            "tedge/{CHILD_DEVICE_ID}/commands/req/firmware_update"
        ))
        .await;

    // Publish a c8y_Firmware operation to the plugin.
    broker
        .publish(
            "c8y/s/ds",
            format!(
                "515,{CHILD_DEVICE_ID},{FIRMWARE_NAME},{FIRMWARE_VERSION},{cloud_firmware_url}"
            )
            .as_str(),
        )
        .await?;

    // Firmware update REQUEST should be received.
    let received_message = tedge_command_messages
        .next()
        .with_timeout(TEST_TIMEOUT_MS)
        .await?
        .expect("No message received.");
    let received_json: serde_json::Value = serde_json::from_str(&received_message)?;
    let operation_id = received_json.get("id").unwrap().as_str().unwrap();

    // Subscribe SmartREST endpoint for child device
    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{CHILD_DEVICE_ID}").as_str())
        .await;

    // Publish a executing RESPONSE from child device
    broker
        .publish(
            &format!("tedge/{CHILD_DEVICE_ID}/commands/res/firmware_update"),
            &json!({
                "status": "executing",
                "id": operation_id,
            })
            .to_string(),
        )
        .await?;

    // Assert the c8y_Firmware operation status mapping to EXECUTING(501)
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_Firmware"],
    )
    .await;

    // Publish a failed RESPONSE from child device
    broker
        .publish(
            &format!("tedge/{CHILD_DEVICE_ID}/commands/res/firmware_update"),
            &json!({
                "status": "failed",
                "id": operation_id,
                "reason": "failure reason"
            })
            .to_string(),
        )
        .await?;

    // Assert the c8y_Firmware operation status mapping to FAILED(502)
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["502,c8y_Firmware,\"failure reason\""],
    )
    .await;

    Ok(())
}

// TODO: This test behaviour should be reconsidered once we get an operation ID from c8y.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn ignore_response_with_invalid_status_child_device() -> anyhow::Result<()> {
    let mut tmp_dir = TempTedgeDir::new();
    let broker = mqtt_tests::test_mqtt_broker();

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        DEFAULT_REQUEST_TIMEOUT_SEC,
        true,
        None,
    )
    .await?;

    // Mock download endpoint for the plugin to download a firmware from the cloud
    let mock_http_server_host = mockito::server_url();
    let cloud_firmware_url = format!("{mock_http_server_host}/some/cloud/url");

    // Subscribe tedge request endpoint for firmware update
    let mut tedge_command_messages = broker
        .messages_published_on(&format!(
            "tedge/{CHILD_DEVICE_ID}/commands/req/firmware_update"
        ))
        .await;

    // Publish a c8y_Firmware operation to the plugin.
    broker
        .publish(
            "c8y/s/ds",
            format!(
                "515,{CHILD_DEVICE_ID},{FIRMWARE_NAME},{FIRMWARE_VERSION},{cloud_firmware_url}"
            )
            .as_str(),
        )
        .await?;

    // Firmware update REQUEST should be received.
    let received_message = tedge_command_messages
        .next()
        .with_timeout(TEST_TIMEOUT_MS)
        .await?
        .expect("No message received.");
    let received_json: serde_json::Value = serde_json::from_str(&received_message)?;
    let operation_id = received_json.get("id").unwrap().as_str().unwrap();

    // Subscribe SmartREST endpoint for child device
    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{CHILD_DEVICE_ID}").as_str())
        .await;

    // Publish an invalid RESPONSE from child device
    broker
        .publish(
            &format!("tedge/{CHILD_DEVICE_ID}/commands/res/firmware_update"),
            &json!({
                "status": "invalid",
                "id": operation_id,
            })
            .to_string(),
        )
        .await?;

    // No message is expected since the invalid status is reported as response.
    let result = smartrest_messages
        .next()
        .with_timeout(TEST_TIMEOUT_MS)
        .await;
    assert!(result.is_err());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn handle_response_with_invalid_operation_id_child_device() -> anyhow::Result<()> {
    let mut tmp_dir = TempTedgeDir::new();
    let broker = mqtt_tests::test_mqtt_broker();

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        DEFAULT_REQUEST_TIMEOUT_SEC,
        true,
        None,
    )
    .await?;

    // Mock download endpoint for the plugin to download a firmware from the cloud
    let mock_http_server_host = mockito::server_url();
    let cloud_firmware_url = format!("{mock_http_server_host}/some/cloud/url");

    // Publish a c8y_Firmware operation to the plugin.
    broker
        .publish(
            "c8y/s/ds",
            format!(
                "515,{CHILD_DEVICE_ID},{FIRMWARE_NAME},{FIRMWARE_VERSION},{cloud_firmware_url}"
            )
            .as_str(),
        )
        .await?;

    // Subscribe SmartREST endpoint for child device
    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{CHILD_DEVICE_ID}").as_str())
        .await;

    // Publish a response with invalid operation_id from the child device
    broker
        .publish(
            &format!("tedge/{CHILD_DEVICE_ID}/commands/res/firmware_update"),
            &json!({
                "status": "successful",
                "id": "invalid_op_id",
            })
            .to_string(),
        )
        .await?;

    // No message is expected since the response with invalid operation_id is ignored.
    let result = smartrest_messages
        .next()
        .with_timeout(TEST_TIMEOUT_MS)
        .await;
    assert!(result.is_err());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn handle_child_response_while_busy_downloading() -> anyhow::Result<()> {
    let mut tmp_dir = TempTedgeDir::new();
    let broker = mqtt_tests::test_mqtt_broker();

    // Mock download endpoint for the plugin to download a firmware from the cloud
    let child1 = "child1";
    let child2 = "child2";
    let mock_http_server_host = mockito::server_url();
    let child1_firmware_url = format!("{mock_http_server_host}/{child1}/cloud/url");
    let child2_firmware_url = format!("{mock_http_server_host}/{child2}/cloud/url");

    // Mock DownloadManager that does not complete download for child 2
    let (req_sndr, mut req_rcvr) = mpsc::unbounded::<DownloadRequest>();
    let (mut res_sndr, res_rcvr) = mpsc::unbounded::<DownloadResponse>();
    let tmp_dir_clone = tmp_dir.clone();
    let child2_firmware_url_clone = child2_firmware_url.clone();
    let (mut child2_download_signal_sndr, mut child2_download_signal_rcvr) = mpsc::channel(1);
    tokio::spawn(async move {
        while let Some(req) = req_rcvr.next().await {
            if req.url == child2_firmware_url_clone {
                // Do not finish the download for child 2 but send a signal that download started
                child2_download_signal_sndr.send(()).await.unwrap();
            } else {
                // Finish the download for other child devices
                let downloaded_firmware = tmp_dir_clone.file(&req.file_name);
                let response = DownloadResponse {
                    id: req.id,
                    result: Ok(downloaded_firmware.file_path),
                };

                let _ = res_sndr.send(response).await;
            }
        }
    });

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        DEFAULT_REQUEST_TIMEOUT_SEC,
        true,
        Some((req_sndr, res_rcvr)),
    )
    .await?;

    // Subscribe to tedge firmware_update requests for child 1
    let mut tedge_command_messages = broker
        .messages_published_on(&format!("tedge/{child1}/commands/req/firmware_update"))
        .await;

    // Publish a c8y_Firmware operation for child 1
    broker
        .publish(
            "c8y/s/ds",
            &format!("515,{child1},{FIRMWARE_NAME},{FIRMWARE_VERSION},{child1_firmware_url}"),
        )
        .await?;

    // Wait till tedge firmware_update command is published for child 1
    let received_message = tedge_command_messages
        .next()
        .with_timeout(TEST_TIMEOUT_MS)
        .await?
        .expect("No message received.");
    let received_json: serde_json::Value = serde_json::from_str(&received_message)?;
    let child1_op_id = received_json.get("id").unwrap().as_str().unwrap();

    // Publish a c8y_Firmware operation for child 2
    broker
        .publish(
            "c8y/s/ds",
            &format!("515,{child2},{FIRMWARE_NAME},{FIRMWARE_VERSION},{child2_firmware_url}"),
        )
        .await?;

    // Wait till download starts for child 2
    let result = child2_download_signal_rcvr
        .next()
        .with_timeout(Duration::from_secs(60))
        .await;
    assert!(result.is_ok(), "Firmware download did not start");

    // Subscribe to SmartREST responses of child 1
    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{child1}").as_str())
        .await;

    // Publish an executing response from child 1 when the download for child 2 is still in progress
    broker
        .publish(
            &format!("tedge/{child1}/commands/res/firmware_update"),
            &json!({
                "status": "executing",
                "id": child1_op_id,
            })
            .to_string(),
        )
        .await?;

    // Assert that the EXECUTING response from child 1 is mapped even when the download for child 2 is still in progress
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_Firmware"],
    )
    .await;

    Ok(())
}

async fn start_firmware_manager(
    tmp_dir: &mut TempTedgeDir,
    port: u16,
    timeout_sec: Duration,
    init: bool,
    download_handle: Option<(
        UnboundedSender<DownloadRequest>,
        UnboundedReceiver<DownloadResponse>,
    )>,
) -> anyhow::Result<()> {
    if init {
        create_required_directories(tmp_dir);
    }

    let (req_sndr, res_rcvr) = if let Some(handle) = download_handle {
        (handle.0, handle.1)
    } else {
        let (req_sndr, mut req_rcvr) = mpsc::unbounded::<DownloadRequest>();
        let (mut res_sndr, res_rcvr) = mpsc::unbounded::<DownloadResponse>();
        let tmp_dir_clone = tmp_dir.clone();
        tokio::spawn(async move {
            if let Some(req) = req_rcvr.next().await {
                let downloaded_firmware = tmp_dir_clone.file(&req.file_name);
                let response = DownloadResponse {
                    id: req.id,
                    result: Ok(downloaded_firmware.file_path),
                };
                let _ = res_sndr.send(response).await;
            }
        });

        (req_sndr, res_rcvr)
    };

    let firmware_manager = FirmwareManager::new(
        "tedge_device_id".to_string(),
        "localhost".to_string(),
        port,
        req_sndr,
        res_rcvr,
        mockito::server_address().to_string(),
        tmp_dir.to_path_buf(),
        timeout_sec,
    )
    .await?;

    // Run the plugin's runtime logic in an async task
    tokio::spawn(async move {
        let _ = firmware_manager.run().await;
    });

    Ok(())
}

fn create_required_directories(tmp_dir: &mut TempTedgeDir) {
    tmp_dir.dir("cache");
    tmp_dir.dir("file-transfer");
    tmp_dir.dir("firmware");
}
