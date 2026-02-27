use assert_matches::assert_matches;
use camino::Utf8Path;
use serde_json::json;
use serde_json::Value;
use std::fs::read_to_string;
use std::sync::Arc;
use std::time::Duration;
use tedge_actors::test_helpers::FakeServerBox;
use tedge_actors::test_helpers::FakeServerBoxBuilder;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::test_helpers::TimedMessageBox;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::ClientMessageBox;
use tedge_actors::MessageReceiver;
use tedge_actors::NoMessage;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::OperationStepRequest;
use tedge_api::workflow::OperationStepResponse;
use tedge_downloader_ext::DownloadResponse;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_mqtt_ext::TopicFilter;
use tedge_test_utils::fs::TempTedgeDir;
use tedge_uploader_ext::UploadResponse;
use toml::from_str;
use toml::Table;

use crate::actor::ConfigDownloadRequest;
use crate::actor::ConfigDownloadResult;
use crate::actor::ConfigUploadRequest;
use crate::actor::ConfigUploadResult;
use crate::ConfigManagerBuilder;
use crate::ConfigManagerConfig;

const TEST_TIMEOUT_MS: Duration = Duration::from_secs(3);

type MqttMessageBox = TimedMessageBox<SimpleMessageBox<MqttMessage, MqttMessage>>;
type DownloaderMessageBox =
    TimedMessageBox<FakeServerBox<ConfigDownloadRequest, ConfigDownloadResult>>;
type UploaderMessageBox = TimedMessageBox<FakeServerBox<ConfigUploadRequest, ConfigUploadResult>>;
type StepMessageBox = ClientMessageBox<OperationStepRequest, OperationStepResponse>;

struct TestHandle {
    pub mqtt: MqttMessageBox,
    pub _fs: SimpleMessageBox<NoMessage, FsWatchEvent>,
    pub downloader: DownloaderMessageBox,
    pub uploader: UploaderMessageBox,
    pub steps: StepMessageBox,
}

fn prepare() -> Result<TempTedgeDir, anyhow::Error> {
    let tempdir = TempTedgeDir::new();
    let tempdir_path = tempdir
        .path()
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("temp dir not created"))?;

    // Test files
    tempdir.file("file_a");
    tempdir.file("file_b").with_raw_content("Some content");
    tempdir.file("file_c");
    tempdir.file("file_d");

    tempdir
        .file("tedge-configuration-plugin.toml")
        .with_raw_content(&format!(
            r#"files = [
            {{ path = "{tempdir_path}/file_a", type = "type_one" }},
            {{ path = "{tempdir_path}/file_b", type = "type_two" }},
            {{ path = "{tempdir_path}/file_c", type = "type_three" }},
            {{ path = "{tempdir_path}/file_d", type = "type_four" }},
        ]"#
        ));

    let plugin_dir = tempdir.dir("config-plugins");

    // Create a mock `file` plugin script
    let plugin_script = include_str!("../tests/data/file");

    let plugin_path = plugin_dir.file("file").with_raw_content(plugin_script);

    // Make the plugin executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(plugin_path.path())?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(plugin_path.path(), perms)?;
    }

    Ok(tempdir)
}

#[allow(clippy::type_complexity)]
async fn new_config_manager_builder(
    temp_dir: &TempTedgeDir,
) -> (
    ConfigManagerBuilder,
    MqttMessageBox,
    SimpleMessageBox<NoMessage, FsWatchEvent>,
    DownloaderMessageBox,
    UploaderMessageBox,
) {
    let config = ConfigManagerConfig {
        config_dir: temp_dir.to_path_buf(),
        plugin_dirs: vec![temp_dir
            .to_path_buf()
            .join("config-plugins")
            .try_into()
            .unwrap()],
        plugin_config_dir: temp_dir.to_path_buf(),
        plugin_config_path: temp_dir.path().join("tedge-configuration-plugin.toml"),
        config_reload_topics: [
            "te/device/main///cmd/config_snapshot",
            "te/device/main///cmd/config_update",
        ]
        .into_iter()
        .map(Topic::new_unchecked)
        .collect(),
        tmp_path: Arc::from(Utf8Path::from_path(&std::env::temp_dir()).unwrap()),
        ops_dir: temp_dir.dir("operations").utf8_path_buf(),
        mqtt_schema: MqttSchema::new(),
        config_snapshot_topic: TopicFilter::new_unchecked("te/device/main///cmd/config_snapshot/+"),
        config_update_topic: TopicFilter::new_unchecked("te/device/main///cmd/config_update/+"),
        tedge_http_host: "127.0.0.1:3000".into(),
        config_update_enabled: true,
        sudo_enabled: false,
    };

    let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 5);
    let mut fs_watcher_builder: SimpleMessageBoxBuilder<NoMessage, FsWatchEvent> =
        SimpleMessageBoxBuilder::new("FS", 5);
    let mut downloader_builder: FakeServerBoxBuilder<ConfigDownloadRequest, ConfigDownloadResult> =
        FakeServerBoxBuilder::default();
    let mut uploader_builder: FakeServerBoxBuilder<ConfigUploadRequest, ConfigUploadResult> =
        FakeServerBoxBuilder::default();

    let mut config_builder = ConfigManagerBuilder::try_new(
        config,
        &mut fs_watcher_builder,
        &mut downloader_builder,
        &mut uploader_builder,
    )
    .await
    .unwrap();

    config_builder.connect_mqtt(&mut mqtt_builder);

    (
        config_builder,
        mqtt_builder.build().with_timeout(TEST_TIMEOUT_MS),
        fs_watcher_builder.build(),
        downloader_builder.build().with_timeout(TEST_TIMEOUT_MS),
        uploader_builder.build().with_timeout(TEST_TIMEOUT_MS),
    )
}

async fn spawn_config_manager_actor(temp_dir: &TempTedgeDir) -> TestHandle {
    let (mut actor_builder, mqtt, _fs, downloader, uploader) =
        new_config_manager_builder(temp_dir).await;
    let steps = ClientMessageBox::new(&mut actor_builder);
    let actor = actor_builder.build();
    tokio::spawn(async move { actor.run().await });
    TestHandle {
        mqtt,
        _fs,
        downloader,
        uploader,
        steps,
    }
}

#[tokio::test]
async fn default_plugin_config() {
    let tempdir = TempTedgeDir::new();
    let _test_handle = spawn_config_manager_actor(&tempdir).await;
    let plugin_config_content =
        read_to_string(tempdir.path().join("tedge-configuration-plugin.toml")).unwrap();
    let plugin_config_toml: Table = from_str(&plugin_config_content).unwrap();

    let tedge_config_path = format!("{}/tedge.toml", tempdir.path().to_string_lossy());
    let tedge_log_plugin_config_path = format!(
        "{}/plugins/tedge-log-plugin.toml",
        tempdir.path().to_string_lossy()
    );
    let expected_config = toml::toml! {
        [[files]]
        path = tedge_config_path
        type = "tedge.toml"

        [[files]]
        path = tedge_log_plugin_config_path
        type = "tedge-log-plugin"
        user = "tedge"
        group = "tedge"
        mode = 0o644
    };

    assert_eq!(plugin_config_toml, expected_config);
}

#[tokio::test]
async fn config_manager_reloads_config_types() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let TestHandle { mut mqtt, .. } = spawn_config_manager_actor(&tempdir).await;

    let config_snapshot_reload_topic = Topic::new_unchecked("te/device/main///cmd/config_snapshot");
    let config_update_reload_topic = Topic::new_unchecked("te/device/main///cmd/config_update");

    assert_eq!(
        mqtt.recv().await,
        Some(
            MqttMessage::new(
                &config_snapshot_reload_topic,
                r#"{"types":["tedge-configuration-plugin","type_four","type_one","type_three","type_two"]}"#
            )
            .with_retain()
        )
    );

    assert_eq!(
        mqtt.recv().await,
        Some(
            MqttMessage::new(
                &config_update_reload_topic,
                r#"{"types":["tedge-configuration-plugin","type_four","type_one","type_three","type_two"]}"#
            )
            .with_retain()
        )
    );

    Ok(())
}

#[tokio::test]
async fn config_manager_uploads_snapshot() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let TestHandle {
        mut mqtt,
        mut uploader,
        ..
    } = spawn_config_manager_actor(&tempdir).await;

    let config_topic = Topic::new_unchecked("te/device/main///cmd/config_snapshot/1234");

    // Let's ignore the reload messages sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received
    let snapshot_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/te/v1/files/main/config-snapshot/type_two-1234",
            "type": "type_two"
        }"#;

    mqtt.send(MqttMessage::new(&config_topic, snapshot_request).with_retain())
        .await?;

    // The config manager notifies that the request has been received and is processed
    let executing_message = mqtt.recv().await;
    assert_eq!(
            executing_message,
            Some(MqttMessage::new(
                &config_topic,
                r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/config-snapshot/type_two-1234","type":"type_two"}"#
            ).with_retain())
        );

    // This message being published over MQTT is also received by the config-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Assert config upload request.
    let (topic, upload_request) = uploader.recv().await.unwrap();

    assert_eq!(Topic::new_unchecked(&topic), config_topic);

    assert_eq!(
        upload_request.url,
        "http://127.0.0.1:3000/te/v1/files/main/config-snapshot/type_two-1234"
    );
    let upload_path = upload_request.file_path.to_string();
    assert!(upload_path.contains("type_two"));

    assert_eq!(upload_request.auth, None);

    // Simulate upload file completion
    let upload_response = UploadResponse::new(&upload_request.url, upload_request.file_path);
    uploader.send((topic, Ok(upload_response))).await?;

    // Finally, the config manager notifies that request was successfully processed
    assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(
                &config_topic,
                format!(r#"{{"status":"successful","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/config-snapshot/type_two-1234","type":"type_two","path":{:?}}}"#, upload_path)
            ).with_retain())
        );

    Ok(())
}

#[tokio::test]
async fn config_manager_creates_tedge_url_for_snapshot_request() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let TestHandle {
        mut mqtt,
        mut uploader,
        ..
    } = spawn_config_manager_actor(&tempdir).await;

    let config_topic = Topic::new_unchecked("te/device/main///cmd/config_snapshot/1234");

    // Let's ignore the reload messages sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received
    let snapshot_request = r#"
        {
            "status": "init",
            "type": "type_two"
        }"#;

    mqtt.send(MqttMessage::new(&config_topic, snapshot_request).with_retain())
        .await?;

    // The config manager notifies that the request has been received and is processed
    let executing_message = mqtt.recv().await;
    assert_eq!(
        executing_message,
        Some(
            MqttMessage::new(&config_topic, r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/config_snapshot/type_two-1234","type":"type_two"}"#)
                .with_retain()
        )
    );

    // This message being published over MQTT is also received by the config-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Assert config upload request.
    let (topic, upload_request) = uploader.recv().await.unwrap();

    assert_eq!(Topic::new_unchecked(&topic), config_topic);

    assert_eq!(
        upload_request.url,
        "http://127.0.0.1:3000/te/v1/files/main/config_snapshot/type_two-1234"
    );
    let upload_path = upload_request.file_path.to_string();
    assert!(upload_path.contains("type_two"));

    assert_eq!(upload_request.auth, None);

    // Simulate upload file completion
    let upload_response = UploadResponse::new(&upload_request.url, upload_request.file_path);
    uploader.send((topic, Ok(upload_response))).await?;

    // Finally, the config manager notifies that request was successfully processed
    assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(
                &config_topic,
                format!(r#"{{"status":"successful","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/config_snapshot/type_two-1234","type":"type_two","path":{:?}}}"#, upload_path)
            ).with_retain())
        );

    Ok(())
}

#[tokio::test]
async fn config_manager_download_update() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let TestHandle {
        mut mqtt,
        mut downloader,
        ..
    } = spawn_config_manager_actor(&tempdir).await;

    let config_topic = Topic::new_unchecked("te/device/main///cmd/config_update/1234");

    // Let's ignore the reload messages sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received
    let snapshot_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/te/v1/files/main/config_update/type_two-1234",
            "remoteUrl": "http://www.remote.url",
            "serverUrl": "http://www.remote.url",
            "type": "type_two"
        }"#;

    mqtt.send(MqttMessage::new(&config_topic, snapshot_request).with_retain())
        .await?;

    // The config manager notifies that the request has been received and is processed
    let executing_message = mqtt.recv().await;
    assert_eq!(
        executing_message,
            Some(MqttMessage::new(
                &config_topic,
                r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/config_update/type_two-1234","remoteUrl":"http://www.remote.url","serverUrl":"http://www.remote.url","type":"type_two"}"#
            ).with_retain())
        );

    // This message being published over MQTT is also received by the config-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Assert config download request.
    let (topic, download_request) = downloader.recv().await.unwrap();

    assert_eq!(Topic::new_unchecked(&topic), config_topic);

    assert_eq!(
        download_request.url,
        "http://127.0.0.1:3000/te/v1/files/main/config_update/type_two-1234"
    );
    assert_eq!(
        download_request.file_path,
        std::env::temp_dir().join("type_two")
    );

    assert!(download_request.headers.is_empty());
    assert!(download_request
        .file_path
        .to_string_lossy()
        .contains("type_two"));

    // Simulate downloading a file is completed.
    std::fs::File::create(&download_request.file_path).unwrap();
    let download_response =
        DownloadResponse::new(&download_request.url, &download_request.file_path);
    downloader.send((topic, Ok(download_response))).await?;

    // Finally, the config manager notifies that request was successfully processed
    assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(
                &config_topic,
                r#"{"status":"successful","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/config_update/type_two-1234","remoteUrl":"http://www.remote.url","serverUrl":"http://www.remote.url","type":"type_two"}"#
            ).with_retain())
        );

    Ok(())
}

#[tokio::test]
async fn request_config_snapshot_that_does_not_exist() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let TestHandle { mut mqtt, .. } = spawn_config_manager_actor(&tempdir).await;

    let config_topic = Topic::new_unchecked("te/device/main///cmd/config_snapshot/1234");

    // Let's ignore the init message sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received
    let snapshot_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/te/v1/files/main/config-snapshot/type_five-1234",
            "type": "type_five"
        }"#;

    mqtt.send(MqttMessage::new(&config_topic, snapshot_request).with_retain())
        .await?;

    let executing_message = mqtt.recv().await;
    // The config manager notifies that the request has been received and is processed
    assert_eq!(
        executing_message,
        Some(MqttMessage::new(
            &config_topic,
            r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/config-snapshot/type_five-1234","type":"type_five"}"#
        ).with_retain())
    );

    // This message being published over MQTT is also received by the config-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Finally, the config manager notifies that given config type does not exists
    assert_eq!(
        mqtt.recv().await,
        Some(MqttMessage::new(
            &config_topic,
            r#"{"status":"failed","reason":"Config plugin 'file' error: Command execution failed: Unknown config type: type_five\n","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/config-snapshot/type_five-1234","type":"type_five"}"#
        ).with_retain())
    );

    Ok(())
}

#[tokio::test]
async fn ignore_topic_for_another_device() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let TestHandle { mut mqtt, .. } = spawn_config_manager_actor(&tempdir).await;

    // Check for child device topic
    let another_device_topic = Topic::new_unchecked("te/device/child01///cmd/config-snapshot/1234");

    // Let's ignore the init message sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received
    let snapshot_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/te/v1/files/child01/config-snapshot/type_two-1234",
            "type": "type_two"
        }"#;

    mqtt.send(MqttMessage::new(&another_device_topic, snapshot_request).with_retain())
        .await?;

    // The config manager does not proceed to "executing" state
    assert!(mqtt.recv().await.is_none());

    Ok(())
}

#[tokio::test]
async fn send_incorrect_payload() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let TestHandle { mut mqtt, .. } = spawn_config_manager_actor(&tempdir).await;

    let config_topic = Topic::new_unchecked("te/device/main///cmd/config_snapshot/1234");

    // Let's ignore the init message sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received with kind instead of type
    let snapshot_request = r#"
        {
            "status": "init",
            "tedgeurl": "http://127.0.0.1:3000/te/v1/files/main/config-snapshot/type_two-1234",
            "kind": "type_two"
        }"#;

    mqtt.send(MqttMessage::new(&config_topic, snapshot_request).with_retain())
        .await?;

    // The config manager does not proceed to "executing" state
    assert!(mqtt.recv().await.is_none());

    Ok(())
}

#[tokio::test]
async fn receive_executing_snapshot_request_without_tedge_url() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let TestHandle {
        mut mqtt,
        mut uploader,
        ..
    } = spawn_config_manager_actor(&tempdir).await;

    let config_topic = Topic::new_unchecked("te/device/main///cmd/config_snapshot/1234");

    // Let's ignore the reload messages sent on start
    mqtt.skip(2).await;

    // Received executing snapshot request
    let executing_request = r#"
        {
            "status": "executing",
            "type": "type_two"
        }"#;

    mqtt.send(MqttMessage::new(&config_topic, executing_request).with_retain())
        .await?;

    // Assert config upload request.
    let (topic, upload_request) = uploader.recv().await.unwrap();

    assert_eq!(Topic::new_unchecked(&topic), config_topic);

    assert_eq!(
        upload_request.url,
        "http://127.0.0.1:3000/te/v1/files/main/config_snapshot/type_two-1234"
    );
    let upload_path = upload_request.file_path.to_string();
    assert!(upload_path.contains("type_two"));

    assert_eq!(upload_request.auth, None);

    // Simulate upload file completion
    let upload_response = UploadResponse::new(&upload_request.url, upload_request.file_path);
    uploader.send((topic, Ok(upload_response))).await?;

    // Finally, the config manager notifies that request was successfully processed
    assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(
                &config_topic,
                format!(r#"{{"status":"successful","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/config_snapshot/type_two-1234","type":"type_two","path":{:?}}}"#, upload_path)
            ).with_retain())
        );

    Ok(())
}

/// Check that requests are processed concurrently by publishing many requests at once and verifying
/// that download/upload requests for all of them are also sent immediately.
#[tokio::test]
async fn config_manager_processes_concurrently() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let TestHandle {
        mut mqtt,
        mut downloader,
        mut uploader,
        ..
    } = spawn_config_manager_actor(&tempdir).await;

    let num_requests = 5;

    // Let's ignore the reload messages sent on start
    mqtt.skip(2).await;

    let snapshot_topic = Topic::new_unchecked("te/device/main///cmd/config_snapshot/1234");

    let snapshot_request = r#"
        {
            "status": "executing",
            "tedgeUrl": "http://127.0.0.1:3000/te/v1/files/main/config-snapshot/type_two-1234",
            "type": "type_two"
        }"#;

    let update_topic = Topic::new_unchecked("te/device/main///cmd/config_update/1234");

    let update_request = r#"
        {
            "status": "executing",
            "tedgeUrl": "http://127.0.0.1:3000/te/v1/files/main/config_update/type_two-1234",
            "remoteUrl": "http://www.remote.url",
            "serverUrl": "http://www.remote.url",
            "type": "type_two"
        }"#;

    for _ in 0..num_requests {
        mqtt.send(MqttMessage::new(&snapshot_topic, snapshot_request).with_retain())
            .await?;

        mqtt.send(MqttMessage::new(&update_topic, update_request).with_retain())
            .await?;
    }

    // Assert we started downloads/upload on all requests.
    for _ in 0..num_requests {
        uploader
            .recv()
            .await
            .expect("upload request should've been sent by config manager for config snapshot");
        downloader
            .recv()
            .await
            .expect("download request should've been sent by config manager for config update");
    }

    Ok(())
}

#[tokio::test]
async fn execute_config_set_operation_step() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let mut handle = spawn_config_manager_actor(&tempdir).await;

    // Let's ignore the reload messages sent on start
    handle.mqtt.skip(2).await;

    let downloaded_path = tempdir
        .file("downloaded_file")
        .with_raw_content("Some content");

    let work_dir = tempdir.dir("workdir");

    let command_state = GenericCommandState::new(
        Topic::new_unchecked("te/device/main///cmd/config_update/1234"),
        "set".to_string(),
        json!({
            "type": "type_two",
            "setFrom": downloaded_path.path(),
            "workDir": work_dir.utf8_path(),
        }),
    );

    let step_request = OperationStepRequest {
        command_step: "set".to_string(),
        command_state,
    };

    let response = handle.steps.await_response(step_request).await?;
    assert_eq!(response, Ok(Value::Null));

    Ok(())
}

#[tokio::test]
async fn execute_config_set_operation_step_invalid_step() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let mut handle = spawn_config_manager_actor(&tempdir).await;

    // Let's ignore the reload messages sent on start
    handle.mqtt.skip(2).await;

    let downloaded_path = tempdir
        .file("downloaded_file")
        .with_raw_content("Some content");

    let command_state = GenericCommandState::new(
        Topic::new_unchecked("te/device/main///cmd/config_update/1234"),
        "set".to_string(),
        json!({
            "type": "type_two",
            "setFrom": downloaded_path.path(),
        }),
    );

    let step_request = OperationStepRequest {
        command_step: "unknown".to_string(),
        command_state,
    };

    let response = handle.steps.await_response(step_request).await?;
    assert_matches!(response, Err(err) if err.contains("Invalid operation step: unknown"));

    Ok(())
}

#[tokio::test]
async fn execute_config_set_operation_step_missing_type() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let mut handle = spawn_config_manager_actor(&tempdir).await;

    // Let's ignore the reload messages sent on start
    handle.mqtt.skip(2).await;

    let downloaded_path = tempdir
        .file("downloaded_file")
        .with_raw_content("Some content");

    let command_state = GenericCommandState::new(
        Topic::new_unchecked("te/device/main///cmd/config_update/1234"),
        "set".to_string(),
        json!({
            "setFrom": downloaded_path.path(),
        }),
    );

    let step_request = OperationStepRequest {
        command_step: "set".to_string(),
        command_state,
    };

    let response = handle.steps.await_response(step_request).await?;
    assert_matches!(response, Err(err) if err.contains("Missing key: type"));

    Ok(())
}

#[tokio::test]
async fn execute_config_set_operation_step_missing_downloaded_path() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let mut handle = spawn_config_manager_actor(&tempdir).await;

    // Let's ignore the reload messages sent on start
    handle.mqtt.skip(2).await;

    let command_state = GenericCommandState::new(
        Topic::new_unchecked("te/device/main///cmd/config_update/1234"),
        "set".to_string(),
        json!({
            "type": "type_two",
        }),
    );

    let step_request = OperationStepRequest {
        command_step: "set".to_string(),
        command_state,
    };

    let response = handle.steps.await_response(step_request).await?;
    assert_matches!(response, Err(err) if err.contains("Missing key: setFrom"));

    Ok(())
}

#[tokio::test]
async fn execute_config_set_operation_step_file_not_found() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let mut handle = spawn_config_manager_actor(&tempdir).await;

    // Let's ignore the reload messages sent on start
    handle.mqtt.skip(2).await;

    let missing_path = tempdir.path().join("missing_file");
    let work_dir = tempdir.dir("workdir");

    let command_state = GenericCommandState::new(
        Topic::new_unchecked("te/device/main///cmd/config_update/1234"),
        "set".to_string(),
        json!({
            "type": "type_two",
            "setFrom": missing_path,
            "workDir": work_dir.utf8_path(),
        }),
    );

    let step_request = OperationStepRequest {
        command_step: "set".to_string(),
        command_state,
    };

    let response = handle.steps.await_response(step_request).await?;
    assert_matches!(
        response,
        Err(err) if err.contains("not found")
    );

    Ok(())
}
