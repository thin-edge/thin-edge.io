use crate::actor::OperationTimeout;
use crate::actor::OperationTimer;
use crate::ConfigManagerBuilder;
use crate::ConfigManagerConfig;
use c8y_api::smartrest::topic::C8yTopic;
use c8y_http_proxy::messages::C8YRestRequest;
use c8y_http_proxy::messages::C8YRestResponse;
use c8y_http_proxy::messages::C8YRestResult;
use c8y_http_proxy::messages::DownloadFile;
use c8y_http_proxy::messages::UploadConfigFile;
use std::time::Duration;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynError;
use tedge_actors::ReceiveMessages;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_test_utils::fs::TempTedgeDir;
use tedge_utils::file::PermissionEntry;
use tokio::time::timeout;

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

    // Assert supported config types MQTT message
    let expected_message = MqttMessage::new(
        &C8yTopic::SmartRestResponse.to_topic().unwrap(),
        format!("119,c8y-configuration-plugin,{test_config_type}"),
    );
    let next_message = timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await;
    assert_eq!(next_message, Ok(Some(expected_message)));

    // Assert supported config types MQTT message
    let expected_message =
        MqttMessage::new(&C8yTopic::SmartRestResponse.to_topic().unwrap(), "500");
    let next_message = timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await;
    assert_eq!(next_message, Ok(Some(expected_message)));

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

    let (mut mqtt_message_box, mut c8y_proxy_message_box, mut _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    let _config_types_msg = mqtt_message_box.recv().await.unwrap();
    let _pending_ops_msg = mqtt_message_box.recv().await.unwrap();

    let c8y_config_upload_msg = MqttMessage::new(
        &Topic::new_unchecked("c8y/s/ds"),
        format!("526,{device_id},{test_config_type}").as_str(),
    );
    mqtt_message_box.send(c8y_config_upload_msg).await?;

    // Assert EXECUTING SmartREST MQTT message
    let expected_message = MqttMessage::new(
        &C8yTopic::SmartRestResponse.to_topic().unwrap(),
        "501,c8y_UploadConfigFile\n",
    );
    let next_message = timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await;
    assert_eq!(next_message, Ok(Some(expected_message)));

    // Assert config file upload HTTP request
    let expected_message = UploadConfigFile {
        config_path: test_config_path.into(),
        config_type: test_config_type.to_string(),
        child_device_id: None,
    }
    .into();
    let next_message = timeout(TEST_TIMEOUT, c8y_proxy_message_box.recv()).await;
    assert_eq!(next_message, Ok(Some(expected_message)));

    // Provide mock config file upload HTTP response to continue
    c8y_proxy_message_box
        .send(Ok(C8YRestResponse::EventId("test-url".to_string())))
        .await?;

    // Assert SUCCESSFUL SmartREST MQTT message
    let expected_message = MqttMessage::new(
        &C8yTopic::SmartRestResponse.to_topic().unwrap(),
        "503,c8y_UploadConfigFile,test-url\n",
    );
    let next_message = timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await;
    assert_eq!(next_message, Ok(Some(expected_message)));

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

    let (mut mqtt_message_box, mut c8y_proxy_message_box, mut _timer_message_box) =
        spawn_config_manager(&device_id, &ttd).await?;

    let _config_types_msg = mqtt_message_box
        .recv()
        .await
        .expect("Supported config types message");
    let _pending_ops_msg = mqtt_message_box
        .recv()
        .await
        .expect("Get pending operations message");

    let download_url = "http://test.domain.com";
    let c8y_config_upload_msg = MqttMessage::new(
        &C8yTopic::SmartRestRequest.to_topic().unwrap(),
        format!("524,{device_id},{download_url},{test_config_type}"),
    );
    mqtt_message_box.send(c8y_config_upload_msg).await?;

    // Assert EXECUTING SmartREST MQTT message
    let expected_message = MqttMessage::new(
        &C8yTopic::SmartRestResponse.to_topic().unwrap(),
        "501,c8y_DownloadConfigFile\n",
    );
    let next_message = timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await;
    assert_eq!(next_message, Ok(Some(expected_message)));

    // Assert config file upload HTTP request
    let expected_message = DownloadFile {
        download_url: download_url.into(),
        file_path: test_config_path.into(),
        file_permissions: PermissionEntry::default(),
    }
    .into();
    let next_message = timeout(TEST_TIMEOUT, c8y_proxy_message_box.recv()).await;
    assert_eq!(next_message, Ok(Some(expected_message)));

    // Provide mock config file upload HTTP response to continue
    c8y_proxy_message_box
        .send(Ok(C8YRestResponse::Unit(())))
        .await?;

    // Assert SUCCESSFUL SmartREST MQTT message
    let expected_message = MqttMessage::new(
        &C8yTopic::SmartRestResponse.to_topic().unwrap(),
        "503,c8y_DownloadConfigFile,\n",
    );
    let next_message = timeout(TEST_TIMEOUT, mqtt_message_box.recv()).await;
    assert_eq!(next_message, Ok(Some(expected_message)));

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
    let c8y_host = "test.c8y.io";
    let mqtt_port = 1234;
    let tedge_http_port = 9876;

    let config = ConfigManagerConfig::new(
        tedge_temp_dir.to_path_buf(),
        tedge_temp_dir.to_path_buf(),
        device_id.to_string(),
        tedge_host.to_string().try_into().unwrap(),
        mqtt_port,
        c8y_host.to_string().try_into().unwrap(),
        tedge_host.to_string().try_into().unwrap(),
        tedge_http_port,
    );

    let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 5);
    let mut c8y_proxy_builder: SimpleMessageBoxBuilder<C8YRestRequest, C8YRestResult> =
        SimpleMessageBoxBuilder::new("C8Y", 1);
    let mut timer_builder: SimpleMessageBoxBuilder<OperationTimer, OperationTimeout> =
        SimpleMessageBoxBuilder::new("Timer", 5);

    let mut config_manager_builder = ConfigManagerBuilder::new(config);

    config_manager_builder.with_c8y_http_proxy(&mut c8y_proxy_builder)?;
    config_manager_builder.with_mqtt_connection(&mut mqtt_builder)?;
    config_manager_builder.with_timer(&mut timer_builder)?;

    let mqtt_message_box = mqtt_builder.build();
    let c8y_proxy_message_box = c8y_proxy_builder.build();
    let timer_message_box = timer_builder.build();

    let (actor, message_box) = config_manager_builder.build();
    let _join_handle = tokio::spawn(async move { actor.run(message_box).await });

    Ok((mqtt_message_box, c8y_proxy_message_box, timer_message_box))
}
