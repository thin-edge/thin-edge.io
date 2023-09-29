use crate::LogManagerBuilder;
use crate::LogManagerConfig;
use crate::Topic;
use filetime::set_file_mtime;
use filetime::FileTime;
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::test_helpers::TimedMessageBox;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::MessageReceiver;
use tedge_actors::NoMessage;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_file_system_ext::FsWatchEvent;
use tedge_http_ext::test_helpers::assert_request_eq;
use tedge_http_ext::test_helpers::HttpResponseBuilder;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpRequestBuilder;
use tedge_http_ext::HttpResult;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::TopicFilter;
use tedge_test_utils::fs::TempTedgeDir;

type MqttMessageBox = TimedMessageBox<SimpleMessageBox<MqttMessage, MqttMessage>>;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

/// Preparing a temp directory containing four files, with
/// two types { type_one, type_two } and one file with type that does not exists:
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
fn new_log_manager_builder(
    temp_dir: &Path,
) -> (
    LogManagerBuilder,
    TimedMessageBox<SimpleMessageBox<MqttMessage, MqttMessage>>,
    SimpleMessageBox<HttpRequest, HttpResult>,
    SimpleMessageBox<NoMessage, FsWatchEvent>,
) {
    let config = LogManagerConfig {
        config_dir: temp_dir.to_path_buf(),
        plugin_config_dir: temp_dir.to_path_buf(),
        plugin_config_path: temp_dir.join("tedge-log-plugin.toml"),
        logtype_reload_topic: Topic::new_unchecked("te/device/main///cmd/log_upload"),
        logfile_request_topic: TopicFilter::new_unchecked("te/device/main///cmd/log_upload/+"),
        current_operations: HashSet::new(),
    };

    let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 5);
    let mut http_builder: SimpleMessageBoxBuilder<HttpRequest, HttpResult> =
        SimpleMessageBoxBuilder::new("HTTP", 1);
    let mut fs_watcher_builder: SimpleMessageBoxBuilder<NoMessage, FsWatchEvent> =
        SimpleMessageBoxBuilder::new("FS", 5);

    let log_builder = LogManagerBuilder::try_new(
        config,
        &mut mqtt_builder,
        &mut http_builder,
        &mut fs_watcher_builder,
    )
    .unwrap();

    (
        log_builder,
        mqtt_builder.build().with_timeout(TEST_TIMEOUT_MS),
        http_builder.build(),
        fs_watcher_builder.build(),
    )
}

/// Spawn a log manager actor and return 2 boxes to exchange MQTT and HTTP messages with it
fn spawn_log_manager_actor(
    temp_dir: &Path,
) -> (
    MqttMessageBox,
    SimpleMessageBox<HttpRequest, HttpResult>,
    SimpleMessageBox<NoMessage, FsWatchEvent>,
) {
    let (actor_builder, mqtt, http, fs) = new_log_manager_builder(temp_dir);
    let mut actor = actor_builder.build();
    tokio::spawn(async move { actor.run().await });
    (mqtt, http, fs)
}

#[tokio::test]
async fn log_manager_reloads_log_types() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _http, _fs) = spawn_log_manager_actor(tempdir.path());

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
    let (mut mqtt, mut http, _fs) = spawn_log_manager_actor(tempdir.path());

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

    // Then uploads the requested content over HTTP
    let actual_request = http.recv().await;
    let expected_request = Some(
        HttpRequestBuilder::put(
            "http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_two-1234",
        )
        .header("Content-Type", "text/plain")
        .body("filename: file_c\nSome content\n".to_string())
        .build()
        .unwrap(),
    );
    assert_request_eq(actual_request, expected_request);

    // File transfer responds with 200 OK
    let response = HttpResponseBuilder::new().status(201).build().unwrap();
    http.send(Ok(response)).await?;

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
    let (mut mqtt, _http, _fs) = spawn_log_manager_actor(tempdir.path());

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
            r#"{"status":"failed","reason":"Handling of operation failed with No such file or directory for log type: type_four","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_four-1234","type":"type_four","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":1000}"#
        ).with_retain())
    );

    Ok(())
}

#[tokio::test]
async fn put_logfiles_without_permissions() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, mut http, _fs) = spawn_log_manager_actor(tempdir.path());

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

    // Then uploads the requested content over HTTP
    assert!(http.recv().await.is_some());

    // File transfer responds with error code
    let response = HttpResponseBuilder::new().status(403).build().unwrap();
    http.send(Ok(response)).await?;

    // Finally, the log manager notifies that given log manager could not upload logfiles via HTTP
    assert_eq!(
            mqtt.recv().await,
            Some(MqttMessage::new(
                &logfile_topic,
                r#"{"status":"failed","reason":"Handling of operation failed with Failed with HTTP error status 403 Forbidden","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_two-1234","type":"type_two","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":1000}"#
            ).with_retain())
        );

    Ok(())
}

#[tokio::test]
async fn ignore_topic_for_another_device() -> Result<(), anyhow::Error> {
    let tempdir = prepare()?;
    let (mut mqtt, _http, _fs) = spawn_log_manager_actor(tempdir.path());

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
    let (mut mqtt, _http, _fs) = spawn_log_manager_actor(tempdir.path());

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
    let (mut mqtt, _http, _fs) = spawn_log_manager_actor(tempdir.path());

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
            r#"{"status":"failed","reason":"Handling of operation failed with No such file or directory for log type: type_three","tedgeUrl":"http://127.0.0.1:3000/tedge/file-transfer/main/log_upload/type_three-1234","type":"type_three","dateFrom":"1970-01-01T00:00:00Z","dateTo":"1970-01-01T00:00:30Z","lines":1000}"#
        ).with_retain())
    );

    Ok(())
}
