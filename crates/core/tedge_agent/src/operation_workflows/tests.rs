use crate::operation_workflows::builder::DownloaderRequest;
use crate::operation_workflows::builder::DownloaderResult;
use crate::operation_workflows::builder::WorkflowActorBuilder;
use crate::operation_workflows::config::OperationConfig;
use crate::software_manager::actor::SoftwareCommand;
use camino::Utf8Path;
use serde_json::json;
use std::process::Output;
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
use tedge_api::RestartCommand;
use tedge_api::SoftwareUpdateCommand;
use tedge_file_system_ext::FsWatchEvent;
use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_script_ext::Execute;
use tempfile::TempDir;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(3000);

#[tokio::test]
async fn convert_incoming_software_list_request() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let TestHandler {
        tmp_dir,
        mut software_box,
        mut mqtt_box,
        ..
    } = spawn_mqtt_operation_converter("device/main//").await?;

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
    } = spawn_mqtt_operation_converter("device/child001//").await?;

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
    } = spawn_mqtt_operation_converter(target_device).await?;

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
    } = spawn_mqtt_operation_converter("device/main//").await?;

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
    } = spawn_mqtt_operation_converter("device/child//").await?;

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

#[tokio::test]
async fn convert_outgoing_software_update_response() -> Result<(), DynError> {
    // Spawn outgoing mqtt message converter
    let TestHandler {
        mut software_box,
        mut mqtt_box,
        ..
    } = spawn_mqtt_operation_converter("device/main//").await?;

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
    } = spawn_mqtt_operation_converter("device/main//").await?;

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

struct TestHandler {
    tmp_dir: TempDir,
    mqtt_box: TimedMessageBox<SimpleMessageBox<MqttMessage, MqttMessage>>,
    software_box: TimedMessageBox<SimpleMessageBox<SoftwareCommand, SoftwareCommand>>,
    restart_box: TimedMessageBox<SimpleMessageBox<RestartCommand, RestartCommand>>,
    _downloader_box: TimedMessageBox<
        SimpleMessageBox<RequestEnvelope<DownloaderRequest, DownloaderResult>, NoMessage>,
    >,
}

async fn spawn_mqtt_operation_converter(device_topic_id: &str) -> Result<TestHandler, DynError> {
    let mut software_builder = SoftwareActor(SimpleMessageBoxBuilder::new("Software", 5));
    let mut restart_builder = RestartActor(SimpleMessageBoxBuilder::new("Restart", 5));
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

    let tmp_dir = tempfile::TempDir::new().unwrap();
    let tmp_path = Utf8Path::from_path(tmp_dir.path()).unwrap();
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
        operations_dir: tmp_path.join("operations"),
        tmp_dir: tmp_path.into(),
    };
    let mut converter_actor_builder = WorkflowActorBuilder::new(
        config,
        &mut mqtt_builder,
        &mut script_builder,
        &mut inotify_builder,
        &mut downloade_builder,
    );
    converter_actor_builder.register_builtin_operation(&mut restart_builder);
    converter_actor_builder.register_builtin_operation(&mut software_builder);

    let software_box = software_builder.0.build().with_timeout(TEST_TIMEOUT_MS);
    let restart_box = restart_builder.0.build().with_timeout(TEST_TIMEOUT_MS);
    let mqtt_box = mqtt_builder.build().with_timeout(TEST_TIMEOUT_MS);
    let _downloader_box = downloade_builder.build().with_timeout(TEST_TIMEOUT_MS);

    let converter_actor = converter_actor_builder.build();
    tokio::spawn(async move { converter_actor.run().await });

    Ok(TestHandler {
        tmp_dir,
        mqtt_box,
        software_box,
        restart_box,
        _downloader_box,
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
