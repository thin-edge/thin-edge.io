use crate::software_manager::actor::SoftwareCommand;
use crate::software_manager::builder::SoftwareManagerBuilder;
use crate::software_manager::config::SoftwareManagerConfig;
use serde_json::json;
use std::time::Duration;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::test_helpers::TimedMessageBox;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynError;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSink;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::messages::CommandStatus;
use tedge_api::messages::SoftwareCommandMetadata;
use tedge_api::messages::SoftwareListCommand;
use tedge_api::messages::SoftwareModuleAction;
use tedge_api::messages::SoftwareModuleItem;
use tedge_api::messages::SoftwareRequestResponseSoftwareList;
use tedge_api::messages::SoftwareUpdateCommand;
use tedge_api::messages::SoftwareUpdateCommandPayload;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_config::TEdgeConfigLocation;
use tedge_test_utils::fs::TempTedgeDir;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

#[tokio::test]
async fn test_pending_software_update_operation() -> Result<(), DynError> {
    let temp_dir = TempTedgeDir::new();
    let content = json!({
        "SoftwareUpdateCommand": {
            "target": "device/main//",
            "cmd_id": "1234",
            "payload": {
                "status": "scheduled",
            }
    }});
    temp_dir
        .dir(".agent")
        .file("software-current-operation")
        .with_raw_content(&content.to_string());

    let mut converter_box = spawn_software_manager(&temp_dir).await?;

    let software_request_response = SoftwareUpdateCommand {
        target: EntityTopicId::default_main_device(),
        cmd_id: "1234".to_string(),
        payload: SoftwareUpdateCommandPayload::default(),
    }
    .with_error("Software Update command cancelled due to unexpected agent restart".to_string());
    converter_box
        .assert_received([software_request_response])
        .await;

    Ok(())
}

#[tokio::test]
async fn test_new_software_update_operation() -> Result<(), DynError> {
    let temp_dir = TempTedgeDir::new();
    temp_dir.dir(".agent");
    temp_dir.file("apt");
    temp_dir.file("docker");

    let mut converter_box = spawn_software_manager(&temp_dir).await?;

    // software cmd metadata internal message
    converter_box
        .assert_received([SoftwareCommandMetadata {
            types: vec!["apt".into(), "docker".into()],
        }])
        .await;

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

    let command = SoftwareUpdateCommand {
        target: EntityTopicId::default_main_device(),
        cmd_id: "random".to_string(),
        payload: SoftwareUpdateCommandPayload {
            status: CommandStatus::Scheduled,
            update_list: vec![debian_list],
            failures: vec![],
        },
    };
    converter_box.send(command.into()).await?;

    match converter_box.recv().await.unwrap() {
        SoftwareCommand::SoftwareUpdateCommand(res) => {
            assert_eq!(res.cmd_id, "random");
            assert_eq!(res.status(), CommandStatus::Executing);
        }
        SoftwareCommand::SoftwareListCommand(_) => {
            panic!("Received SoftwareListCommand")
        }
        SoftwareCommand::SoftwareCommandMetadata(_) => {
            panic!("Received SoftwareCommandMetadata")
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_pending_software_list_operation() -> Result<(), DynError> {
    let temp_dir = TempTedgeDir::new();
    let content = json!({
        "SoftwareListCommand": {
            "target": "device/main//",
            "cmd_id": "1234",
            "payload": {
                "status": "scheduled",
            }
    }});
    temp_dir
        .dir(".agent")
        .file("software-current-operation")
        .with_raw_content(&content.to_string());

    let mut converter_box = spawn_software_manager(&temp_dir).await?;

    let software_request_response =
        SoftwareListCommand::new(&EntityTopicId::default_main_device(), "1234".to_string())
            .with_error(
                "Software List request cancelled due to unexpected agent restart".to_string(),
            );
    converter_box
        .assert_received([software_request_response])
        .await;

    Ok(())
}

#[tokio::test]
// testing that tedge-agent returns an empty software list when there is no sm plugin
async fn test_new_software_list_operation() -> Result<(), DynError> {
    let temp_dir = TempTedgeDir::new();
    temp_dir.dir(".agent");

    let mut converter_box = spawn_software_manager(&temp_dir).await?;

    let command =
        SoftwareListCommand::new(&EntityTopicId::default_main_device(), "1234".to_string())
            .with_status(CommandStatus::Scheduled);
    converter_box.send(command.clone().into()).await?;

    let software_metadata = SoftwareCommandMetadata { types: vec![] };
    let executing_response = command.clone().with_status(CommandStatus::Executing);
    let mut successful_response = command.clone().with_status(CommandStatus::Successful);
    successful_response.add_modules("".to_string(), vec![]);

    converter_box.assert_received([software_metadata]).await;
    converter_box
        .assert_received([executing_response, successful_response])
        .await;

    Ok(())
}

async fn spawn_software_manager(
    tmp_dir: &TempTedgeDir,
) -> Result<TimedMessageBox<SimpleMessageBox<SoftwareCommand, SoftwareCommand>>, DynError> {
    let mut converter_builder: SimpleMessageBoxBuilder<SoftwareCommand, SoftwareCommand> =
        SimpleMessageBoxBuilder::new("Converter", 5);

    let config = SoftwareManagerConfig {
        device: EntityTopicId::default_main_device(),
        tmp_dir: tmp_dir.utf8_path_buf(),
        config_dir: tmp_dir.utf8_path_buf(),
        state_dir: "/some/unknown/dir".into(),
        sm_plugins_dir: tmp_dir.utf8_path_buf(),
        log_dir: tmp_dir.utf8_path_buf(),
        default_plugin_type: None,
        config_location: TEdgeConfigLocation::from_custom_root(tmp_dir.utf8_path_buf()),
    };

    let mut software_actor_builder = SoftwareManagerBuilder::new(config);
    converter_builder.register_peer(NoConfig, software_actor_builder.get_sender());
    software_actor_builder.register_peer(NoConfig, converter_builder.get_sender());

    let converter_box = converter_builder.build().with_timeout(TEST_TIMEOUT_MS);

    let software_actor = software_actor_builder.build();
    tokio::spawn(async move { software_actor.run().await });

    Ok(converter_box)
}
