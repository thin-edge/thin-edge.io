use super::*;
use crate::FirmwareManager;
use assert_json_diff::assert_json_include;
use c8y_api::http_proxy::MockC8YHttpProxy;
use c8y_api::smartrest::error::SMCumulocityMapperError;
use mockall::predicate;
use mqtt_tests::with_timeout::WithTimeout;
use mqtt_tests::StreamExt;
use serde_json::json;
use sha256::digest;
use std::sync::Arc;
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
    create_required_directories(&mut tmp_dir);

    let broker = mqtt_tests::test_mqtt_broker();
    let mut c8y_http_client = MockC8YHttpProxy::new();
    c8y_http_client
        .expect_download_file()
        .with(
            predicate::always(),
            predicate::always(),
            predicate::always(),
        )
        .returning(|_, file_name, tmp_dir_path| {
            let downloaded_path = tmp_dir_path.join(file_name);
            std::fs::File::create(&downloaded_path)?;
            Ok(downloaded_path)
        });

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        c8y_http_client,
        DEFAULT_REQUEST_TIMEOUT_SEC,
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
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn handle_request_file_download_failed() -> anyhow::Result<()> {
    let mut tmp_dir = TempTedgeDir::new();
    create_required_directories(&mut tmp_dir);

    let broker = mqtt_tests::test_mqtt_broker();
    let mut c8y_http_client = MockC8YHttpProxy::new();
    // Key: Download will return Err to imitate downloading failure.
    c8y_http_client
        .expect_download_file()
        .with(
            predicate::always(),
            predicate::always(),
            predicate::always(),
        )
        .returning(|_, _, _| Err(SMCumulocityMapperError::RequestTimeout));

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        c8y_http_client,
        DEFAULT_REQUEST_TIMEOUT_SEC,
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
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_Firmware", "502,c8y_Firmware"],
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
    let mut c8y_http_client = MockC8YHttpProxy::new();
    c8y_http_client
        .expect_download_file()
        .with(
            predicate::always(),
            predicate::always(),
            predicate::always(),
        )
        .returning(|_, file_name, tmp_dir_path| {
            let downloaded_path = tmp_dir_path.join(file_name);
            std::fs::File::create(&downloaded_path)?;
            Ok(downloaded_path)
        });

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        c8y_http_client,
        DEFAULT_REQUEST_TIMEOUT_SEC,
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
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_Firmware", "502,c8y_Firmware"],
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
    let mut c8y_http_client = MockC8YHttpProxy::new();
    c8y_http_client
        .expect_download_file()
        .with(
            predicate::always(),
            predicate::always(),
            predicate::always(),
        )
        .returning(|_, file_name, tmp_dir_path| {
            let downloaded_path = tmp_dir_path.join(file_name);
            std::fs::File::create(&downloaded_path)?;
            Ok(downloaded_path)
        });

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        c8y_http_client,
        DEFAULT_REQUEST_TIMEOUT_SEC,
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
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_Firmware", "502,c8y_Firmware"],
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
    let mut c8y_http_client = MockC8YHttpProxy::new();
    c8y_http_client
        .expect_download_file()
        .with(
            predicate::always(),
            predicate::always(),
            predicate::always(),
        )
        .returning(|_, file_name, tmp_dir_path| {
            let downloaded_path = tmp_dir_path.join(file_name);
            std::fs::File::create(&downloaded_path)?;
            Ok(downloaded_path)
        });

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        c8y_http_client,
        DEFAULT_REQUEST_TIMEOUT_SEC,
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
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_Firmware", "502,c8y_Firmware"],
    )
    .await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn handle_request_timeout_child_device() -> anyhow::Result<()> {
    let mut tmp_dir = TempTedgeDir::new();
    create_required_directories(&mut tmp_dir);

    let broker = mqtt_tests::test_mqtt_broker();
    let mut c8y_http_client = MockC8YHttpProxy::new();
    c8y_http_client
        .expect_download_file()
        .with(
            predicate::always(),
            predicate::always(),
            predicate::always(),
        )
        .returning(|_, file_name, tmp_dir_path| {
            let downloaded_path = tmp_dir_path.join(file_name);
            std::fs::File::create(&downloaded_path)?;
            Ok(downloaded_path)
        });

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        c8y_http_client,
        Duration::from_secs(1),
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
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_Firmware", "502,c8y_Firmware"],
    )
    .await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn handle_response_successful_child_device() -> anyhow::Result<()> {
    let mut tmp_dir = TempTedgeDir::new();
    create_required_directories(&mut tmp_dir);

    let broker = mqtt_tests::test_mqtt_broker();
    let mut c8y_http_client = MockC8YHttpProxy::new();
    c8y_http_client
        .expect_download_file()
        .with(
            predicate::always(),
            predicate::always(),
            predicate::always(),
        )
        .returning(|_, file_name, tmp_dir_path| {
            let downloaded_path = tmp_dir_path.join(file_name);
            std::fs::File::create(&downloaded_path)?;
            Ok(downloaded_path)
        });

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        c8y_http_client,
        DEFAULT_REQUEST_TIMEOUT_SEC,
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
                "reason": null
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
    create_required_directories(&mut tmp_dir);

    let broker = mqtt_tests::test_mqtt_broker();
    let mut c8y_http_client = MockC8YHttpProxy::new();
    c8y_http_client
        .expect_download_file()
        .with(
            predicate::always(),
            predicate::always(),
            predicate::always(),
        )
        .returning(|_, file_name, tmp_dir_path| {
            let downloaded_path = tmp_dir_path.join(file_name);
            std::fs::File::create(&downloaded_path)?;
            Ok(downloaded_path)
        });

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        c8y_http_client,
        DEFAULT_REQUEST_TIMEOUT_SEC,
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

    // Assert the c8y_Firmware operation status mapping to EXECUTING(501), SUCCESSFUL(503), and Set Firmware(115)
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_Firmware", "502,c8y_Firmware,\"failure reason\""],
    )
    .await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn handle_response_with_invalid_status_child_device() -> anyhow::Result<()> {
    let mut tmp_dir = TempTedgeDir::new();
    create_required_directories(&mut tmp_dir);

    let broker = mqtt_tests::test_mqtt_broker();
    let mut c8y_http_client = MockC8YHttpProxy::new();
    c8y_http_client
        .expect_download_file()
        .with(
            predicate::always(),
            predicate::always(),
            predicate::always(),
        )
        .returning(|_, file_name, tmp_dir_path| {
            let downloaded_path = tmp_dir_path.join(file_name);
            std::fs::File::create(&downloaded_path)?;
            Ok(downloaded_path)
        });

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        c8y_http_client,
        DEFAULT_REQUEST_TIMEOUT_SEC,
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
                "reason": null
            })
            .to_string(),
        )
        .await?;

    // Assert the c8y_Firmware operation status mapping to EXECUTING(501), SUCCESSFUL(503), and Set Firmware(115)
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_Firmware", "502,c8y_Firmware"],
    )
    .await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn handle_response_with_invalid_operation_id_child_device() -> anyhow::Result<()> {
    let mut tmp_dir = TempTedgeDir::new();
    create_required_directories(&mut tmp_dir);

    let broker = mqtt_tests::test_mqtt_broker();
    let mut c8y_http_client = MockC8YHttpProxy::new();
    c8y_http_client
        .expect_download_file()
        .with(
            predicate::always(),
            predicate::always(),
            predicate::always(),
        )
        .returning(|_, file_name, tmp_dir_path| {
            let downloaded_path = tmp_dir_path.join(file_name);
            std::fs::File::create(&downloaded_path)?;
            Ok(downloaded_path)
        });

    start_firmware_manager(
        &mut tmp_dir,
        broker.port,
        c8y_http_client,
        DEFAULT_REQUEST_TIMEOUT_SEC,
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

    // Publish an invalid RESPONSE from child device
    broker
        .publish(
            &format!("tedge/{CHILD_DEVICE_ID}/commands/res/firmware_update"),
            &json!({
                "status": "successful",
                "id": "invalid_op_id",
                "reason": null
            })
            .to_string(),
        )
        .await?;

    // Assert the c8y_Firmware operation status mapping to EXECUTING(501), SUCCESSFUL(503), and Set Firmware(115)
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_Firmware", "502,c8y_Firmware"],
    )
    .await;

    Ok(())
}

async fn start_firmware_manager(
    tmp_dir: &mut TempTedgeDir,
    port: u16,
    http_client: MockC8YHttpProxy,
    timeout_sec: Duration,
) -> anyhow::Result<()> {
    let mut firmware_manager = FirmwareManager::new(
        "tedge_device_id".to_string(),
        port,
        Arc::new(Mutex::new(http_client)),
        mockito::server_address().to_string(),
        tmp_dir.to_path_buf(),
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
