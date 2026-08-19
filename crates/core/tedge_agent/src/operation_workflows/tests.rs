use crate::operation_workflows::builder::DownloaderRequest;
use crate::operation_workflows::builder::DownloaderResult;
use crate::operation_workflows::builder::UploaderRequest;
use crate::operation_workflows::builder::UploaderResult;
use crate::operation_workflows::builder::WorkflowActorBuilder;
use crate::operation_workflows::config::OperationConfig;
use crate::software_manager::actor::SoftwareCommand;
use crate::Capabilities;
use camino::Utf8Path;
use serde_json::json;
use std::os::unix::process::ExitStatusExt;
use std::process::Output;
use std::sync::Arc;
use std::time::Duration;
use tedge_actors::test_helpers::FakeServerBox;
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
use tedge_api::commands::CmdMetaSyncSignal;
use tedge_api::commands::CommandStatus;
use tedge_api::commands::RestartCommandPayload;
use tedge_api::commands::SoftwareCommandMetadata;
use tedge_api::commands::SoftwareListCommand;
use tedge_api::commands::SoftwareListCommandPayload;
use tedge_api::commands::SoftwareModuleAction;
use tedge_api::commands::SoftwareModuleItem;
use tedge_api::commands::SoftwareRequestResponseSoftwareList;
use tedge_api::commands::SoftwareUpdateCommandPayload;
use tedge_api::entity::EntityMetadata;
use tedge_api::entity::EntityType;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::log::log_dir::OperationLogs;
use tedge_api::workflow::GenericCommandData;
use tedge_api::workflow::GenericCommandState;
use tedge_api::workflow::OperationName;
use tedge_api::workflow::OperationStep;
use tedge_api::workflow::OperationStepHandler;
use tedge_api::workflow::OperationStepRequest;
use tedge_api::workflow::OperationStepResponse;
use tedge_api::workflow::SyncOnCommand;
use tedge_api::RestartCommand;
use tedge_api::SoftwareUpdateCommand;
use tedge_downloader_ext::DownloadResponse;
use tedge_file_system_ext::FsWatchEvent;
use tedge_http_ext::test_helpers::HttpResponseBuilder;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpResult;
use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_script_ext::Execute;
use tedge_test_utils::fs::TempTedgeDir;
use tedge_uploader_ext::UploadResponse;
use tedge_utils::paths::TedgePaths;
use test_case::test_case;
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
async fn upload_action() -> Result<(), DynError> {
    let workflow = r#"
operation = "config_snapshot"

[init]
action = "proceed"
on_success = "upload"

[upload]
action = "upload"
input.url = "${.payload.tedgeUrl}"
on_success = "successful"
on_error = "failed"

[successful]
action = "cleanup"

[failed]
action = "cleanup"
"#;

    let TestHandler {
        mut mqtt_box,
        mut uploader_box,
        mut actor_handle,
        ..
    } = spawn_mqtt_operation_converter(
        "device/main//",
        vec![("config_snapshot.toml".to_string(), workflow.to_string())],
    )
    .await?;

    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/config_snapshot/123"),
        r#"{"status":"init","tedgeUrl":"http://example.com/upload","path":"/tmp/snapshot.conf","type":"tedge.toml"}"#,
    );
    mqtt_box.send(mqtt_message).await?;

    let RequestEnvelope {
        request: (topic, upload_request),
        reply_to: _,
    } = recv_or_fail_on_actor_exit(&mut uploader_box, &mut actor_handle, "upload request")
        .await
        .expect("upload request expected");
    assert_eq!(topic, "te/device/main///cmd/config_snapshot/123");
    assert_eq!(upload_request.url, "http://example.com/upload");
    assert_eq!(upload_request.file_path.as_str(), "/tmp/snapshot.conf");

    Ok(())
}

#[tokio::test]
async fn upload_action_without_input_url() -> Result<(), DynError> {
    let workflow = r#"
operation = "config_snapshot"

[init]
action = "proceed"
on_success = "upload"

[upload]
action = "upload"
on_success = "successful"
on_error = "failed"

[successful]
action = "cleanup"

[failed]
action = "cleanup"
"#;

    let TestHandler {
        mut mqtt_box,
        mut uploader_box,
        mut actor_handle,
        ..
    } = spawn_mqtt_operation_converter(
        "device/main//",
        vec![("config_snapshot.toml".to_string(), workflow.to_string())],
    )
    .await?;

    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/config_snapshot/123"),
        r#"{"status":"init","tedgeUrl":"http://example.com/upload","path":"/tmp/snapshot.conf","type":"tedge.toml"}"#,
    );
    mqtt_box.send(mqtt_message).await?;

    // Fall back to tedgeUrl and path from the payload when input is omitted
    let RequestEnvelope {
        request: (topic, upload_request),
        mut reply_to,
    } = recv_or_fail_on_actor_exit(&mut uploader_box, &mut actor_handle, "upload request")
        .await
        .expect("upload request expected");
    assert_eq!(topic, "te/device/main///cmd/config_snapshot/123");
    assert_eq!(upload_request.url, "http://example.com/upload");
    assert_eq!(upload_request.file_path.as_str(), "/tmp/snapshot.conf");

    reply_to
        .send((
            topic.clone(),
            Ok(UploadResponse {
                url: upload_request.url.clone(),
                file_path: upload_request.file_path.clone(),
            }),
        ))
        .await?;

    let payload = recv_command_state_with_status(
        &mut mqtt_box,
        &mut actor_handle,
        "te/device/main///cmd/config_snapshot/123",
        "successful",
    )
    .await;
    assert_eq!(
        payload.get("path").and_then(|v| v.as_str()),
        Some("/tmp/snapshot.conf")
    );

    Ok(())
}

#[tokio::test]
async fn upload_action_missing_path() -> Result<(), DynError> {
    let workflow = r#"
operation = "config_snapshot"

[init]
action = "proceed"
on_success = "upload"

[upload]
action = "upload"
input.url = "${.payload.tedgeUrl}"
on_success = "successful"
on_error = "failed"

[successful]
action = "cleanup"

[failed]
action = "cleanup"
"#;

    let TestHandler {
        mut mqtt_box,
        mut uploader_box,
        mut actor_handle,
        ..
    } = spawn_mqtt_operation_converter(
        "device/main//",
        vec![("config_snapshot.toml".to_string(), workflow.to_string())],
    )
    .await?;

    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/config_snapshot/123"),
        r#"{"status":"init","tedgeUrl":"http://example.com/upload","type":"tedge.toml"}"#,
    );
    mqtt_box.send(mqtt_message).await?;

    assert_no_message_or_actor_exit(
        &mut uploader_box,
        &mut actor_handle,
        "waiting for unexpected upload request",
    )
    .await;

    let payload = recv_command_state_with_status(
        &mut mqtt_box,
        &mut actor_handle,
        "te/device/main///cmd/config_snapshot/123",
        "failed",
    )
    .await;
    assert_eq!(
        payload.get("reason").and_then(|v| v.as_str()),
        Some("builtin 'upload' action failed with: No valid file path found in input.path or path",)
    );

    Ok(())
}

#[tokio::test]
async fn config_snapshot_get_operation_step() -> Result<(), DynError> {
    let workflow = r#"
operation = "config_snapshot"

[init]
action = "proceed"
on_success = "get"

[get]
action = "builtin:config_snapshot:get"
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
        vec![("config_snapshot.toml".to_string(), workflow.to_string())],
    )
    .await?;

    let init_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/config_snapshot/123"),
        r#"{"status":"init","tedgeUrl":"http://example.com/upload","type":"tedge.toml"}"#,
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

    assert_eq!(request.command_step, "get");
    assert_eq!(request.command_state.status, "get");
    let command_payload = serde_json::to_value(&request.command_state.payload)?;
    assert_eq!(
        command_payload.get("type").and_then(|v| v.as_str()),
        Some("tedge.toml")
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

#[tokio::test]
async fn sync_signal_sent_to_listeners_on_successful_workflow_operation() -> Result<(), DynError> {
    // A workflow-based `config_update` operation reaching its `successful` state must notify
    // the actors listening for its completion. Unlike a monolithic builtin operation, this
    // terminal state flows through `process_command_update` (not `process_builtin_command_update`),
    // so this test guards the fix that emits the sync signal on that path.
    let workflow = r#"
operation = "config_update"

[init]
action = "proceed"
on_success = "successful"

[successful]
action = "cleanup"
"#;

    let TestHandler {
        mut mqtt_box,
        mut sync_signal_box,
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
        r#"{"status":"init"}"#,
    );
    mqtt_box.send(init_message).await?;

    // The listener registered for `config_update` is notified once the workflow finishes
    let signal =
        recv_or_fail_on_actor_exit(&mut sync_signal_box, &mut actor_handle, "sync signal").await;
    assert!(
        signal.is_some(),
        "expected a sync signal on operation completion"
    );

    Ok(())
}

#[tokio::test]
async fn sync_signal_sent_to_listeners_on_failed_workflow_operation() -> Result<(), DynError> {
    // Listeners must also be notified when the operation reaches its `failed` terminal state.
    let workflow = r#"
operation = "config_update"

[init]
action = "proceed"
on_success = "failed"

[failed]
action = "cleanup"
"#;

    let TestHandler {
        mut mqtt_box,
        mut sync_signal_box,
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
        r#"{"status":"init"}"#,
    );
    mqtt_box.send(init_message).await?;

    // The listener registered for `config_update` is notified once the workflow finishes
    let signal =
        recv_or_fail_on_actor_exit(&mut sync_signal_box, &mut actor_handle, "sync signal").await;
    assert!(
        signal.is_some(),
        "expected a sync signal on operation completion"
    );

    Ok(())
}

#[tokio::test]
async fn a_command_addressed_to_a_service_of_this_device_is_driven() -> Result<(), DynError> {
    let TestHandler {
        mut mqtt_box,
        mut actor_handle,
        ..
    } = spawn_workflow_actor(
        "device/main//",
        vec![(
            "service-restart.toml".to_string(),
            SERVICE_RESTART_WORKFLOW.to_string(),
        )],
        FakeEntityStore::Entities(vec![service_of(
            "device/main/service/collectd",
            "device/main//",
        )]),
    )
    .await?;

    // The service workflow declares no capability on the device topics
    skip_device_restart_capability(&mut mqtt_box, "device/main//").await;

    mqtt_box
        .send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/collectd/cmd/restart/1"),
            r#"{"status":"init","serviceName":"collectd","serviceType":"service"}"#,
        ))
        .await?;

    let payload = recv_command_state_with_status(
        &mut mqtt_box,
        &mut actor_handle,
        "te/device/main/service/collectd/cmd/restart/1",
        "successful",
    )
    .await;
    assert_eq!(
        payload.get("serviceName").and_then(|v| v.as_str()),
        Some("collectd")
    );

    Ok(())
}

#[tokio::test]
async fn a_command_addressed_to_a_service_of_another_device_is_ignored() -> Result<(), DynError> {
    let TestHandler {
        mut mqtt_box,
        mut actor_handle,
        ..
    } = spawn_workflow_actor(
        "device/main//",
        vec![(
            "service-restart.toml".to_string(),
            SERVICE_RESTART_WORKFLOW.to_string(),
        )],
        FakeEntityStore::Entities(vec![service_of(
            "device/child01/service/nginx",
            "device/child01//",
        )]),
    )
    .await?;

    skip_device_restart_capability(&mut mqtt_box, "device/main//").await;

    mqtt_box
        .send(MqttMessage::new(
            &Topic::new_unchecked("te/device/child01/service/nginx/cmd/restart/1"),
            r#"{"status":"init","serviceName":"nginx","serviceType":"service"}"#,
        ))
        .await?;

    assert_no_message_or_actor_exit(
        &mut mqtt_box,
        &mut actor_handle,
        "a command addressed to a service of another device is ignored",
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn a_command_addressed_to_an_unregistered_entity_is_ignored() -> Result<(), DynError> {
    let TestHandler {
        mut mqtt_box,
        mut actor_handle,
        ..
    } = spawn_workflow_actor(
        "device/main//",
        vec![(
            "service-restart.toml".to_string(),
            SERVICE_RESTART_WORKFLOW.to_string(),
        )],
        FakeEntityStore::Entities(vec![]),
    )
    .await?;

    skip_device_restart_capability(&mut mqtt_box, "device/main//").await;

    mqtt_box
        .send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/collectd/cmd/restart/1"),
            r#"{"status":"init","serviceName":"collectd","serviceType":"service"}"#,
        ))
        .await?;

    assert_no_message_or_actor_exit(
        &mut mqtt_box,
        &mut actor_handle,
        "a command addressed to an unregistered entity is ignored",
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn a_failed_lookup_leaves_the_command_untouched() -> Result<(), DynError> {
    let TestHandler {
        mut mqtt_box,
        mut actor_handle,
        ..
    } = spawn_workflow_actor(
        "device/main//",
        vec![(
            "service-restart.toml".to_string(),
            SERVICE_RESTART_WORKFLOW.to_string(),
        )],
        FakeEntityStore::Failing,
    )
    .await?;

    skip_device_restart_capability(&mut mqtt_box, "device/main//").await;

    mqtt_box
        .send(MqttMessage::new(
            &Topic::new_unchecked("te/device/main/service/collectd/cmd/restart/1"),
            r#"{"status":"init","serviceName":"collectd","serviceType":"service"}"#,
        ))
        .await?;

    assert_no_message_or_actor_exit(
        &mut mqtt_box,
        &mut actor_handle,
        "the entity store cannot be queried",
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn a_service_command_is_driven_under_a_custom_topic_scheme() -> Result<(), DynError> {
    let TestHandler {
        mut mqtt_box,
        mut restart_box,
        mut actor_handle,
        ..
    } = spawn_workflow_actor(
        "factory01/hallA/plc1/",
        vec![(
            "service-restart.toml".to_string(),
            SERVICE_RESTART_WORKFLOW.to_string(),
        )],
        FakeEntityStore::Entities(vec![service_of(
            "factory01/hallA/plc1/nginx",
            "factory01/hallA/plc1/",
        )]),
    )
    .await?;

    skip_device_restart_capability(&mut mqtt_box, "factory01/hallA/plc1/").await;

    mqtt_box
        .send(MqttMessage::new(
            &Topic::new_unchecked("te/factory01/hallA/plc1//cmd/restart/1"),
            r#"{"status":"init"}"#,
        ))
        .await?;
    let command = recv_or_fail_on_actor_exit(
        &mut restart_box,
        &mut actor_handle,
        "a restart command for the device itself",
    )
    .await
    .expect("expected a restart command");
    assert_eq!(
        command.target,
        "factory01/hallA/plc1/".parse::<EntityTopicId>()?
    );

    mqtt_box
        .send(MqttMessage::new(
            &Topic::new_unchecked("te/factory01/hallA/plc1/nginx/cmd/restart/2"),
            r#"{"status":"init","serviceName":"nginx","serviceType":"service"}"#,
        ))
        .await?;
    recv_command_state_with_status(
        &mut mqtt_box,
        &mut actor_handle,
        "te/factory01/hallA/plc1/nginx/cmd/restart/2",
        "successful",
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn an_edited_service_workflow_declares_no_capability_of_the_device() -> Result<(), DynError> {
    let tmp_dir = Arc::new(TempTedgeDir::new());
    let TestHandler {
        mut mqtt_box,
        mut inotify_box,
        mut actor_handle,
        ..
    } = spawn_workflow_actor_in(
        Arc::clone(&tmp_dir),
        "device/main//",
        vec![(
            "service-restart.toml".to_string(),
            SERVICE_RESTART_WORKFLOW.to_string(),
        )],
        FakeEntityStore::Entities(vec![]),
    )
    .await?;

    skip_device_restart_capability(&mut mqtt_box, "device/main//").await;

    let path = edit_workflow(&tmp_dir, "service-restart.toml", SERVICE_RESTART_WORKFLOW);
    inotify_box.send(FsWatchEvent::Modified(path)).await?;

    assert_no_message_or_actor_exit(
        &mut mqtt_box,
        &mut actor_handle,
        "a service workflow is edited",
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn an_edited_device_workflow_declares_its_capability_again() -> Result<(), DynError> {
    let tmp_dir = Arc::new(TempTedgeDir::new());
    let TestHandler {
        mut mqtt_box,
        mut inotify_box,
        mut actor_handle,
        ..
    } = spawn_workflow_actor_in(
        Arc::clone(&tmp_dir),
        "device/main//",
        vec![(
            "device-pause.toml".to_string(),
            DEVICE_PAUSE_WORKFLOW.to_string(),
        )],
        FakeEntityStore::Entities(vec![]),
    )
    .await?;

    assert_received_contains_str(
        &mut mqtt_box,
        [
            ("te/device/main///cmd/pause", "{}"),
            ("te/device/main///cmd/restart", "{}"),
        ],
    )
    .await;

    let path = edit_workflow(&tmp_dir, "device-pause.toml", DEVICE_PAUSE_WORKFLOW);
    inotify_box.send(FsWatchEvent::Modified(path)).await?;

    let capability = recv_or_fail_on_actor_exit(
        &mut mqtt_box,
        &mut actor_handle,
        "the capability of the edited workflow",
    )
    .await
    .expect("expected a capability message");
    assert_eq!(capability.topic.name, "te/device/main///cmd/pause");

    Ok(())
}

#[tokio::test]
async fn a_successful_action_completes_the_command() -> Result<(), DynError> {
    let TestHandler {
        mut mqtt_box,
        mut script_box,
        mut actor_handle,
        ..
    } = spawn_agent_running(SHIPPED_RESTART_WORKFLOW, "service_restart.toml").await?;

    trigger_service_command(&mut mqtt_box, "restart", "collectd", "collectd").await?;

    pass_the_agent_guard(&mut script_box, &mut actor_handle).await;

    let script = recv_script(&mut script_box, &mut actor_handle).await;
    assert_eq!(script.request.command, "sudo");
    assert_eq!(
        script.request.args,
        [
            "-n",
            "tedge",
            "service",
            "restart",
            "collectd",
            "--service-type",
            "service"
        ]
    );
    // A backend which never returns is given a bounded time
    assert!(script.request.timeouts.is_some());
    reply_with(script, exited(0)).await;

    assert_command_status(
        &mut mqtt_box,
        &mut actor_handle,
        "restart",
        "collectd",
        "successful",
    )
    .await;

    Ok(())
}

#[test_case(exited(1), "operation log"; "the backend failed")]
#[test_case(exited(2), "not supported"; "the action is not supported")]
// A process killed on timeout is reported with signal 9, not with an exit code
#[test_case(std::process::ExitStatus::from_raw(9), "did not complete in time"; "killed on timeout")]
#[tokio::test]
async fn a_failing_action_fails_the_command(
    outcome: std::process::ExitStatus,
    expected_reason: &str,
) -> Result<(), DynError> {
    let TestHandler {
        mut mqtt_box,
        mut script_box,
        mut actor_handle,
        ..
    } = spawn_agent_running(SHIPPED_RESTART_WORKFLOW, "service_restart.toml").await?;

    trigger_service_command(&mut mqtt_box, "restart", "collectd", "collectd").await?;

    pass_the_agent_guard(&mut script_box, &mut actor_handle).await;

    // The backend writes its own reason to the operation log, not on stdout,
    // so the workflow has to give a reason of its own for each outcome
    let script = recv_script(&mut script_box, &mut actor_handle).await;
    reply_with(script, outcome).await;

    let failure = assert_command_status(
        &mut mqtt_box,
        &mut actor_handle,
        "restart",
        "collectd",
        "failed",
    )
    .await;
    let reason = failure["reason"]
        .as_str()
        .expect("a failed command gives a reason");
    assert!(
        reason.contains(expected_reason),
        "unexpected reason: {reason}"
    );

    Ok(())
}

#[test_case("tedge-agent", true)]
#[test_case("tedge-agent.service", true)]
#[test_case("tedge-agentx", false)]
#[test_case("tedge-agent.socket", false)]
#[test_case("collectd", false)]
fn test_tells_both_spellings_of_a_unit_name_from_any_other(service_name: &str, is_the_unit: bool) {
    let outcome = std::process::Command::new("test")
        .args(both_spellings_of(service_name, "tedge-agent"))
        .output()
        .expect("test is expected to be installed");

    assert_eq!(
        outcome.status.success(),
        is_the_unit,
        "unexpected match of {service_name}: {outcome:?}"
    );
}

#[test_case("tedge-agent"; "named as the service")]
#[test_case("tedge-agent.service"; "named as the unit")]
#[tokio::test]
async fn restarting_the_agent_restarts_the_process_once(
    service_name: &str,
) -> Result<(), DynError> {
    let tmp_dir = Arc::new(TempTedgeDir::new());
    let TestHandler {
        mut mqtt_box,
        mut script_box,
        mut actor_handle,
        ..
    } = spawn_agent_running_in(
        Arc::clone(&tmp_dir),
        SHIPPED_RESTART_WORKFLOW,
        "service_restart.toml",
    )
    .await?;

    trigger_service_command(&mut mqtt_box, "restart", "tedge-agent", service_name).await?;

    let check = recv_script(&mut script_box, &mut actor_handle).await;
    assert_eq!(
        check.request.args,
        both_spellings_of(service_name, "tedge-agent")
    );
    reply_with(check, exited(0)).await;

    // The agent restarts itself rather than asking a backend to do it, and the state awaiting
    // the restart is persisted before the process stops
    assert_command_status(
        &mut mqtt_box,
        &mut actor_handle,
        "restart",
        "tedge-agent",
        "await-agent-restart",
    )
    .await;
    assert!(
        matches!(actor_handle.await, Ok(Err(RuntimeError::RestartRequired))),
        "the agent is expected to ask for a process restart"
    );

    // On restart, the awaited restart is over: the command completes without the action being run
    let TestHandler {
        mut mqtt_box,
        mut script_box,
        mut actor_handle,
        ..
    } = spawn_agent_running_in(tmp_dir, SHIPPED_RESTART_WORKFLOW, "service_restart.toml").await?;

    assert_command_status(
        &mut mqtt_box,
        &mut actor_handle,
        "restart",
        "tedge-agent",
        "successful",
    )
    .await;
    assert_no_message_or_actor_exit(&mut script_box, &mut actor_handle, "a script").await;

    Ok(())
}

/// Run the agent of the main device with one of the shipped service workflows
async fn spawn_agent_running(workflow: &str, file_name: &str) -> Result<TestHandler, DynError> {
    spawn_agent_running_in(Arc::new(TempTedgeDir::new()), workflow, file_name).await
}

async fn spawn_agent_running_in(
    tmp_dir: Arc<TempTedgeDir>,
    workflow: &str,
    file_name: &str,
) -> Result<TestHandler, DynError> {
    let mut handler = spawn_workflow_actor_in(
        tmp_dir,
        "device/main//",
        vec![(file_name.to_string(), workflow.to_string())],
        FakeEntityStore::Entities(vec![
            service_of("device/main/service/collectd", "device/main//"),
            service_of("device/main/service/tedge-agent", "device/main//"),
            service_of("device/main/service/tedge-mapper-c8y", "device/main//"),
            service_of("device/main/service/tedge-mapper-collectd", "device/main//"),
            service_of("device/main/service/tedge-mapper-local", "device/main//"),
        ]),
    )
    .await?;

    // A service workflow declares no capability on the device topics
    skip_device_restart_capability(&mut handler.mqtt_box, "device/main//").await;

    Ok(handler)
}

/// Address a service of the main device with a command, as the c8y mapper does
///
/// `service_name` is the name the cloud sends, which a backend may know the target under rather
/// than the one it was registered with: a systemd unit is named `<service>` as well as
/// `<service>.service`.
async fn trigger_service_command(
    mqtt: &mut impl Sender<MqttMessage>,
    action: &str,
    service: &str,
    service_name: &str,
) -> Result<(), DynError> {
    let topic = format!("te/device/main/service/{service}/cmd/{action}/1");
    mqtt.send(MqttMessage::new(
        &Topic::new_unchecked(&topic),
        json!({"status": "init", "serviceName": service_name, "serviceType": "service"})
            .to_string(),
    ))
    .await?;

    Ok(())
}

/// The argv of a guard asking whether a service name is a service, under either of its spellings
fn both_spellings_of(service_name: &str, service: &str) -> Vec<String> {
    vec![
        service_name.to_string(),
        "=".to_string(),
        service.to_string(),
        "-o".to_string(),
        service_name.to_string(),
        "=".to_string(),
        format!("{service}.service"),
    ]
}

/// Let the command of a service other than tedge-agent through the guard on the agent's own name
async fn pass_the_agent_guard(
    script_box: &mut impl MessageReceiver<RequestEnvelope<Execute, std::io::Result<Output>>>,
    actor_handle: &mut JoinHandle<Result<(), RuntimeError>>,
) {
    let check = recv_script(script_box, actor_handle).await;
    reply_with(check, exited(1)).await;
}

/// The next script run by the workflow, to be given its outcome with [reply_with]
async fn recv_script(
    script_box: &mut impl MessageReceiver<RequestEnvelope<Execute, std::io::Result<Output>>>,
    actor_handle: &mut JoinHandle<Result<(), RuntimeError>>,
) -> RequestEnvelope<Execute, std::io::Result<Output>> {
    recv_or_fail_on_actor_exit(script_box, actor_handle, "a script to run")
        .await
        .expect("expected a script to be run")
}

async fn reply_with(
    script: RequestEnvelope<Execute, std::io::Result<Output>>,
    status: std::process::ExitStatus,
) {
    let RequestEnvelope { mut reply_to, .. } = script;
    reply_to
        .send(Ok(Output {
            status,
            stdout: vec![],
            stderr: vec![],
        }))
        .await
        .expect("the workflow is expected to await the outcome of its script")
}

fn exited(code: i32) -> std::process::ExitStatus {
    std::process::ExitStatus::from_raw(code << 8)
}

async fn assert_command_status(
    mqtt: &mut impl MessageReceiver<MqttMessage>,
    actor_handle: &mut JoinHandle<Result<(), RuntimeError>>,
    action: &str,
    service: &str,
    status: &str,
) -> serde_json::Value {
    let topic = format!("te/device/main/service/{service}/cmd/{action}/1");
    recv_command_state_with_status(mqtt, actor_handle, &topic, status).await
}

struct TestHandler {
    tmp_dir: Arc<TempTedgeDir>,
    actor_handle: JoinHandle<Result<(), RuntimeError>>,
    mqtt_box: TimedMessageBox<SimpleMessageBox<MqttMessage, MqttMessage>>,
    software_box: TimedMessageBox<SimpleMessageBox<SoftwareCommand, SoftwareCommand>>,
    restart_box: TimedMessageBox<SimpleMessageBox<RestartCommand, RestartCommand>>,
    inotify_box: TimedMessageBox<SimpleMessageBox<NoMessage, FsWatchEvent>>,
    downloader_box: TimedMessageBox<
        SimpleMessageBox<RequestEnvelope<DownloaderRequest, DownloaderResult>, NoMessage>,
    >,
    uploader_box: TimedMessageBox<
        SimpleMessageBox<RequestEnvelope<UploaderRequest, UploaderResult>, NoMessage>,
    >,
    config_box: TimedMessageBox<
        SimpleMessageBox<RequestEnvelope<OperationStepRequest, OperationStepResponse>, NoMessage>,
    >,
    sync_signal_box: TimedMessageBox<SimpleMessageBox<CmdMetaSyncSignal, NoMessage>>,
    script_box: TimedMessageBox<
        SimpleMessageBox<RequestEnvelope<Execute, std::io::Result<Output>>, NoMessage>,
    >,
}

/// A fake entity store, answering the entity lookups of the workflow actor over HTTP
enum FakeEntityStore {
    /// Serve the given entities, answering `404` for any other
    Entities(Vec<EntityMetadata>),
    /// Answer `500` to every request, as an entity store that cannot be queried
    Failing,
}

impl FakeEntityStore {
    async fn serve(self, mut http: FakeServerBox<HttpRequest, HttpResult>) {
        while let Some(request) = http.recv().await {
            let response = match &self {
                FakeEntityStore::Failing => HttpResponseBuilder::new().status(500).build(),
                FakeEntityStore::Entities(entities) => {
                    let target = request
                        .uri()
                        .path()
                        .strip_prefix("/te/v1/entities/")
                        .and_then(|path| path.parse::<EntityTopicId>().ok());
                    match entities
                        .iter()
                        .find(|entity| Some(&entity.topic_id) == target.as_ref())
                    {
                        Some(entity) => HttpResponseBuilder::new().status(200).json(entity).build(),
                        None => HttpResponseBuilder::new().status(404).build(),
                    }
                }
            };
            if http.send(response).await.is_err() {
                break;
            }
        }
    }
}

async fn spawn_mqtt_operation_converter(
    device_topic_id: &str,
    workflows: Vec<(String, String)>,
) -> Result<TestHandler, DynError> {
    spawn_workflow_actor(
        device_topic_id,
        workflows,
        FakeEntityStore::Entities(vec![]),
    )
    .await
}

async fn spawn_workflow_actor(
    device_topic_id: &str,
    workflows: Vec<(String, String)>,
    entity_store: FakeEntityStore,
) -> Result<TestHandler, DynError> {
    spawn_workflow_actor_in(
        Arc::new(TempTedgeDir::new()),
        device_topic_id,
        workflows,
        entity_store,
    )
    .await
}

async fn spawn_workflow_actor_in(
    tmp_dir: Arc<TempTedgeDir>,
    device_topic_id: &str,
    workflows: Vec<(String, String)>,
    entity_store: FakeEntityStore,
) -> Result<TestHandler, DynError> {
    let mut software_builder = SoftwareActor(SimpleMessageBoxBuilder::new("Software", 5));
    let mut restart_builder = RestartActor(SimpleMessageBoxBuilder::new("Restart", 5));
    let mut config_builder = ConfigActorBuilder(SimpleMessageBoxBuilder::new("Config", 5));
    let sync_listener_builder =
        SyncListenerActorBuilder(SimpleMessageBoxBuilder::new("SyncListener", 5));

    let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 32);
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
    let mut uploader_builder: SimpleMessageBoxBuilder<
        RequestEnvelope<UploaderRequest, UploaderResult>,
        NoMessage,
    > = SimpleMessageBoxBuilder::new("Uploader", 5);
    let mut http_builder = FakeServerBox::<HttpRequest, HttpResult>::builder();

    let tmp_path = Utf8Path::from_path(tmp_dir.path()).unwrap();
    let config_root = TedgePaths::from_root_with_defaults(tmp_path, "", "");
    let operations_dir = tmp_dir.dir("operations");

    tmp_dir.dir("running-operations");
    for (file_name, content) in workflows {
        operations_dir.file(&file_name).with_raw_content(&content);
    }
    let device_topic_id = device_topic_id
        .parse::<EntityTopicId>()
        .expect("Invalid topic id");

    let service_topic_id = device_topic_id
        .default_service_for_device("tedge-agent")
        .unwrap_or_else(|| {
            let (prefix, _) = device_topic_id.as_str().rsplit_once('/').unwrap();
            format!("{prefix}/tedge-agent")
                .parse()
                .expect("Invalid service topic id")
        });
    let log_dir = TedgePaths::from_root_with_defaults(tmp_path, "", "").root_dir();

    let config = OperationConfig {
        mqtt_schema: MqttSchema::new(),
        device_topic_id,
        service_topic_id,
        log_dir: OperationLogs::new(log_dir),
        config_dir: config_root.clone(),
        state_dir: TedgePaths::from_root_with_defaults(tmp_path.join("running-operations"), "", ""),
        operations_dir: config_root.dir("operations").unwrap(),
        tmp_dir: TedgePaths::from_root_with_defaults(tmp_path.join(tmp_path), "", ""),
        capabilities: Capabilities::default(),
        entities_url: "http://127.0.0.1:8000/te/v1/entities".into(),
    };
    let mut workflow_actor_builder = WorkflowActorBuilder::new(
        config,
        &mut mqtt_builder,
        &mut script_builder,
        &mut inotify_builder,
        &mut downloade_builder,
        &mut uploader_builder,
        &mut http_builder,
    );
    workflow_actor_builder.register_builtin_operation(&mut restart_builder);
    workflow_actor_builder.register_builtin_operation(&mut software_builder);
    workflow_actor_builder.register_builtin_operation_step_handler(&mut config_builder);
    workflow_actor_builder
        .register_sync_signal_sink(OperationType::ConfigUpdate, &sync_listener_builder);

    let config_box = config_builder.0.build().with_timeout(TEST_TIMEOUT_MS);
    let sync_signal_box = sync_listener_builder
        .0
        .build()
        .with_timeout(TEST_TIMEOUT_MS);
    let software_box = software_builder.0.build().with_timeout(TEST_TIMEOUT_MS);
    let restart_box = restart_builder.0.build().with_timeout(TEST_TIMEOUT_MS);
    let mqtt_box = mqtt_builder.build().with_timeout(TEST_TIMEOUT_MS);
    let script_box = script_builder.build().with_timeout(TEST_TIMEOUT_MS);
    let downloader_box = downloade_builder.build().with_timeout(TEST_TIMEOUT_MS);
    let uploader_box = uploader_builder.build().with_timeout(TEST_TIMEOUT_MS);
    let inotify_box = inotify_builder.build().with_timeout(TEST_TIMEOUT_MS);

    tokio::spawn(entity_store.serve(http_builder.build()));

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
        inotify_box,
        downloader_box,
        uploader_box,
        config_box,
        sync_signal_box,
        script_box,
    })
}

fn service_of(service: &str, device: &str) -> EntityMetadata {
    EntityMetadata::new(service.parse().unwrap(), EntityType::Service)
        .with_parent(device.parse().unwrap())
}

const SHIPPED_RESTART_WORKFLOW: &str = include_str!("../resources/service_restart.toml");

const SERVICE_RESTART_WORKFLOW: &str = r#"
operation = "restart"
type = "service"

[init]
action = "proceed"
on_success = "executing"

[executing]
action = "proceed"
on_success = "successful"

[successful]
action = "cleanup"
"#;

const DEVICE_PAUSE_WORKFLOW: &str = r#"
operation = "pause"

[init]
action = "proceed"
on_success = "successful"

[successful]
action = "cleanup"
"#;

/// Rewrite a workflow file, as an administrator adapting a definition does
fn edit_workflow(tmp_dir: &TempTedgeDir, file_name: &str, definition: &str) -> std::path::PathBuf {
    let path = tmp_dir.path().join("operations").join(file_name);
    std::fs::write(&path, format!("# edited\n{definition}")).expect("the workflow file is written");
    path
}

/// Skip the only capability message published on start when no software type is declared
async fn skip_device_restart_capability(
    mqtt: &mut impl MessageReceiver<MqttMessage>,
    device: &str,
) {
    assert_received_contains_str(mqtt, [(format!("te/{device}/cmd/restart").as_str(), "{}")]).await;
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
        vec![
            (OperationType::ConfigUpdate, OperationStep::from("set")),
            (OperationType::ConfigSnapshot, OperationStep::from("get")),
        ]
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

// A fake actor that listens for sync signals emitted on the completion of `config_update`
// operations, standing in for actors such as the log manager which re-scan their supported
// types on config/software updates.
struct SyncListenerActorBuilder(SimpleMessageBoxBuilder<CmdMetaSyncSignal, NoMessage>);

impl MessageSink<CmdMetaSyncSignal> for SyncListenerActorBuilder {
    fn get_sender(&self) -> DynSender<CmdMetaSyncSignal> {
        self.0.get_sender()
    }
}

impl SyncOnCommand for SyncListenerActorBuilder {
    fn sync_on_commands(&self) -> Vec<OperationType> {
        vec![OperationType::ConfigUpdate]
    }
}
