use crate::operation_workflows::builder::DownloaderRequest;
use crate::operation_workflows::builder::DownloaderResult;
use crate::operation_workflows::builder::WorkflowActorBuilder;
use crate::operation_workflows::config::OperationConfig;
use crate::software_manager::actor::SoftwareCommand;
use crate::Capabilities;
use camino::Utf8Path;
use serde_json::json;
use std::process::Output;
use std::sync::Arc;
use std::time::Duration;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::test_helpers::TimedMessageBox;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynError;
use tedge_actors::DynSender;
use tedge_actors::MappingSender;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::NoMessage;
use tedge_actors::RequestEnvelope;
use tedge_actors::RuntimeError;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::commands::CommandStatus;
use tedge_api::commands::RestartCommandPayload;
use tedge_api::commands::SoftwareCommandMetadata;
use tedge_api::commands::SoftwareListCommand;
use tedge_api::commands::SoftwareListCommandPayload;
use tedge_api::commands::SoftwareModuleAction;
use tedge_api::commands::SoftwareModuleItem;
use tedge_api::commands::SoftwareRequestResponseSoftwareList;
use tedge_api::commands::SoftwareUpdateCommandPayload;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::GenericCommandData;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::OperationName;
use tedge_api::workflow::OperationStep;
use tedge_api::workflow::OperationStepHandler;
use tedge_api::workflow::OperationStepRequest;
use tedge_api::workflow::OperationStepResponse;
use tedge_api::RestartCommand;
use tedge_api::SoftwareUpdateCommand;
use tedge_downloader_ext::DownloadResponse;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_script_ext::Execute;
use tedge_test_utils::fs::TempTedgeDir;
use tokio::task::JoinHandle;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

#[tokio::test]
async fn convert_incoming_software_list_request() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let TestHandler {
        tmp_dir,
        mut software_box,
        mut mqtt_box,
        ..
    } = spawn_mqtt_operation_converter("device/main//", vec![]).await?;

    // Simulate SoftwareList MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/software_list/some-cmd-id"),
        r#"{ "status": "init" }"#,
    );
    mqtt_box.send(mqtt_message).await?;

    // Assert SoftwareListCommand
    software_box
        .assert_received([SoftwareListCommand {
            target: EntityTopicId::default_main_device(),
            cmd_id: "some-cmd-id".to_string(),
            payload: SoftwareListCommandPayload {
                status: CommandStatus::Scheduled,
                current_software_list: Vec::default(),
                log_path: Some(
                    tmp_dir
                        .path()
                        .join("workflow-software_list-some-cmd-id.log")
                        .try_into()
                        .unwrap(),
                ),
            },
        }])
        .await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_software_update_request() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let TestHandler {
        tmp_dir,
        mut software_box,
        mut mqtt_box,
        ..
    } = spawn_mqtt_operation_converter("device/child001//", vec![]).await?;

    // Simulate SoftwareUpdate MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/child001///cmd/software_update/1234"),
        r#"{"status":"init","updateList":[{"type":"debian","modules":[{"name":"debian1","version":"0.0.1","action":"install"}]}]}"#,
    );
    mqtt_box.send(mqtt_message).await?;

    // Create expected request
    let debian_module1 = SoftwareModuleItem {
        name: "debian1".into(),
        version: Some("0.0.1".into()),
        action: Some(SoftwareModuleAction::Install),
        url: None,
        reason: None,
    };
    let debian_list = SoftwareRequestResponseSoftwareList {
        plugin_type: "debian".into(),
        modules: vec![debian_module1],
        errors: vec![],
    };

    // The output of converter => SoftwareUpdateCommand
    software_box
        .assert_received([SoftwareUpdateCommand {
            target: EntityTopicId::default_child_device("child001").unwrap(),
            cmd_id: "1234".to_string(),
            payload: SoftwareUpdateCommandPayload {
                status: CommandStatus::Scheduled,
                update_list: vec![debian_list],
                failures: vec![],
                log_path: Some(
                    tmp_dir
                        .path()
                        .join("workflow-software_update-1234.log")
                        .try_into()
                        .unwrap(),
                ),
            },
        }])
        .await;

    Ok(())
}

#[tokio::test]
async fn convert_incoming_restart_request() -> Result<(), DynError> {
    let target_device = "device/child-foo//";

    // Spawn incoming mqtt message converter
    let TestHandler {
        tmp_dir,
        mut restart_box,
        mut mqtt_box,
        ..
    } = spawn_mqtt_operation_converter(target_device, vec![]).await?;

    // Simulate Restart MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked(&format!("te/{target_device}/cmd/restart/random")),
        r#"{"status": "init"}"#,
    );
    mqtt_box.send(mqtt_message).await?;

    // Assert RestartOperationRequest
    restart_box
        .assert_received([RestartCommand {
            target: target_device.parse()?,
            cmd_id: "random".to_string(),
            payload: RestartCommandPayload {
                status: CommandStatus::Scheduled,
                log_path: Some(
                    tmp_dir
                        .path()
                        .join("workflow-restart-random.log")
                        .try_into()
                        .unwrap(),
                ),
            },
        }])
        .await;

    Ok(())
}

#[tokio::test]
async fn convert_outgoing_software_list_response() -> Result<(), DynError> {
    // Spawn outgoing mqtt message converter
    let TestHandler {
        mut software_box,
        mut mqtt_box,
        ..
    } = spawn_mqtt_operation_converter("device/main//", vec![]).await?;

    // Declare supported software types from software actor
    software_box
        .send(SoftwareCommand::SoftwareCommandMetadata(
            SoftwareCommandMetadata {
                types: vec!["apt".into(), "docker".into()],
            },
        ))
        .await?;

    skip_capability_messages(&mut mqtt_box, "device/main//").await;

    // Simulate SoftwareList response message received.
    let software_list_request =
        SoftwareListCommand::new(&EntityTopicId::default_main_device(), "1234".to_string());
    let software_list_response = software_list_request
        .clone()
        .with_status(CommandStatus::Successful);
    software_box.send(software_list_response.into()).await?;

    mqtt_box
        .assert_received([MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/software_list/1234"),
            r#"{"status":"successful"}"#,
        )
        .with_retain()])
        .await;

    Ok(())
}

#[tokio::test]
async fn publish_capabilities_on_start() -> Result<(), DynError> {
    // Spawn outgoing mqtt message converter
    let TestHandler {
        mut software_box,
        mut mqtt_box,
        ..
    } = spawn_mqtt_operation_converter("device/child//", vec![]).await?;

    mqtt_box
        .assert_received([MqttMessage::new(
            &Topic::new_unchecked("te/device/child///cmd/restart"),
            "{}",
        )
        .with_retain()])
        .await;

    // Declare supported software types from software actor
    software_box
        .send(SoftwareCommand::SoftwareCommandMetadata(
            SoftwareCommandMetadata {
                types: vec!["apt".into(), "docker".into()],
            },
        ))
        .await?;

    mqtt_box
        .assert_received([MqttMessage::new(
            &Topic::new_unchecked("te/device/child///cmd/software_list"),
            json!({"types": ["apt", "docker"]}).to_string(),
        )
        .with_retain()])
        .await;

    mqtt_box
        .assert_received([MqttMessage::new(
            &Topic::new_unchecked("te/device/child///cmd/software_update"),
            json!({"types": ["apt", "docker"]}).to_string(),
        )
        .with_retain()])
        .await;

    Ok(())
}

#[ignore = "incomplete"]
#[tokio::test]
async fn convert_outgoing_software_update_response() -> Result<(), DynError> {
    // Spawn outgoing mqtt message converter
    let TestHandler {
        mut software_box,
        mut mqtt_box,
        ..
    } = spawn_mqtt_operation_converter("device/main//", vec![]).await?;

    // Declare supported software types from software actor
    software_box
        .send(SoftwareCommand::SoftwareCommandMetadata(
            SoftwareCommandMetadata {
                types: vec!["apt".into(), "docker".into()],
            },
        ))
        .await?;

    skip_capability_messages(&mut mqtt_box, "device/main//").await;

    // Simulate SoftwareUpdate response message received.
    let software_update_request =
        SoftwareUpdateCommand::new(&EntityTopicId::default_main_device(), "1234".to_string());
    let software_update_response = software_update_request.with_status(CommandStatus::Successful);
    software_box.send(software_update_response.into()).await?;

    mqtt_box
        .assert_received([MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/software_update/1234"),
            r#"{"status":"successful"}"#,
        )
        .with_retain()])
        .await;

    Ok(())
}

#[tokio::test]
async fn convert_outgoing_restart_response() -> Result<(), DynError> {
    // Spawn outgoing mqtt message converter
    let TestHandler {
        mut software_box,
        mut restart_box,
        mut mqtt_box,
        ..
    } = spawn_mqtt_operation_converter("device/main//", vec![]).await?;

    // Declare supported software types from software actor
    software_box
        .send(SoftwareCommand::SoftwareCommandMetadata(
            SoftwareCommandMetadata {
                types: vec!["apt".into(), "docker".into()],
            },
        ))
        .await?;

    skip_capability_messages(&mut mqtt_box, "device/main//").await;

    // Simulate Restart response message received.
    let executing_response = RestartCommand {
        target: EntityTopicId::default_main_device(),
        cmd_id: "abc".to_string(),
        payload: RestartCommandPayload::new(CommandStatus::Successful),
    };
    restart_box.send(executing_response).await?;

    let (topic, payload) = mqtt_box
        .recv()
        .await
        .map(|msg| (msg.topic, msg.payload))
        .expect("MqttMessage");
    assert_eq!(topic.name, "te/device/main///cmd/restart/abc");
    assert!(format!("{:?}", payload).contains(r#"status":"successful"#));

    Ok(())
}

#[tokio::test]
async fn download_action() -> Result<(), DynError> {
    let workflow = r#"
operation = "config_update"

[init]
action = "proceed"
on_success = "download"

[download]
action = "download"
input.url = "${.payload.remoteUrl}"
on_success = "successful"
on_error = "failed"

[successful]
action = "cleanup"

[failed]
action = "cleanup"
"#;

    let TestHandler {
        mut mqtt_box,
        mut downloader_box,
        mut actor_handle,
        ..
    } = spawn_mqtt_operation_converter(
        "device/main//",
        vec![("config_update.toml".to_string(), workflow.to_string())],
    )
    .await?;

    // Trigger the operation
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/config_update/123"),
        r#"{"status":"init","remoteUrl":"http://example.com/file"}"#,
    );
    mqtt_box.send(mqtt_message).await?;

    let RequestEnvelope {
        request: (topic, download_request),
        reply_to: _,
    } = recv_or_fail_on_actor_exit(&mut downloader_box, &mut actor_handle, "download request")
        .await
        .expect("download request expected");
    assert_eq!(topic, "te/device/main///cmd/config_update/123");
    assert_eq!(download_request.url, "http://example.com/file");

    Ok(())
}

#[tokio::test]
async fn download_action_without_input_url() -> Result<(), DynError> {
    let workflow = r#"
operation = "config_update"

[init]
action = "proceed"
on_success = "download"

[download]
action = "download"
on_success = "successful"
on_error = "failed"

[successful]
action = "cleanup"

[failed]
action = "cleanup"
"#;

    let TestHandler {
        mut mqtt_box,
        mut downloader_box,
        mut actor_handle,
        ..
    } = spawn_mqtt_operation_converter(
        "device/main//",
        vec![("config_update.toml".to_string(), workflow.to_string())],
    )
    .await?;

    // Trigger the operation
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/config_update/123"),
        r#"{"status":"init","tedgeUrl":"http://example.com/file"}"#,
    );
    mqtt_box.send(mqtt_message).await?;

    // Even without input.url mapping, the download action should fall back to tedgeUrl
    let RequestEnvelope {
        request: (topic, download_request),
        mut reply_to,
    } = recv_or_fail_on_actor_exit(&mut downloader_box, &mut actor_handle, "download request")
        .await
        .expect("download request expected");
    assert_eq!(topic, "te/device/main///cmd/config_update/123");
    assert_eq!(download_request.url, "http://example.com/file");

    // Complete the download successfully
    reply_to
        .send((
            topic.clone(),
            Ok(DownloadResponse {
                url: download_request.url.clone(),
                file_path: download_request.file_path.clone(),
            }),
        ))
        .await?;

    // The workflow should complete successfully
    let payload = recv_command_state_with_status(
        &mut mqtt_box,
        &mut actor_handle,
        "te/device/main///cmd/config_update/123",
        "successful",
    )
    .await;
    assert_eq!(
        payload.get("status").and_then(|v| v.as_str()),
        Some("successful")
    );

    Ok(())
}

#[tokio::test]
async fn download_action_without_input_url_or_tedge_url() -> Result<(), DynError> {
    let workflow = r#"
operation = "config_update"

[init]
action = "proceed"
on_success = "download"

[download]
action = "download"
on_success = "successful"
on_error = "failed"

[successful]
action = "cleanup"

[failed]
action = "cleanup"
"#;

    let TestHandler {
        mut mqtt_box,
        mut downloader_box,
        mut actor_handle,
        ..
    } = spawn_mqtt_operation_converter(
        "device/main//",
        vec![("config_update.toml".to_string(), workflow.to_string())],
    )
    .await?;

    // Trigger the operation
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/config_update/123"),
        r#"{"status":"init","remoteUrl":"http://example.com/file"}"#,
    );
    mqtt_box.send(mqtt_message).await?;

    // Even without input.url mapping, the download action should fall back to remoteUrl
    let RequestEnvelope {
        request: (topic, download_request),
        mut reply_to,
    } = recv_or_fail_on_actor_exit(&mut downloader_box, &mut actor_handle, "download request")
        .await
        .expect("download request expected");
    assert_eq!(topic, "te/device/main///cmd/config_update/123");
    assert_eq!(download_request.url, "http://example.com/file");

    // Complete the download successfully
    reply_to
        .send((
            topic.clone(),
            Ok(DownloadResponse {
                url: download_request.url.clone(),
                file_path: download_request.file_path.clone(),
            }),
        ))
        .await?;

    // The workflow should complete successfully
    let payload = recv_command_state_with_status(
        &mut mqtt_box,
        &mut actor_handle,
        "te/device/main///cmd/config_update/123",
        "successful",
    )
    .await;
    assert_eq!(
        payload.get("status").and_then(|v| v.as_str()),
        Some("successful")
    );

    Ok(())
}

#[tokio::test]
async fn download_action_no_url_available() -> Result<(), DynError> {
    let workflow = r#"
operation = "config_update"

[init]
action = "proceed"
on_success = "download"

[download]
action = "download"
on_success = "successful"
on_error = "failed"

[successful]
action = "cleanup"

[failed]
action = "cleanup"
"#;

    let TestHandler {
        mut mqtt_box,
        mut downloader_box,
        mut actor_handle,
        ..
    } = spawn_mqtt_operation_converter(
        "device/main//",
        vec![("config_update.toml".to_string(), workflow.to_string())],
    )
    .await?;

    // Trigger the operation without any URL field
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/config_update/123"),
        r#"{"status":"init"}"#,
    );
    mqtt_box.send(mqtt_message).await?;

    // No URL available should not trigger a downloader request
    assert_no_message_or_actor_exit(
        &mut downloader_box,
        &mut actor_handle,
        "waiting for unexpected download request",
    )
    .await;

    // The workflow should fail with an explicit reason
    let payload = recv_command_state_with_status(
        &mut mqtt_box,
        &mut actor_handle,
        "te/device/main///cmd/config_update/123",
        "failed",
    )
    .await;
    assert_eq!(
        payload.get("status").and_then(|v| v.as_str()),
        Some("failed")
    );
    assert_eq!(
        payload.get("reason").and_then(|v| v.as_str()),
        Some(
            "builtin 'download' action failed with: No valid URL found in input.url, tedgeUrl, or remoteUrl",
        )
    );

    Ok(())
}

#[tokio::test]
async fn builtin_operation_step_action() -> Result<(), DynError> {
    let workflow = r#"
operation = "config_update"

[init]
action = "proceed"
on_success = "set"

[set]
action = "builtin:config_update:set"
input.setFrom = "${.payload.downloadedPath}"
on_success = "successful"
on_error = "failed"

[successful]
action = "cleanup"

[failed]
action = "cleanup"
"#;

    let TestHandler {
        mut mqtt_box,
        mut config_box,
        mut actor_handle,
        ..
    } = spawn_mqtt_operation_converter(
        "device/main//",
        vec![("config_update.toml".to_string(), workflow.to_string())],
    )
    .await?;

    // Trigger the operation
    let init_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/config_update/123"),
        r#"{"status":"init", "downloadedPath":"/tmp/test-file"}"#,
    );
    mqtt_box.send(init_message).await?;

    let RequestEnvelope {
        request,
        reply_to: _,
    } = recv_or_fail_on_actor_exit(
        &mut config_box,
        &mut actor_handle,
        "builtin operation step request",
    )
    .await
    .expect("expected builtin operation step request");

    assert_eq!(request.command_step, "set");
    assert_eq!(request.command_state.status, "set");
    let command_payload = serde_json::to_value(&request.command_state.payload)?;
    assert_eq!(
        command_payload.get("setFrom").and_then(|v| v.as_str()),
        Some("/tmp/test-file")
    );

    Ok(())
}

#[tokio::test]
async fn builtin_operation_step_action_missing_input_mapping() -> Result<(), DynError> {
    let workflow = r#"
operation = "config_update"

[init]
action = "proceed"
on_success = "set"

[set]
action = "builtin:config_update:set"
on_success = "successful"
on_error = "failed"

[successful]
action = "cleanup"

[failed]
action = "cleanup"
"#;

    let TestHandler {
        mut mqtt_box,
        mut config_box,
        mut actor_handle,
        ..
    } = spawn_mqtt_operation_converter(
        "device/main//",
        vec![("config_update.toml".to_string(), workflow.to_string())],
    )
    .await?;

    let init_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/config_update/123"),
        r#"{"status":"init", "downloadedPath":"/tmp/test-file"}"#,
    );
    mqtt_box.send(init_message).await?;

    let RequestEnvelope {
        request,
        reply_to: _,
    } = recv_or_fail_on_actor_exit(
        &mut config_box,
        &mut actor_handle,
        "builtin operation step request",
    )
    .await
    .expect("expected builtin operation step request");

    assert_eq!(request.command_step, "set");
    assert_eq!(request.command_state.status, "set");
    let command_payload = serde_json::to_value(&request.command_state.payload)?;
    assert_eq!(command_payload.get("setFrom"), None);

    Ok(())
}

struct TestHandler {
    tmp_dir: Arc<TempTedgeDir>,
    actor_handle: JoinHandle<Result<(), RuntimeError>>,
    mqtt_box: TimedMessageBox<SimpleMessageBox<MqttMessage, MqttMessage>>,
    software_box: TimedMessageBox<SimpleMessageBox<SoftwareCommand, SoftwareCommand>>,
    restart_box: TimedMessageBox<SimpleMessageBox<RestartCommand, RestartCommand>>,
    _inotify_box: TimedMessageBox<SimpleMessageBox<NoMessage, FsWatchEvent>>,
    downloader_box: TimedMessageBox<
        SimpleMessageBox<RequestEnvelope<DownloaderRequest, DownloaderResult>, NoMessage>,
    >,
    config_box: TimedMessageBox<
        SimpleMessageBox<RequestEnvelope<OperationStepRequest, OperationStepResponse>, NoMessage>,
    >,
}

async fn spawn_mqtt_operation_converter(
    device_topic_id: &str,
    workflows: Vec<(String, String)>,
) -> Result<TestHandler, DynError> {
    let mut software_builder = SoftwareActor(SimpleMessageBoxBuilder::new("Software", 5));
    let mut restart_builder = RestartActor(SimpleMessageBoxBuilder::new("Restart", 5));
    let mut config_builder = ConfigActorBuilder(SimpleMessageBoxBuilder::new("Config", 5));

    let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 5);
    let mut script_builder: SimpleMessageBoxBuilder<
        RequestEnvelope<Execute, std::io::Result<Output>>,
        NoMessage,
    > = SimpleMessageBoxBuilder::new("Script", 5);
    let mut inotify_builder: SimpleMessageBoxBuilder<NoMessage, FsWatchEvent> =
        SimpleMessageBoxBuilder::new("Inotify", 5);
    let mut downloade_builder: SimpleMessageBoxBuilder<
        RequestEnvelope<DownloaderRequest, DownloaderResult>,
        NoMessage,
    > = SimpleMessageBoxBuilder::new("Downloader", 5);

    let tmp_dir = Arc::new(TempTedgeDir::new());
    let tmp_path = Utf8Path::from_path(tmp_dir.path()).unwrap();
    let operations_dir = tmp_dir.dir("operations");
    for (file_name, content) in workflows {
        operations_dir.file(&file_name).with_raw_content(&content);
    }
    let device_topic_id = device_topic_id
        .parse::<EntityTopicId>()
        .expect("Invalid topic id");
    let service_topic_id = device_topic_id
        .default_service_for_device("tedge-agent")
        .expect("Invalid service topic id");
    let config = OperationConfig {
        mqtt_schema: MqttSchema::new(),
        device_topic_id,
        service_topic_id,
        log_dir: tmp_path.into(),
        config_dir: tmp_path.into(),
        state_dir: tmp_path.join("running-operations"),
        operations_dir: operations_dir.utf8_path_buf(),
        tmp_dir: tmp_path.into(),
        capabilities: Capabilities::default(),
    };
    let mut workflow_actor_builder = WorkflowActorBuilder::new(
        config,
        &mut mqtt_builder,
        &mut script_builder,
        &mut inotify_builder,
        &mut downloade_builder,
    );
    workflow_actor_builder.register_builtin_operation(&mut restart_builder);
    workflow_actor_builder.register_builtin_operation(&mut software_builder);
    workflow_actor_builder.register_builtin_operation_step_handler(&mut config_builder);

    let config_box = config_builder.0.build().with_timeout(TEST_TIMEOUT_MS);
    let software_box = software_builder.0.build().with_timeout(TEST_TIMEOUT_MS);
    let restart_box = restart_builder.0.build().with_timeout(TEST_TIMEOUT_MS);
    let mqtt_box = mqtt_builder.build().with_timeout(TEST_TIMEOUT_MS);
    let downloader_box = downloade_builder.build().with_timeout(TEST_TIMEOUT_MS);
    let _inotify_box = inotify_builder.build().with_timeout(TEST_TIMEOUT_MS);

    let workflow_actor = workflow_actor_builder.build();
    let tmp_dir_guard = Arc::clone(&tmp_dir);
    let actor_handle = tokio::spawn(async move {
        // Keep tmp_dir alive for the full actor lifetime.
        let _tmp_dir_guard = tmp_dir_guard;
        workflow_actor.run().await
    });

    Ok(TestHandler {
        tmp_dir,
        actor_handle,
        mqtt_box,
        software_box,
        restart_box,
        _inotify_box,
        downloader_box,
        config_box,
    })
}

async fn skip_capability_messages(mqtt: &mut impl MessageReceiver<MqttMessage>, device: &str) {
    //Skip all the init messages by still doing loose assertions
    assert_received_contains_str(
        mqtt,
        [
            (format!("te/{}/cmd/restart", device).as_ref(), "{}"),
            (
                format!("te/{}/cmd/software_list", device).as_ref(),
                &json!({"types": ["apt", "docker"]}).to_string(),
            ),
            (
                format!("te/{}/cmd/software_update", device).as_ref(),
                &json!({"types": ["apt", "docker"]}).to_string(),
            ),
        ],
    )
    .await;
}

async fn recv_command_state_with_status(
    mqtt: &mut impl MessageReceiver<MqttMessage>,
    actor_handle: &mut tokio::task::JoinHandle<Result<(), RuntimeError>>,
    topic: &str,
    status: &str,
) -> serde_json::Value {
    while let Some(msg) =
        recv_or_fail_on_actor_exit(mqtt, actor_handle, "waiting for command state message").await
    {
        if msg.topic.name != topic {
            continue;
        }
        let payload: serde_json::Value = serde_json::from_slice(msg.payload_bytes())
            .expect("command payload must be valid JSON");
        if payload.get("status").and_then(|v| v.as_str()) == Some(status) {
            return payload;
        }
    }

    panic!("expected command state with status '{status}' on topic '{topic}'");
}

async fn recv_or_fail_on_actor_exit<T>(
    message_box: &mut impl MessageReceiver<T>,
    actor_handle: &mut JoinHandle<Result<(), RuntimeError>>,
    context: &str,
) -> Option<T> {
    tokio::select! {
        msg = message_box.recv() => {
            if msg.is_some() {
                return msg;
            }

            panic!(
                "message receive timed out while waiting for {context}"
            );
        }
        actor = actor_handle => {
            match actor {
                Ok(Ok(())) => panic!("workflow actor exited unexpectedly while waiting for {context}"),
                Ok(Err(err)) => panic!("workflow actor failed while waiting for {context}: {err}"),
                Err(err) => panic!("workflow actor panicked while waiting for {context}: {err}"),
            }
        }
    }
}

async fn assert_no_message_or_actor_exit<T>(
    message_box: &mut impl MessageReceiver<T>,
    actor_handle: &mut JoinHandle<Result<(), RuntimeError>>,
    context: &str,
) {
    tokio::select! {
        msg = message_box.recv() => {
            assert!(msg.is_none(), "unexpected message received while {context}");
        }
        actor = actor_handle => {
            match actor {
                Ok(Ok(())) => panic!("workflow actor exited unexpectedly while {context}"),
                Ok(Err(err)) => panic!("workflow actor failed while {context}: {err}"),
                Err(err) => panic!("workflow actor panicked while {context}: {err}"),
            }
        }
    }
}

// FIXME: find a way to avoid repeating ourselves with fake and actual restart actors
struct RestartActor(SimpleMessageBoxBuilder<RestartCommand, RestartCommand>);

impl MessageSource<GenericCommandData, NoConfig> for RestartActor {
    fn connect_sink(&mut self, config: NoConfig, peer: &impl MessageSink<GenericCommandData>) {
        self.0.connect_sink(config, &peer.get_sender())
    }
}

impl IntoIterator for &RestartActor {
    type Item = (OperationName, DynSender<GenericCommandState>);
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let sender = MappingSender::new(self.0.get_sender(), |msg: GenericCommandState| {
            msg.try_into().ok()
        });
        vec![(OperationType::Restart.to_string(), sender.into())].into_iter()
    }
}

// FIXME: find a way to avoid repeating ourselves with fake and actual software actors
struct SoftwareActor(SimpleMessageBoxBuilder<SoftwareCommand, SoftwareCommand>);

impl MessageSource<GenericCommandData, NoConfig> for SoftwareActor {
    fn connect_sink(&mut self, config: NoConfig, peer: &impl MessageSink<GenericCommandData>) {
        self.0
            .connect_mapped_sink(config, &peer.get_sender(), |msg: SoftwareCommand| {
                msg.into_generic_commands()
            })
    }
}

impl IntoIterator for &SoftwareActor {
    type Item = (OperationName, DynSender<GenericCommandState>);
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        let software_list_sender =
            MappingSender::new(self.0.get_sender(), |msg: GenericCommandState| {
                SoftwareListCommand::try_from(msg)
                    .map(SoftwareCommand::SoftwareListCommand)
                    .ok()
            });
        let software_update_sender =
            MappingSender::new(self.0.get_sender(), |msg: GenericCommandState| {
                SoftwareUpdateCommand::try_from(msg)
                    .map(SoftwareCommand::SoftwareUpdateCommand)
                    .ok()
            })
            .into();
        vec![
            (
                OperationType::SoftwareList.to_string(),
                software_list_sender.into(),
            ),
            (
                OperationType::SoftwareUpdate.to_string(),
                software_update_sender,
            ),
        ]
        .into_iter()
    }
}

struct ConfigActorBuilder(
    SimpleMessageBoxBuilder<
        RequestEnvelope<OperationStepRequest, OperationStepResponse>,
        NoMessage,
    >,
);

impl OperationStepHandler for ConfigActorBuilder {
    fn supported_operation_steps(&self) -> Vec<(OperationType, OperationStep)> {
        vec![(OperationType::ConfigUpdate, OperationStep::from("set"))]
    }
}

impl MessageSink<RequestEnvelope<OperationStepRequest, OperationStepResponse>>
    for ConfigActorBuilder
{
    fn get_sender(
        &self,
    ) -> DynSender<RequestEnvelope<OperationStepRequest, OperationStepResponse>> {
        self.0.get_sender()
    }
}
