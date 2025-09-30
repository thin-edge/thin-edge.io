use crate::LogManagerBuilder;
use crate::LogManagerConfig;
use crate::LogUploadRequest;
use crate::LogUploadResult;
use crate::Topic;
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

/// Preparing a temp directory with a mocked `file` plugin.
fn prepare() -> Result<TempTedgeDir, anyhow::Error> {
    let tempdir = TempTedgeDir::new();

    let plugin_dir = tempdir.dir("log-plugins");

    let plugin_script = r#"#!/bin/bash
case "$1" in
    "list")
        echo "type_one"
        echo "type_two"
        ;;
    "get")
        case "$2" in
            "type_one")
                echo "DEBUG: Starting application"
                echo "INFO: Application initialized"
                echo "ERROR: Database connection failed"
                echo "DEBUG: Retrying database connection"
                echo "INFO: Database connected successfully"
                echo "WARN: Low memory detected"
                echo "DEBUG: Garbage collection started"
                echo "INFO: Processing complete"
                ;;
            "type_two")
                echo "Some content"
                ;;
            *)
                # Simulate no logs found for unknown types
                echo "No logs found for log type \"$2\"" >&2
                exit 1
                ;;
        esac
        ;;
    *)
        exit 1
        ;;
esac
"#;

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
        tmp_dir: Arc::from(Utf8Path::from_path(temp_dir).unwrap()),
        log_dir: temp_dir.to_path_buf().try_into().unwrap(),
        plugin_dirs: vec![temp_dir
            .to_path_buf()
            .join("log-plugins")
            .try_into()
            .unwrap()],
        plugin_config_dir: temp_dir.to_path_buf(),
        plugin_config_path: temp_dir.join("tedge-log-plugin.toml"),
        logtype_reload_topic: Topic::new_unchecked("te/device/main///cmd/log_upload"),
        logfile_request_topic: TopicFilter::new_unchecked("te/device/main///cmd/log_upload/+"),
        sudo_enabled: false,
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
            MqttMessage::new(&log_reload_topic, r#"{"types":["type_one","type_two"]}"#)
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
            "tedgeUrl": "http://127.0.0.1:3000/te/v1/files/main/log_upload/type_two-1234",
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
                r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/log_upload/type_two-1234","type":"type_two","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":1000}"#
            ).with_retain())
        );
    // This message being published over MQTT is also received by the log-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Assert log upload request.
    let (topic, upload_request) = uploader.recv().await.unwrap();

    assert_eq!(Topic::new_unchecked(&topic), logfile_topic);

    assert_eq!(
        upload_request.url,
        "http://127.0.0.1:3000/te/v1/files/main/log_upload/type_two-1234"
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
                r#"{"status":"successful","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/log_upload/type_two-1234","type":"type_two","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":1000}"#
            ).with_retain())
        );

    Ok(())
}

#[tokio::test]
async fn filter_logs_by_line_count() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _fs, mut uploader) = spawn_log_manager_actor(tempdir.path()).await;

    let logfile_topic = Topic::new_unchecked("te/device/main///cmd/log_upload/5678");

    // Let's ignore the init message sent on start
    mqtt.skip(1).await;

    // When a log request is received with lines limit of 3
    let log_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/te/v1/files/main/log_upload/type_one-5678",
            "type": "type_one",
            "dateFrom": "1970-01-01T00:00:00+00:00",
            "dateTo": "1970-01-01T00:00:30+00:00",
            "lines": 3
        }"#;
    mqtt.send(MqttMessage::new(&logfile_topic, log_request).with_retain())
        .await?;

    // The log manager notifies that the request has been received and is processed
    let executing_message = mqtt.recv().await;
    assert_eq!(
        executing_message,
        Some(MqttMessage::new(
                &logfile_topic,
                r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/log_upload/type_one-5678","type":"type_one","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":3}"#
            ).with_retain())
        );
    // This message being published over MQTT is also received by the log-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Assert log upload request.
    let (topic, upload_request) = uploader.recv().await.unwrap();

    assert_eq!(Topic::new_unchecked(&topic), logfile_topic);

    // Verify the uploaded file contains only the last 3 lines
    let file_content = read_to_string(&upload_request.file_path).unwrap();
    let lines: Vec<&str> = file_content.lines().collect();
    assert_eq!(lines.len(), 3);
    assert!(lines.contains(&"WARN: Low memory detected"));
    assert!(lines.contains(&"DEBUG: Garbage collection started"));
    assert!(lines.contains(&"INFO: Processing complete"));

    // Simulate upload is completed.
    let upload_response = UploadResponse::new(&upload_request.url, upload_request.file_path);
    uploader.send((topic, Ok(upload_response))).await?;

    // Finally, the log manager notifies that request was successfully processed
    assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(
                &logfile_topic,
                r#"{"status":"successful","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/log_upload/type_one-5678","type":"type_one","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":3}"#
            ).with_retain())
        );

    Ok(())
}

#[tokio::test]
async fn filter_logs_by_search_text() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _fs, mut uploader) = spawn_log_manager_actor(tempdir.path()).await;

    let logfile_topic = Topic::new_unchecked("te/device/main///cmd/log_upload/9012");

    // Let's ignore the init message sent on start
    mqtt.skip(1).await;

    // When a log request is received with search text filter for "ERROR"
    let log_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/te/v1/files/main/log_upload/type_one-9012",
            "type": "type_one",
            "dateFrom": "1970-01-01T00:00:00+00:00",
            "dateTo": "1970-01-01T00:00:30+00:00",
            "lines": 1000,
            "searchText": "ERROR"
        }"#;
    mqtt.send(MqttMessage::new(&logfile_topic, log_request).with_retain())
        .await?;

    // The log manager notifies that the request has been received and is processed
    let executing_message = mqtt.recv().await;
    assert_eq!(
        executing_message,
        Some(MqttMessage::new(
                &logfile_topic,
                r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/log_upload/type_one-9012","type":"type_one","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","searchText":"ERROR","lines":1000}"#
            ).with_retain())
        );
    // This message being published over MQTT is also received by the log-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Assert log upload request.
    let (topic, upload_request) = uploader.recv().await.unwrap();

    assert_eq!(Topic::new_unchecked(&topic), logfile_topic);

    // Verify the uploaded file contains only lines with "ERROR"
    let file_content = read_to_string(&upload_request.file_path).unwrap();
    let lines: Vec<&str> = file_content.lines().collect();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0], "ERROR: Database connection failed");

    // Simulate upload is completed.
    let upload_response = UploadResponse::new(&upload_request.url, upload_request.file_path);
    uploader.send((topic, Ok(upload_response))).await?;

    // Finally, the log manager notifies that request was successfully processed
    assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(
                &logfile_topic,
                r#"{"status":"successful","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/log_upload/type_one-9012","type":"type_one","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","searchText":"ERROR","lines":1000}"#
            ).with_retain())
        );

    Ok(())
}

#[tokio::test]
async fn filter_logs_by_search_text_and_line_count() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _fs, mut uploader) = spawn_log_manager_actor(tempdir.path()).await;

    let logfile_topic = Topic::new_unchecked("te/device/main///cmd/log_upload/3456");

    // Let's ignore the init message sent on start
    mqtt.skip(1).await;

    // When a log request is received with both search text filter and line count
    let log_request = r#"
        {
            "status": "init",
            "tedgeUrl": "http://127.0.0.1:3000/te/v1/files/main/log_upload/type_one-3456",
            "type": "type_one",
            "dateFrom": "1970-01-01T00:00:00+00:00",
            "dateTo": "1970-01-01T00:00:30+00:00",
            "lines": 2,
            "searchText": "DEBUG"
        }"#;
    mqtt.send(MqttMessage::new(&logfile_topic, log_request).with_retain())
        .await?;

    // The log manager notifies that the request has been received and is processed
    let executing_message = mqtt.recv().await;
    assert_eq!(
        executing_message,
        Some(MqttMessage::new(
                &logfile_topic,
                r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/log_upload/type_one-3456","type":"type_one","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","searchText":"DEBUG","lines":2}"#
            ).with_retain())
        );
    // This message being published over MQTT is also received by the log-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Assert log upload request.
    let (topic, upload_request) = uploader.recv().await.unwrap();

    assert_eq!(Topic::new_unchecked(&topic), logfile_topic);

    // Verify the uploaded file contains only the last 2 lines that match "DEBUG"
    let file_content = read_to_string(&upload_request.file_path).unwrap();
    let lines: Vec<&str> = file_content.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines.contains(&"DEBUG: Retrying database connection"));
    assert!(lines.contains(&"DEBUG: Garbage collection started"));

    // Simulate upload is completed.
    let upload_response = UploadResponse::new(&upload_request.url, upload_request.file_path);
    uploader.send((topic, Ok(upload_response))).await?;

    // Finally, the log manager notifies that request was successfully processed
    assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(
                &logfile_topic,
                r#"{"status":"successful","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/log_upload/type_one-3456","type":"type_one","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","searchText":"DEBUG","lines":2}"#
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
                "tedgeUrl": "http://127.0.0.1:3000/te/v1/files/main/log_upload/type_four-1234",
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
            r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/log_upload/type_four-1234","type":"type_four","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":1000}"#
        ).with_retain())
    );
    // This message being published over MQTT is also received by the log-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Finally, the log manager notifies that given log type does not exists
    assert_eq!(
        mqtt.recv().await,
        Some(MqttMessage::new(
            &logfile_topic,
            r#"{"status":"failed","reason":"Failed to initiate log file upload: Log plugin 'file' error: Get command error: No logs found for log type \"type_four\"\n","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/log_upload/type_four-1234","type":"type_four","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":1000}"#
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
            "tedgeUrl": "http://127.0.0.1:3000/te/v1/files/child01/log_upload/type_two-1234",
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
            "url": "http://127.0.0.1:3000/te/v1/files/child01/log_upload/type_two-1234",
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
            "tedgeUrl": "http://127.0.0.1:3000/te/v1/files/main/log_upload/type_three-1234",
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
                r#"{"status":"executing","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/log_upload/type_three-1234","type":"type_three","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":1000}"#
            ).with_retain())
        );
    // This message being published over MQTT is also received by the log-manager itself
    mqtt.send(executing_message.unwrap()).await?;

    // Finally, the log manager notifies that given log type does not exists
    assert_eq!(
        mqtt.recv().await,
        Some(MqttMessage::new(
            &logfile_topic,
            r#"{"status":"failed","reason":"Failed to initiate log file upload: Log plugin 'file' error: Get command error: No logs found for log type \"type_three\"\n","tedgeUrl":"http://127.0.0.1:3000/te/v1/files/main/log_upload/type_three-1234","type":"type_three","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":1000}"#
        ).with_retain())
    );

    Ok(())
}
