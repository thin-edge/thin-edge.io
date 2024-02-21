use crate::software_manager::actor::SoftwareCommand;
use crate::tedge_operation_converter::builder::TedgeOperationConverterBuilder;
use crate::tedge_operation_converter::config::OperationConfig;
use camino::Utf8Path;
use serde_json::json;
use std::process::Output;
use std::time::Duration;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::test_helpers::TimedMessageBox;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynError;
use tedge_actors::MessageReceiver;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::messages::CommandStatus;
use tedge_api::messages::RestartCommandPayload;
use tedge_api::messages::SoftwareCommandMetadata;
use tedge_api::messages::SoftwareListCommand;
use tedge_api::messages::SoftwareModuleAction;
use tedge_api::messages::SoftwareModuleItem;
use tedge_api::messages::SoftwareRequestResponseSoftwareList;
use tedge_api::messages::SoftwareUpdateCommandPayload;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::mqtt_topics::MqttSchema;
use tedge_api::workflow::WorkflowSupervisor;
use tedge_api::RestartCommand;
use tedge_api::SoftwareUpdateCommand;
use tedge_mqtt_ext::test_helpers::assert_received_contains_str;
use tedge_mqtt_ext::MqttMessage;
use tedge_mqtt_ext::Topic;
use tedge_script_ext::Execute;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

#[tokio::test]
async fn convert_incoming_software_list_request() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let (mut software_box, _restart_box, mut mqtt_box) =
        spawn_mqtt_operation_converter("device/main//").await?;

    // Simulate SoftwareList MQTT message received.
    let mqtt_message = MqttMessage::new(
        &Topic::new_unchecked("te/device/main///cmd/software_list/some-cmd-id"),
        r#"{ "status": "init" }"#,
    );
    mqtt_box.send(mqtt_message).await?;

    // Assert SoftwareListCommand
    software_box
        .assert_received([SoftwareListCommand::new(
            &EntityTopicId::default_main_device(),
            "some-cmd-id".to_string(),
        )
        .with_status(CommandStatus::Scheduled)])
        .await;
    Ok(())
}

#[tokio::test]
async fn convert_incoming_software_update_request() -> Result<(), DynError> {
    // Spawn incoming mqtt message converter
    let (mut software_box, _restart_box, mut mqtt_box) =
        spawn_mqtt_operation_converter("device/child001//").await?;

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
            },
        }])
        .await;

    Ok(())
}

#[tokio::test]
async fn convert_incoming_restart_request() -> Result<(), DynError> {
    let target_device = "device/child-foo//";

    // Spawn incoming mqtt message converter
    let (_software_box, mut restart_box, mut mqtt_box) =
        spawn_mqtt_operation_converter(target_device).await?;

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
            payload: RestartCommandPayload::new(CommandStatus::Scheduled),
        }])
        .await;

    Ok(())
}

#[tokio::test]
async fn convert_outgoing_software_list_response() -> Result<(), DynError> {
    // Spawn outgoing mqtt message converter
    let (mut software_box, _restart_box, mut mqtt_box) =
        spawn_mqtt_operation_converter("device/main//").await?;

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
        .with_status(CommandStatus::Executing);
    software_box.send(software_list_response.into()).await?;

    mqtt_box
        .assert_received([MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/software_list/1234"),
            r#"{"status":"executing"}"#,
        )
        .with_retain()])
        .await;

    Ok(())
}

#[tokio::test]
async fn publish_capabilities_on_start() -> Result<(), DynError> {
    // Spawn outgoing mqtt message converter
    let (mut software_box, _restart_box, mut mqtt_box) =
        spawn_mqtt_operation_converter("device/child//").await?;

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
    let (mut software_box, _restart_box, mut mqtt_box) =
        spawn_mqtt_operation_converter("device/main//").await?;

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
    let software_update_response = software_update_request.with_status(CommandStatus::Executing);
    software_box.send(software_update_response.into()).await?;

    mqtt_box
        .assert_received([MqttMessage::new(
            &Topic::new_unchecked("te/device/main///cmd/software_update/1234"),
            r#"{"status":"executing"}"#,
        )
        .with_retain()])
        .await;

    Ok(())
}

#[tokio::test]
async fn convert_outgoing_restart_response() -> Result<(), DynError> {
    // Spawn outgoing mqtt message converter
    let (mut software_box, mut restart_box, mut mqtt_box) =
        spawn_mqtt_operation_converter("device/main//").await?;

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
        payload: RestartCommandPayload::new(CommandStatus::Executing),
    };
    restart_box.send(executing_response).await?;

    let (topic, payload) = mqtt_box
        .recv()
        .await
        .map(|msg| (msg.topic, msg.payload))
        .expect("MqttMessage");
    assert_eq!(topic.name, "te/device/main///cmd/restart/abc");
    assert!(format!("{:?}", payload).contains(r#"status":"executing"#));

    Ok(())
}

async fn spawn_mqtt_operation_converter(
    device_topic_id: &str,
) -> Result<
    (
        TimedMessageBox<SimpleMessageBox<SoftwareCommand, SoftwareCommand>>,
        TimedMessageBox<SimpleMessageBox<RestartCommand, RestartCommand>>,
        TimedMessageBox<SimpleMessageBox<MqttMessage, MqttMessage>>,
    ),
    DynError,
> {
    let mut software_builder: SimpleMessageBoxBuilder<SoftwareCommand, SoftwareCommand> =
        SimpleMessageBoxBuilder::new("Software", 5);
    let mut restart_builder: SimpleMessageBoxBuilder<RestartCommand, RestartCommand> =
        SimpleMessageBoxBuilder::new("Restart", 5);
    let mut mqtt_builder: SimpleMessageBoxBuilder<MqttMessage, MqttMessage> =
        SimpleMessageBoxBuilder::new("MQTT", 5);
    let mut script_builder: SimpleMessageBoxBuilder<Execute, std::io::Result<Output>> =
        SimpleMessageBoxBuilder::new("Script", 5);

    let workflows = WorkflowSupervisor::default();

    let tmp_dir = tempfile::TempDir::new().unwrap();
    let tmp_path = Utf8Path::from_path(tmp_dir.path()).unwrap();
    let config = OperationConfig {
        mqtt_schema: MqttSchema::new(),
        device_topic_id: device_topic_id.parse().expect("Invalid topic id"),
        log_dir: tmp_path.into(),
        config_dir: tmp_path.into(),
        state_dir: tmp_path.into(),
    };
    let converter_actor_builder = TedgeOperationConverterBuilder::new(
        config,
        workflows,
        &mut software_builder,
        &mut restart_builder,
        &mut mqtt_builder,
        &mut script_builder,
    );

    let software_box = software_builder.build().with_timeout(TEST_TIMEOUT_MS);
    let restart_box = restart_builder.build().with_timeout(TEST_TIMEOUT_MS);
    let mqtt_message_box = mqtt_builder.build().with_timeout(TEST_TIMEOUT_MS);

    let converter_actor = converter_actor_builder.build();
    tokio::spawn(async move { converter_actor.run().await });

    Ok((software_box, restart_box, mqtt_message_box))
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
