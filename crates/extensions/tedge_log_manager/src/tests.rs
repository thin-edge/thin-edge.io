use crate::LogManagerBuilder;
use crate::LogManagerConfig;
use crate::LogUploadRequest;
use crate::LogUploadResult;
use crate::Topic;
use filetime::set_file_mtime;
use filetime::FileTime;
use std::fs::read_to_string;
use std::path::Path;
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
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tedge_test_utils::fs::TempTedgeDir;
use tedge_uploader_ext::UploadResponse;
use toml::from_str;
use toml::toml;
use toml::Table;

type MqttMessageBox = TimedMessageBox<SimpleMessageBox<MqttMessage, MqttMessage>>;
type UploaderMessageBox = TimedMessageBox<FakeServerBox<LogUploadRequest, LogUploadResult>>;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

/// Preparing a temp directory containing four files, with
/// two types { type_one, type_two } and one file for log type that does not exists:
///
///     file_a, type_one
///     file_b, type_one
///     file_c, type_two
///     file_d, type_one
///     file_e, type_three (does not exist)
/// each file has the following modified "file update" timestamp:
///     file_a has timestamp: 1970/01/01 00:00:02
///     file_b has timestamp: 1970/01/01 00:00:03
///     file_c has timestamp: 1970/01/01 00:00:11
///     file_d has timestamp: (current, not modified)
fn prepare() -> Result<TempTedgeDir, anyhow::Error> {
    let tempdir = TempTedgeDir::new();
    let tempdir_path = tempdir
        .path()
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("temp dir not created"))?;

    std::fs::File::create(format!("{tempdir_path}/file_a"))?;
    std::fs::File::create(format!("{tempdir_path}/file_b"))?;
    tempdir.file("file_c").with_raw_content("Some content");
    std::fs::File::create(format!("{tempdir_path}/file_d"))?;

    let new_mtime = FileTime::from_unix_time(2, 0);
    set_file_mtime(format!("{tempdir_path}/file_a"), new_mtime).unwrap();

    let new_mtime = FileTime::from_unix_time(3, 0);
    set_file_mtime(format!("{tempdir_path}/file_b"), new_mtime).unwrap();

    let new_mtime = FileTime::from_unix_time(11, 0);
    set_file_mtime(format!("{tempdir_path}/file_c"), new_mtime).unwrap();

    tempdir
        .file("tedge-log-plugin.toml")
        .with_raw_content(&format!(
            r#"files = [
            {{ type = "type_one", path = "{tempdir_path}/file_a" }},
            {{ type = "type_one", path = "{tempdir_path}/file_b" }},
            {{ type = "type_two", path = "{tempdir_path}/file_c" }},
            {{ type = "type_one", path = "{tempdir_path}/file_d" }},
            {{ type = "type_three", path = "{tempdir_path}/file_e" }}, 
        ]"#
        ));

    Ok(tempdir)
}

/// Create a log manager actor builder
/// along two boxes to exchange MQTT and HTTP messages with the log actor
#[allow(clippy::type_complexity)]
async fn new_log_manager_builder(
    temp_dir: &Path,
) -> (
    LogManagerBuilder,
    TimedMessageBox<SimpleMessageBox<MqttMessage, MqttMessage>>,
    SimpleMessageBox<NoMessage, FsWatchEvent>,
    UploaderMessageBox,
) {
    let config = LogManagerConfig {
        mqtt_schema: MqttSchema::default(),
        config_dir: temp_dir.to_path_buf(),
        tmp_dir: temp_dir.to_path_buf(),
        log_dir: temp_dir.to_path_buf().try_into().unwrap(),
        plugin_config_dir: temp_dir.to_path_buf(),
        plugin_config_path: temp_dir.join("tedge-log-plugin.toml"),
        logtype_reload_topic: Topic::new_unchecked("te/device/main///cmd/log_upload"),
        logfile_request_topic: TopicFilter::new_unchecked("te/device/main///cmd/log_upload/+"),
    };

    let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 5);
    let mut fs_watcher_builder: SimpleMessageBoxBuilder<NoMessage, FsWatchEvent> =
        SimpleMessageBoxBuilder::new("FS", 5);
    let mut uploader_builder: FakeServerBoxBuilder<LogUploadRequest, LogUploadResult> =
        FakeServerBoxBuilder::default();

    let mut log_builder =
        LogManagerBuilder::try_new(config, &mut fs_watcher_builder, &mut uploader_builder)
            .await
            .unwrap();

    log_builder.connect_mqtt(&mut mqtt_builder);

    (
        log_builder,
        mqtt_builder.build().with_timeout(TEST_TIMEOUT_MS),
        fs_watcher_builder.build(),
        uploader_builder.build().with_timeout(TEST_TIMEOUT_MS),
    )
}

/// Spawn a log manager actor and return 2 boxes to exchange MQTT and HTTP messages with it
async fn spawn_log_manager_actor(
    temp_dir: &Path,
) -> (
    MqttMessageBox,
    SimpleMessageBox<NoMessage, FsWatchEvent>,
    UploaderMessageBox,
) {
    let (actor_builder, mqtt, fs, uploader) = new_log_manager_builder(temp_dir).await;
    let actor = actor_builder.build();
    tokio::spawn(async move { actor.run().await });
    (mqtt, fs, uploader)
}

#[tokio::test]
async fn default_plugin_config() {
    let tempdir = TempTedgeDir::new();
    let (_mqtt, _fs, _uploader) = spawn_log_manager_actor(tempdir.path()).await;
    let plugin_config_content =
        read_to_string(tempdir.path().join("tedge-log-plugin.toml")).unwrap();
    let plugin_config_toml: Table = from_str(&plugin_config_content).unwrap();

    let agent_logs_path = format!(
        "{}/agent/workflow-software_*",
        tempdir.path().to_string_lossy()
    );
    let expected_config = toml! {
        [[files]]
        type = "software-management"
        path = agent_logs_path
    };

    assert_eq!(plugin_config_toml, expected_config);
}

#[tokio::test]
async fn log_manager_reloads_log_types() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _fs, _uploader) = spawn_log_manager_actor(tempdir.path()).await;

    let log_reload_topic = Topic::new_unchecked("te/device/main///cmd/log_upload");

    assert_eq!(
        mqtt.recv().await,
        Some(
            MqttMessage::new(
                &log_reload_topic,
                r#"{"types":["type_one","type_three","type_two"]}"#
            )
            .with_retain()
        )
    );

    Ok(())
}

#[tokio::test]
async fn log_manager_upload_log_files_on_request() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _fs, mut uploader) = spawn_log_manager_actor(tempdir.path()).await;

    let logfile_topic = Topic::new_unchecked("te/device/main///cmd/log_upload/1234");

    // Let's ignore the init message sent on start
    mqtt.skip(1).await;

    // When a log request is received
    let log_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_two-1234",
            "type": "type_two",
            "dateFrom": "1970-01-01T00:00:00+00:00",
            "dateTo": "1970-01-01T00:00:30+00:00",
            "lines": 1000
        }"#;
    mqtt.send(MqttMessage::new(&logfile_topic, log_request).with_retain())
        .await?;

    // The log manager notifies that the request has been received and is processed
    let executing_message = mqtt.recv().await;
    assert_eq!(
        executing_message,
        Some(MqttMessage::new(
                &logfile_topic,
                r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_two-1234","type":"type_two","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":1000}"#
            ).with_retain())
        );
    // This message being published over MQTT is also received by the log-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Assert log upload request.
    let (topic, upload_request) = uploader.recv().await.unwrap();

    assert_eq!(Topic::new_unchecked(&topic), logfile_topic);

    assert_eq!(
        upload_request.url,
        "http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_two-1234"
    );
    assert!(
        upload_request.file_path.starts_with(tempdir.path()),
        "Expected the log file to be created in tempdir"
    );
    assert!(
        upload_request
            .file_path
            .file_name()
            .unwrap()
            .starts_with("type_two"),
        "Expected a log file name with the log type as prefix"
    );

    assert_eq!(upload_request.auth, None);

    // Simulate upload is completed.
    let upload_response = UploadResponse::new(&upload_request.url, upload_request.file_path);
    uploader.send((topic, Ok(upload_response))).await?;

    // Finally, the log manager notifies that request was successfully processed
    assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(
                &logfile_topic,
                r#"{"status":"successful","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_two-1234","type":"type_two","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":1000}"#
            ).with_retain())
        );

    Ok(())
}

#[tokio::test]
async fn request_logtype_that_does_not_exist() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _fs, _uploader) = spawn_log_manager_actor(tempdir.path()).await;

    let logfile_topic = Topic::new_unchecked("te/device/main///cmd/log_upload/1234");

    // Let's ignore the init message sent on start
    mqtt.skip(1).await;

    // When a log request is received
    let log_request = r#"
            {
                "status": "init",
                "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_four-1234",
                "type": "type_four",
                "dateFrom": "1970-01-01T00:00:00+00:00",
                "dateTo": "1970-01-01T00:00:30+00:00",
                "lines": 1000
            }"#;
    mqtt.send(MqttMessage::new(&logfile_topic, log_request).with_retain())
        .await?;

    // The log manager notifies that the request has been received and is processed
    let executing_message = mqtt.recv().await;
    assert_eq!(
        executing_message,
        Some(MqttMessage::new(
            &logfile_topic,
            r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_four-1234","type":"type_four","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":1000}"#
        ).with_retain())
    );
    // This message being published over MQTT is also received by the log-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Finally, the log manager notifies that given log type does not exists
    assert_eq!(
        mqtt.recv().await,
        Some(MqttMessage::new(
            &logfile_topic,
            r#"{"status":"failed","reason":"Failed to initiate log file upload: No logs found for log type \"type_four\"","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_four-1234","type":"type_four","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":1000}"#
        ).with_retain())
    );

    Ok(())
}

#[tokio::test]
async fn ignore_topic_for_another_device() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _http, _fs) = spawn_log_manager_actor(tempdir.path()).await;

    // Check for child device topic
    let another_device_topic = Topic::new_unchecked("te/device/child01///cmd/log_upload/1234");

    // Let's ignore the init message sent on start
    mqtt.skip(1).await;

    // When a log request is received
    let log_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/child01/log_upload/type_two-1234",
            "type": "type_two",
            "dateFrom": "1970-01-01T00:00:00+00:00",
            "dateTo": "1970-01-01T00:00:30+00:00",
            "lines": 1000
        }"#;
    mqtt.send(MqttMessage::new(&another_device_topic, log_request).with_retain())
        .await?;

    // The log manager does proceed to "executing" state
    assert!(mqtt.recv().await.is_none());

    Ok(())
}

#[tokio::test]
async fn send_incorrect_payload() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _fs, _uploader) = spawn_log_manager_actor(tempdir.path()).await;

    let logfile_topic = Topic::new_unchecked("te/device/main///cmd/log_upload/1234");

    // Let's ignore the init message sent on start
    mqtt.skip(1).await;

    // Receive log request with url instead of tedgeUrl
    let log_request = r#"
        {
            "status": "init",
            "url": "http://127.0.0.1:3000/tedge/file-transfer/child01/log_upload/type_two-1234",
            "type": "type_two",
            "dateFrom": "1970-01-01T00:00:00+00:00",
            "dateTo": "1970-01-01T00:00:30+00:00",
            "lines": 1000
        }"#;
    mqtt.send(MqttMessage::new(&logfile_topic, log_request).with_retain())
        .await?;

    // The log manager does proceed to "executing" state
    assert!(mqtt.recv().await.is_none());

    Ok(())
}

#[tokio::test]
async fn read_log_from_file_that_does_not_exist() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _fs, _uploader) = spawn_log_manager_actor(tempdir.path()).await;

    let logfile_topic = Topic::new_unchecked("te/device/main///cmd/log_upload/1234");

    // Let's ignore the init message sent on start
    mqtt.skip(1).await;

    // When a log request is received
    let log_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_three-1234",
            "type": "type_three",
            "dateFrom": "1970-01-01T00:00:00+00:00",
            "dateTo": "1970-01-01T00:00:30+00:00",
            "lines": 1000
        }"#;
    mqtt.send(MqttMessage::new(&logfile_topic, log_request).with_retain())
        .await?;

    // The log manager notifies that the request has been received and is processed
    let executing_message = mqtt.recv().await;
    assert_eq!(
        executing_message,
        Some(MqttMessage::new(
                &logfile_topic,
                r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_three-1234","type":"type_three","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":1000}"#
            ).with_retain())
        );
    // This message being published over MQTT is also received by the log-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Finally, the log manager notifies that given log type does not exists
    assert_eq!(
        mqtt.recv().await,
        Some(MqttMessage::new(
            &logfile_topic,
            r#"{"status":"failed","reason":"Failed to initiate log file upload: No logs found for log type \"type_three\"","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_three-1234","type":"type_three","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":1000}"#
        ).with_retain())
    );

    Ok(())
}
