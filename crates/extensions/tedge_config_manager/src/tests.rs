use camino::Utf8Path;
use std::fs::read_to_string;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tedge_actors::test_helpers::FakeServerBox;
use tedge_actors::test_helpers::FakeServerBoxBuilder;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::test_helpers::TimedMessageBox;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::MessageReceiver;
use tedge_actors::NoMessage;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::mqtt_topics::MqttSchema;
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
use crate::TedgeWriteStatus;

const TEST_TIMEOUT_MS: Duration = Duration::from_secs(5);

type MqttMessageBox = TimedMessageBox<SimpleMessageBox<MqttMessage, MqttMessage>>;
type DownloaderMessageBox =
    TimedMessageBox<FakeServerBox<ConfigDownloadRequest, ConfigDownloadResult>>;
type UploaderMessageBox = TimedMessageBox<FakeServerBox<ConfigUploadRequest, ConfigUploadResult>>;

fn prepare() -> Result<TempTedgeDir, anyhow::Error> {
    let tempdir = TempTedgeDir::new();
    let tempdir_path = tempdir
        .path()
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("temp dir not created"))?;

    std::fs::File::create(format!("{tempdir_path}/file_a"))?;
    tempdir.file("file_b").with_raw_content("Some content");
    std::fs::File::create(format!("{tempdir_path}/file_c"))?;
    std::fs::File::create(format!("{tempdir_path}/file_d"))?;

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

    Ok(tempdir)
}

#[allow(clippy::type_complexity)]
async fn new_config_manager_builder(
    temp_dir: &Path,
) -> (
    ConfigManagerBuilder,
    MqttMessageBox,
    SimpleMessageBox<NoMessage, FsWatchEvent>,
    DownloaderMessageBox,
    UploaderMessageBox,
) {
    let config = ConfigManagerConfig {
        config_dir: temp_dir.to_path_buf(),
        plugin_config_dir: temp_dir.to_path_buf(),
        plugin_config_path: temp_dir.join("tedge-configuration-plugin.toml"),
        config_reload_topics: [
            "te/device/main///cmd/config_snapshot",
            "te/device/main///cmd/config_update",
        ]
        .into_iter()
        .map(Topic::new_unchecked)
        .collect(),
        tmp_path: Arc::from(Utf8Path::from_path(&std::env::temp_dir()).unwrap()),
        use_tedge_write: TedgeWriteStatus::Disabled,
        mqtt_schema: MqttSchema::new(),
        config_snapshot_topic: TopicFilter::new_unchecked("te/device/main///cmd/config_snapshot/+"),
        config_update_topic: TopicFilter::new_unchecked("te/device/main///cmd/config_update/+"),
        tedge_http_host: "127.0.0.1:3000".into(),
        config_update_enabled: true,
    };

    let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 5);
    let mut fs_watcher_builder: SimpleMessageBoxBuilder<NoMessage, FsWatchEvent> =
        SimpleMessageBoxBuilder::new("FS", 5);
    let mut downloader_builder: FakeServerBoxBuilder<ConfigDownloadRequest, ConfigDownloadResult> =
        FakeServerBoxBuilder::default();
    let mut uploader_builder: FakeServerBoxBuilder<ConfigUploadRequest, ConfigUploadResult> =
        FakeServerBoxBuilder::default();

    let config_builder = ConfigManagerBuilder::try_new(
        config,
        &mut mqtt_builder,
        &mut fs_watcher_builder,
        &mut downloader_builder,
        &mut uploader_builder,
    )
    .await
    .unwrap();

    (
        config_builder,
        mqtt_builder.build().with_timeout(TEST_TIMEOUT_MS),
        fs_watcher_builder.build(),
        downloader_builder.build().with_timeout(TEST_TIMEOUT_MS),
        uploader_builder.build().with_timeout(TEST_TIMEOUT_MS),
    )
}

async fn spawn_config_manager_actor(
    temp_dir: &Path,
) -> (
    MqttMessageBox,
    SimpleMessageBox<NoMessage, FsWatchEvent>,
    DownloaderMessageBox,
    UploaderMessageBox,
) {
    let (actor_builder, mqtt, fs, downloader, uploader) =
        new_config_manager_builder(temp_dir).await;
    let actor = actor_builder.build();
    tokio::spawn(async move { actor.run().await });
    (mqtt, fs, downloader, uploader)
}

#[tokio::test]
async fn default_plugin_config() {
    let tempdir = TempTedgeDir::new();
    let (_mqtt, _fs, _downloader, _uploader) = spawn_config_manager_actor(tempdir.path()).await;
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
        mode = 444
    };

    assert_eq!(plugin_config_toml, expected_config);
}

#[tokio::test]
async fn config_manager_reloads_config_types() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _fs, _downloader, _uploader) = spawn_config_manager_actor(tempdir.path()).await;

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
    let (mut mqtt, _fs, _downloader, mut uploader) =
        spawn_config_manager_actor(tempdir.path()).await;

    let config_topic = Topic::new_unchecked("te/device/main///cmd/config_snapshot/1234");

    // Let's ignore the reload messages sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received
    let snapshot_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_two-1234",
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
                r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_two-1234","type":"type_two"}"#
            ).with_retain())
        );

    // This message being published over MQTT is also received by the config-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Assert config upload request.
    let (topic, upload_request) = uploader.recv().await.unwrap();

    assert_eq!(Topic::new_unchecked(&topic), config_topic);

    assert_eq!(
        upload_request.url,
        "http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_two-1234"
    );
    assert_eq!(upload_request.file_path, tempdir.path().join("file_b"));

    assert_eq!(upload_request.auth, None);

    // Simulate upload file completion
    let upload_response = UploadResponse::new(&upload_request.url, upload_request.file_path);
    uploader.send((topic, Ok(upload_response))).await?;

    // Finally, the config manager notifies that request was successfully processed
    assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(
                &config_topic,
                format!(r#"{{"status":"successful","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_two-1234","type":"type_two","path":{:?}}}"#, tempdir.path().join("file_b"))
            ).with_retain())
        );

    Ok(())
}

#[tokio::test]
async fn config_manager_creates_tedge_url_for_snapshot_request() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _fs, _downloader, mut uploader) =
        spawn_config_manager_actor(tempdir.path()).await;

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
            MqttMessage::new(&config_topic, r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config_snapshot/type_two-1234","type":"type_two"}"#)
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
        "http://127.0.0.1:3000/tedge/file-transfer/main/config_snapshot/type_two-1234"
    );
    assert_eq!(upload_request.file_path, tempdir.path().join("file_b"));

    assert_eq!(upload_request.auth, None);

    // Simulate upload file completion
    let upload_response = UploadResponse::new(&upload_request.url, upload_request.file_path);
    uploader.send((topic, Ok(upload_response))).await?;

    // Finally, the config manager notifies that request was successfully processed
    assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(
                &config_topic,
                format!(r#"{{"status":"successful","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config_snapshot/type_two-1234","type":"type_two","path":{:?}}}"#, tempdir.path().join("file_b"))
            ).with_retain())
        );

    Ok(())
}

#[tokio::test]
async fn config_manager_download_update() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _fs, mut downloader, _uploader) =
        spawn_config_manager_actor(tempdir.path()).await;

    let config_topic = Topic::new_unchecked("te/device/main///cmd/config_update/1234");

    // Let's ignore the reload messages sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received
    let snapshot_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/main/config_update/type_two-1234",
            "remoteUrl": "http://www.remote.url",
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
                r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config_update/type_two-1234","remoteUrl":"http://www.remote.url","type":"type_two"}"#
            ).with_retain())
        );

    // This message being published over MQTT is also received by the config-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Assert config download request.
    let (topic, download_request) = downloader.recv().await.unwrap();

    assert_eq!(Topic::new_unchecked(&topic), config_topic);

    assert_eq!(
        download_request.url,
        "http://127.0.0.1:3000/tedge/file-transfer/main/config_update/type_two-1234"
    );
    assert_eq!(
        download_request.file_path,
        std::env::temp_dir().join("type_two")
    );

    assert_eq!(download_request.auth, None);

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
                format!(r#"{{"status":"successful","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config_update/type_two-1234","remoteUrl":"http://www.remote.url","type":"type_two","path":{:?}}}"#, tempdir.path().join("file_b"))
            ).with_retain())
        );

    Ok(())
}

#[tokio::test]
async fn request_config_snapshot_that_does_not_exist() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _fs, _downloader, _uploader) = spawn_config_manager_actor(tempdir.path()).await;

    let config_topic = Topic::new_unchecked("te/device/main///cmd/config_snapshot/1234");

    // Let's ignore the init message sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received
    let snapshot_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_five-1234",
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
            r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_five-1234","type":"type_five"}"#
        ).with_retain())
    );

    // This message being published over MQTT is also received by the config-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Finally, the config manager notifies that given config type does not exists
    assert_eq!(
        mqtt.recv().await,
        Some(MqttMessage::new(
            &config_topic,
            r#"{"status":"failed","reason":"Failed to initiate configuration snapshot upload to file-transfer service: The requested config_type \"type_five\" is not defined in the plugin configuration file.","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_five-1234","type":"type_five"}"#
        ).with_retain())
    );

    Ok(())
}

#[tokio::test]
async fn ignore_topic_for_another_device() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _fs, _downloader, _uploader) = spawn_config_manager_actor(tempdir.path()).await;

    // Check for child device topic
    let another_device_topic = Topic::new_unchecked("te/device/child01///cmd/config-snapshot/1234");

    // Let's ignore the init message sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received
    let snapshot_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/child01/config-snapshot/type_two-1234",
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
    let (mut mqtt, _fs, _downloader, _uploader) = spawn_config_manager_actor(tempdir.path()).await;

    let config_topic = Topic::new_unchecked("te/device/main///cmd/config_snapshot/1234");

    // Let's ignore the init message sent on start
    mqtt.skip(2).await;

    // When a config snapshot request is received with kind instead of type
    let snapshot_request = r#"
        {
            "status": "init",
            "tedgeurl": "http://127.0.0.1:3000/tedge/file-transfer/main/config-snapshot/type_two-1234",
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
    let (mut mqtt, _fs, _downloader, mut uploader) =
        spawn_config_manager_actor(tempdir.path()).await;

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
        "http://127.0.0.1:3000/tedge/file-transfer/main/config_snapshot/type_two-1234"
    );
    assert_eq!(upload_request.file_path, tempdir.path().join("file_b"));

    assert_eq!(upload_request.auth, None);

    // Simulate upload file completion
    let upload_response = UploadResponse::new(&upload_request.url, upload_request.file_path);
    uploader.send((topic, Ok(upload_response))).await?;

    // Finally, the config manager notifies that request was successfully processed
    assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(
                &config_topic,
                format!(r#"{{"status":"successful","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/config_snapshot/type_two-1234","type":"type_two","path":{:?}}}"#, tempdir.path().join("file_b"))
            ).with_retain())
        );

    Ok(())
}
