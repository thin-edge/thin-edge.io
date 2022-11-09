use crate::child_device::{ChildDeviceRequestPayload, ChildDeviceResponsePayload};

use super::*;
use agent_interface::OperationStatus;
use c8y_api::{http_proxy::MockC8YHttpProxy, smartrest::error::SMCumulocityMapperError};
use mockall::predicate;
use std::time::Duration;
use tedge_test_utils::fs::TempTedgeDir;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn test_handle_config_upload_request_tedge_device() -> anyhow::Result<()> {
    let tedge_device_id = "tedge-device";
    let test_config_type = "test-config";
    let test_config_path = "/some/test/config";
    let c8y_config_plugin_type = "c8y-configuration-plugin";
    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let broker = mqtt_tests::test_mqtt_broker();

    let mut messages = broker.messages_published_on("c8y/s/us").await;

    let mut http_client = MockC8YHttpProxy::new();
    http_client
        .expect_upload_config_file()
        .with(
            predicate::always(),
            predicate::eq(test_config_type),
            predicate::eq(None),
        )
        .return_once(
            |_path, _type, _child_id| Ok("http://server/some/test/config/url".to_string()),
        );
    http_client
        .expect_upload_config_file()
        .with(
            predicate::always(),
            predicate::eq(c8y_config_plugin_type),
            predicate::eq(None),
        )
        .return_once(|_path, _type, _child_id| {
            Ok("http://server/c8y/config/plugin/url".to_string())
        });

    // Run the plugin's runtime logic in an async task
    tokio::spawn(async move {
        let _ = run(
            tedge_device_id.into(),
            broker.port,
            &mut http_client,
            "localhost",
            ttd.path().to_path_buf(),
            ttd.path(),
        )
        .await;
    });

    // Assert supported config types message(119) on plugin startup
    mqtt_tests::assert_received_all_expected(
        &mut messages,
        TEST_TIMEOUT_MS,
        &[format!("119,{c8y_config_plugin_type},{test_config_type}")],
    )
    .await;

    // Send a config upload request to the plugin
    broker
        .publish(
            "c8y/s/ds",
            format!("526,{tedge_device_id},{test_config_type}").as_str(),
        )
        .await?;

    // Assert the c8y_UploadConfigFile operation transitioning from EXECUTING(501) to SUCCESSFUL(503) with the uploaded config URL
    mqtt_tests::assert_received_all_expected(
        &mut messages,
        TEST_TIMEOUT_MS,
        &[
            "501,c8y_UploadConfigFile",
            "503,c8y_UploadConfigFile,http://server/some/test/config/url",
        ],
    )
    .await;

    // Send a config upload request for `c8y-configuration-plugin` type to the plugin
    broker
        .publish(
            "c8y/s/ds",
            format!("526,{tedge_device_id},{c8y_config_plugin_type}").as_str(),
        )
        .await?;

    // Assert the c8y_UploadConfigFile operation transitioning from EXECUTING(501) to SUCCESSFUL(503) with the uploaded config URL
    mqtt_tests::assert_received_all_expected(
        &mut messages,
        TEST_TIMEOUT_MS,
        &[
            "501,c8y_UploadConfigFile",
            "503,c8y_UploadConfigFile,http://server/c8y/config/plugin/url",
        ],
    )
    .await;

    Ok(())
}

// Test c8y_UploadConfigFile SmartREST request mapping to tedge config_snapshot command
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn test_handle_config_upload_request_child_device() -> anyhow::Result<()> {
    let tedge_device_id = "tedge-device";
    let child_device_id = "child-aa";
    let config_type = "file_a";
    let test_config_path = "/some/test/config";

    let tmp_dir = TempTedgeDir::new();
    tmp_dir
        .dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = "file_a" }
            ]
        });

    let server_address = mockito::server_address().to_string();
    let broker = mqtt_tests::test_mqtt_broker();
    let mut c8y_http_client = MockC8YHttpProxy::new();

    // Run the plugin's runtime logic in an async task
    tokio::spawn(async move {
        let _ = run(
            tedge_device_id.into(),
            broker.port,
            &mut c8y_http_client,
            &mockito::server_address().to_string(),
            tmp_dir.path().to_path_buf(),
            tmp_dir.path(),
        )
        .await;
    });

    let mut tedge_command_messages = broker
        .messages_published_on(&format!(
            "tedge/{child_device_id}/commands/req/config_snapshot"
        ))
        .await;

    // Send a c8y_UploadConfigFile request to the plugin
    broker
        .publish(
            "c8y/s/ds",
            format!("526,{child_device_id},{config_type}").as_str(),
        )
        .await?;

    let expected_request = ChildDeviceRequestPayload {
        url: format!(
            "http://{server_address}/tedge/file-transfer/{child_device_id}/config_snapshot/file_a"
        ),
        path: test_config_path.into(),
        config_type: Some(config_type.into()),
    };
    let expected_request = serde_json::to_string(&expected_request)?;

    // Assert the mapping from c8y_UploadConfigFile request to tedge config_snapshot command
    mqtt_tests::assert_received_all_expected(
        &mut tedge_command_messages,
        TEST_TIMEOUT_MS,
        &[expected_request],
    )
    .await;

    Ok(())
}

// Test tedge config_snapshot command executing response mapping to SmartREST
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn test_handle_config_upload_executing_response_child_device() -> anyhow::Result<()> {
    let tedge_device_id = "tedge-device";
    let child_device_id = "child-device";
    let config_type = "config_type";
    let test_config_path = "/some/test/config";

    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = config_type }
            ]
        });

    let broker = mqtt_tests::test_mqtt_broker();
    let mut c8y_http_client = MockC8YHttpProxy::new();

    // Run the plugin's runtime logic in an async task
    tokio::spawn(async move {
        let _ = run(
            tedge_device_id.into(),
            broker.port,
            &mut c8y_http_client,
            &mockito::server_url(),
            ttd.path().to_path_buf(),
            ttd.path(),
        )
        .await;
    });

    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{child_device_id}").as_str())
        .await;

    // Fake config_snapshot executing status response from child device
    //
    broker
        .publish(
            &format!("tedge/{child_device_id}/commands/res/config_snapshot"),
            &serde_json::to_string(&ChildDeviceResponsePayload {
                status: Some(OperationStatus::Executing),
                path: test_config_path.into(),
                config_type: config_type.into(),
                reason: None,
            })
            .unwrap(),
        )
        .await?;

    // Assert the c8y_UploadConfigFile operation status mapping to EXECUTING(501)
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_UploadConfigFile"],
    )
    .await;

    Ok(())
}

// Test tedge config_snapshot command failed response mapping to SmartREST
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn test_handle_config_upload_failed_response_child_device() -> anyhow::Result<()> {
    let tedge_device_id = "tedge-device";
    let child_device_id = "child-device";
    let config_type = "config_type";
    let test_config_path = "/some/test/config";

    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = config_type }
            ]
        });

    let broker = mqtt_tests::test_mqtt_broker();
    let mut c8y_http_client = MockC8YHttpProxy::new();

    // Run the plugin's runtime logic in an async task
    tokio::spawn(async move {
        let _ = run(
            tedge_device_id.into(),
            broker.port,
            &mut c8y_http_client,
            &mockito::server_url(),
            ttd.path().to_path_buf(),
            ttd.path(),
        )
        .await;
    });

    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{child_device_id}").as_str())
        .await;

    // Fake config_snapshot executing status response from child device
    broker
        .publish(
            &format!("tedge/{child_device_id}/commands/res/config_snapshot"),
            &serde_json::to_string(&ChildDeviceResponsePayload {
                status: Some(OperationStatus::Failed),
                path: test_config_path.into(),
                config_type: config_type.into(),
                reason: Some("upload failed".into()),
            })
            .unwrap(),
        )
        .await?;

    // Assert the c8y_UploadConfigFile operation status mapping to FAILED(502)
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &[r#"502,c8y_UploadConfigFile,"upload failed""#],
    )
    .await;

    Ok(())
}

// Test invalid config_snapshot response from child is mapped to
// back-to-back EXECUTING and FAILED messages for c8y_UploadConfigFile operation
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn test_invalid_config_upload_response_child_device() -> anyhow::Result<()> {
    let tedge_device_id = "tedge-device";
    let child_device_id = "child-device";
    let tmp_dir = TempTedgeDir::new();
    tmp_dir.dir("c8y").file("c8y-configuration-plugin.toml");

    let broker = mqtt_tests::test_mqtt_broker();
    let mut c8y_http_client = MockC8YHttpProxy::new();

    // Run the plugin's runtime logic in an async task
    tokio::spawn(async move {
        let _ = run(
            tedge_device_id.into(),
            broker.port,
            &mut c8y_http_client,
            &mockito::server_url(),
            tmp_dir.path().to_path_buf(),
            tmp_dir.path(),
        )
        .await;
    });

    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{child_device_id}").as_str())
        .await;

    // Invalid config_snapshot response from child device
    broker
        .publish(
            &format!("tedge/{child_device_id}/commands/res/config_snapshot"),
            "invalid json",
        )
        .await?;

    // Assert the c8y_UploadConfigFile operation status mapping to FAILED(502)
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_UploadConfigFile", "502,c8y_UploadConfigFile"],
    )
    .await;

    Ok(())
}

// Test tedge config_snapshot command successful response mapping to SmartREST
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn test_handle_config_upload_successful_response_child_device() -> anyhow::Result<()> {
    let tedge_device_id = "tedge-device";
    let child_device_id = "child-device";
    let config_type = "config_type";
    let test_config_path = "/some/test/config";

    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = config_type }
            ]
        });

    let broker = mqtt_tests::test_mqtt_broker();

    let local_http_host = mockito::server_address().to_string();

    //Mock the config file upload to Cumulocity
    let mut c8y_http_client = MockC8YHttpProxy::new();
    c8y_http_client
        .expect_upload_config_file()
        .with(
            predicate::always(),
            predicate::eq(config_type),
            predicate::eq(Some(child_device_id.to_string())),
        )
        .return_once(|_path, _type, _child_id| Ok("http://server/config/file/url".to_string()));

    // Run the plugin's runtime logic in an async task
    tokio::spawn(async move {
        let _ = run(
            tedge_device_id.into(),
            broker.port,
            &mut c8y_http_client,
            &local_http_host,
            ttd.path().to_path_buf(),
            ttd.path(),
        )
        .await;
    });

    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{child_device_id}").as_str())
        .await;

    // Fake child device sending config_snapshot successful status TODO
    broker
        .publish(
            &format!("tedge/{child_device_id}/commands/res/config_snapshot"),
            &serde_json::to_string(&ChildDeviceResponsePayload {
                status: Some(OperationStatus::Successful),
                path: test_config_path.into(),
                config_type: config_type.into(),
                reason: None,
            })
            .unwrap(),
        )
        .await?;

    // Assert the c8y_UploadConfigFile operation status mapping to SUCCESSFUL(503)
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["503,c8y_UploadConfigFile,http://server/config/file/url"],
    )
    .await;

    Ok(())
}

// If the child device sends successful response without uploading the file,
// the c8y_UploadConfigFile operation should fail
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn test_child_config_upload_successful_response_mapped_to_failed_without_uploaded_file(
) -> anyhow::Result<()> {
    let tedge_device_id = "tedge-device";
    let child_device_id = "child-device";
    let config_type = "config_type";
    let test_config_path = "/some/test/config";
    let tmp_dir = TempTedgeDir::new();
    tmp_dir.dir("c8y").file("c8y-configuration-plugin.toml");

    let broker = mqtt_tests::test_mqtt_broker();

    let local_http_host = mockito::server_address().to_string();

    // Mock the config file upload to Cumulocity to fail with file not found
    let mut c8y_http_client = MockC8YHttpProxy::new();
    c8y_http_client
        .expect_upload_config_file()
        .with(
            predicate::always(),
            predicate::eq(config_type),
            predicate::eq(Some(child_device_id.to_string())),
        )
        .return_once(|_path, _type, _child_id| {
            Err(SMCumulocityMapperError::ExecuteFailed(
                "File not found".to_string(),
            ))
        });

    // Run the plugin's runtime logic in an async task
    tokio::spawn(async move {
        let _ = run(
            tedge_device_id.into(),
            broker.port,
            &mut c8y_http_client,
            &local_http_host,
            tmp_dir.path().to_path_buf(),
            tmp_dir.path(),
        )
        .await;
    });

    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{child_device_id}").as_str())
        .await;

    // Fake child device sending config_snapshot successful status TODO
    broker
        .publish(
            &format!("tedge/{child_device_id}/commands/res/config_snapshot"),
            &serde_json::to_string(&ChildDeviceResponsePayload {
                status: Some(OperationStatus::Successful),
                path: test_config_path.into(),
                config_type: config_type.into(),
                reason: None,
            })
            .unwrap(),
        )
        .await?;

    // Assert the c8y_UploadConfigFile operation status mapping to SUCCESSFUL(503)
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["502,c8y_UploadConfigFile"],
    )
    .await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn test_handle_config_update_request_tedge_device() -> anyhow::Result<()> {
    let tedge_device_id = "tedge-device";
    let test_config_type = "test-config";
    let c8y_config_plugin_type = "c8y-configuration-plugin";
    let tmp_dir = TempTedgeDir::new();

    let test_config_file = tmp_dir.file(test_config_type);
    let test_config_path = test_config_file.path().to_str().unwrap();

    tmp_dir
        .dir("c8y")
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = test_config_type }
            ]
        });

    let broker = mqtt_tests::test_mqtt_broker();
    let mut c8y_http_client = MockC8YHttpProxy::new();
    c8y_http_client
        .expect_url_is_in_my_tenant_domain()
        .with(predicate::always())
        .returning(|_path| false);

    // Mock download endpoint for the plugin to download config file update from the cloud
    let config_update_cloud_url_path = "/some/cloud/url";
    let _download_config_url_mock = mockito::mock("GET", config_update_cloud_url_path)
        .with_body_from_fn(|w| w.write_all(b"v2"))
        .with_status(200)
        .create();
    let local_http_host = mockito::server_url();
    let config_update_download_url = format!("{local_http_host}{config_update_cloud_url_path}");

    // Run the plugin's runtime logic in an async task
    tokio::spawn(async move {
        let _ = run(
            tedge_device_id.into(),
            broker.port,
            &mut c8y_http_client,
            local_http_host.as_str(),
            tmp_dir.path().to_path_buf(),
            tmp_dir.path(),
        )
        .await;
    });

    let mut messages = broker.messages_published_on("c8y/s/us").await;

    // Send a c8y_DownloadConfigFile request to the plugin
    broker
        .publish(
            "c8y/s/ds",
            format!("524,{tedge_device_id},{config_update_download_url},{test_config_type}")
                .as_str(),
        )
        .await?;

    // Assert the c8y_UploadConfigFile operation transitioning from EXECUTING(501) to SUCCESSFUL(503) with the uploaded config URL
    mqtt_tests::assert_received_all_expected(
        &mut messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_DownloadConfigFile", "503,c8y_DownloadConfigFile"],
    )
    .await;

    // Send a c8y_DownloadConfigFile request for `c8y-configuration-plugin` type to the plugin
    broker
        .publish(
            "c8y/s/ds",
            format!("524,{tedge_device_id},{config_update_download_url},{c8y_config_plugin_type}")
                .as_str(),
        )
        .await?;

    // Assert the c8y_DownloadConfigFile operation transitioning from EXECUTING(501) to SUCCESSFUL(503) with the uploaded config URL
    mqtt_tests::assert_received_all_expected(
        &mut messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_DownloadConfigFile", "503,c8y_DownloadConfigFile"],
    )
    .await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn test_handle_config_update_request_child_device() -> anyhow::Result<()> {
    let tedge_device_id = "tedge-device";
    let child_device_id = "child-device";
    let config_type = "file_a";
    let test_config_path = "/some/test/config";
    let tmp_dir = TempTedgeDir::new();
    tmp_dir
        .dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = "file_a" }
            ]
        });

    let broker = mqtt_tests::test_mqtt_broker();
    let local_http_host = mockito::server_address().to_string();
    let mut c8y_http_client = MockC8YHttpProxy::new();
    c8y_http_client
        .expect_url_is_in_my_tenant_domain()
        .with(predicate::always())
        .return_once(|_path| false);

    // Run the plugin's runtime logic in an async task
    tokio::spawn(async move {
        let _ = run(
            tedge_device_id.into(),
            broker.port,
            &mut c8y_http_client,
            local_http_host.as_str(),
            tmp_dir.path().to_path_buf(),
            tmp_dir.path(),
        )
        .await;
    });

    let mut tedge_command_messages = broker
        .messages_published_on(&format!(
            "tedge/{child_device_id}/commands/req/config_update"
        ))
        .await;

    // Mock download endpoint for the plugin to download config file update from the cloud
    let config_update_cloud_url_path = "/some/cloud/url";
    let _download_config_url_mock = mockito::mock("GET", config_update_cloud_url_path)
        .with_body_from_fn(|w| w.write_all(b"v2"))
        .with_status(200)
        .create();
    let local_http_host = mockito::server_url();
    let config_update_download_url = format!("{local_http_host}{config_update_cloud_url_path}");

    // Send a c8y_DownloadConfigFile request to the plugin
    broker
        .publish(
            "c8y/s/ds",
            format!("524,{child_device_id},{config_update_download_url},{config_type}").as_str(),
        )
        .await?;

    let expected_request = ChildDeviceRequestPayload {
        url: format!(
            "{local_http_host}/tedge/file-transfer/{child_device_id}/config_update/{config_type}"
        ),
        path: test_config_path.into(),
        config_type: Some(config_type.into()),
    };
    let expected_request = serde_json::to_string(&expected_request)?;

    // Assert the mapping from c8y_DownloadConfigFile request to tedge command
    mqtt_tests::assert_received_all_expected(
        &mut tedge_command_messages,
        TEST_TIMEOUT_MS,
        &[expected_request],
    )
    .await;

    Ok(())
}

// Validate c8y_DownloadConfigFile operation in cloud failing if the config URL is broken
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn test_c8y_config_download_child_device_fail_on_broken_url() -> anyhow::Result<()> {
    let tedge_device_id = "tedge-device";
    let child_device_id = "child-device";
    let config_type = "file_a";
    let test_config_path = "/some/test/config";
    let tmp_dir = TempTedgeDir::new();
    tmp_dir
        .dir("c8y")
        .dir(child_device_id)
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = "file_a" }
            ]
        });

    let broker = mqtt_tests::test_mqtt_broker();
    let local_http_host = mockito::server_address().to_string();
    let mut c8y_http_client = MockC8YHttpProxy::new();
    c8y_http_client
        .expect_url_is_in_my_tenant_domain()
        .with(predicate::always())
        .return_once(|_path| false);

    // Run the plugin's runtime logic in an async task
    tokio::spawn(async move {
        let _ = run(
            tedge_device_id.into(),
            broker.port,
            &mut c8y_http_client,
            local_http_host.as_str(),
            tmp_dir.path().to_path_buf(),
            tmp_dir.path(),
        )
        .await;
    });

    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{child_device_id}").as_str())
        .await;

    // Mock download endpoint for the plugin which returns bad response
    let config_update_download_url_path = "/some/cloud/url";
    let _download_config_url_mock = mockito::mock("GET", config_update_download_url_path)
        .with_status(404)
        .with_body("Broken URL")
        .create();
    let local_http_host = mockito::server_url();
    let config_update_download_url = format!("{local_http_host}{config_update_download_url_path}");

    // Send a c8y_DownloadConfigFile request to the plugin with broken URL
    broker
        .publish(
            "c8y/s/ds",
            format!("524,{child_device_id},{config_update_download_url},{config_type}").as_str(),
        )
        .await?;

    // Assert that the c8y_DownloadConfigFile operation is marked failed (SR 502)
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_DownloadConfigFile", "502,c8y_DownloadConfigFile"],
    )
    .await;

    Ok(())
}

// Test tedge config_update command successful response mapping to SmartREST
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial_test::serial]
async fn test_handle_config_update_successful_response_child_device() -> anyhow::Result<()> {
    let tedge_device_id = "tedge-device";
    let child_device_id = "child-device";
    let config_type = "config_type";
    let test_config_path = "/some/test/config";

    let ttd = TempTedgeDir::new();
    ttd.dir("c8y")
        .file("c8y-configuration-plugin.toml")
        .with_toml_content(toml::toml! {
            files = [
                { path = test_config_path, type = config_type }
            ]
        });

    let broker = mqtt_tests::test_mqtt_broker();
    let local_http_host = mockito::server_address().to_string();
    let mut c8y_http_client = MockC8YHttpProxy::new();

    // Run the plugin's runtime logic in an async task
    tokio::spawn(async move {
        let _ = run(
            tedge_device_id.into(),
            broker.port,
            &mut c8y_http_client,
            &local_http_host,
            ttd.path().to_path_buf(),
            ttd.path(),
        )
        .await;
    });

    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{child_device_id}").as_str())
        .await;

    // Fake child device sending config_update successful status
    broker
        .publish(
            &format!("tedge/{child_device_id}/commands/res/config_update"),
            &serde_json::to_string(&ChildDeviceResponsePayload {
                status: Some(OperationStatus::Successful),
                path: test_config_path.into(),
                config_type: config_type.into(),
                reason: None,
            })
            .unwrap(),
        )
        .await?;

    // Assert the c8y_DownloadConfigFile operation status mapping to SUCCESSFUL(503)
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["503,c8y_DownloadConfigFile"],
    )
    .await;

    Ok(())
}

// Test invalid config_update response from child is mapped to
// back-to-back EXECUTING and FAILED messages for c8y_DownloadConfigFile operation
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn test_invalid_config_update_response_child_device() -> anyhow::Result<()> {
    let tedge_device_id = "tedge-device";
    let child_device_id = "child-device";
    let tmp_dir = TempTedgeDir::new();
    tmp_dir.dir("c8y").file("c8y-configuration-plugin.toml");

    let broker = mqtt_tests::test_mqtt_broker();
    let mut c8y_http_client = MockC8YHttpProxy::new();

    // Run the plugin's runtime logic in an async task
    tokio::spawn(async move {
        let _ = run(
            tedge_device_id.into(),
            broker.port,
            &mut c8y_http_client,
            &mockito::server_url(),
            tmp_dir.path().to_path_buf(),
            tmp_dir.path(),
        )
        .await;
    });

    let mut smartrest_messages = broker
        .messages_published_on(format!("c8y/s/us/{child_device_id}").as_str())
        .await;

    // Invalid config_snapshot response from child device
    broker
        .publish(
            &format!("tedge/{child_device_id}/commands/res/config_update"),
            "invalid json",
        )
        .await?;

    // Assert the c8y_UploadConfigFile operation status mapping to FAILED(502)
    mqtt_tests::assert_received_all_expected(
        &mut smartrest_messages,
        TEST_TIMEOUT_MS,
        &["501,c8y_DownloadConfigFile", "502,c8y_DownloadConfigFile"],
    )
    .await;

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[serial_test::serial]
async fn test_handle_multiline_config_upload_requests() -> anyhow::Result<()> {
    let tedge_device_id = "tedge-device";
    let test_config_type = "c8y-configuration-plugin";
    let ttd = TempTedgeDir::new();
    ttd.dir("c8y").file("c8y-configuration-plugin.toml");

    let broker = mqtt_tests::test_mqtt_broker();

    let mut messages = broker.messages_published_on("c8y/s/us").await;

    let mut http_client = MockC8YHttpProxy::new();
    http_client
        .expect_upload_config_file()
        .with(
            predicate::always(),
            predicate::eq(test_config_type),
            predicate::eq(None),
        )
        .returning(|_path, _type, _child_id| Ok("http://server/some/test/config/url".to_string()));

    // Run the plugin's runtime logic in an async task
    tokio::spawn(async move {
        let _ = run(
            tedge_device_id.into(),
            broker.port,
            &mut http_client,
            "localhost",
            ttd.path().to_path_buf(),
            ttd.path(),
        )
        .await;
    });

    // Assert supported config types message(119) on plugin startup
    mqtt_tests::assert_received_all_expected(
        &mut messages,
        TEST_TIMEOUT_MS,
        &[format!("119,{test_config_type}")],
    )
    .await;

    // Send a config upload request to the plugin
    broker
            .publish(
                "c8y/s/ds",
                format!("526,{tedge_device_id},{test_config_type}\n526,{tedge_device_id},{test_config_type}").as_str(),
            )
            .await?;

    // Assert the c8y_UploadConfigFile operation transitioning from EXECUTING(501) to SUCCESSFUL(503) with the uploaded config URL
    mqtt_tests::assert_received_all_expected(
        &mut messages,
        TEST_TIMEOUT_MS,
        &[
            "501,c8y_UploadConfigFile",
            "503,c8y_UploadConfigFile,http://server/some/test/config/url",
            "501,c8y_UploadConfigFile",
            "503,c8y_UploadConfigFile,http://server/some/test/config/url",
        ],
    )
    .await;

    Ok(())
}
