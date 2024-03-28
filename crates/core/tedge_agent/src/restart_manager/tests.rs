use crate::restart_manager::builder::RestartManagerBuilder;
use crate::restart_manager::config::RestartManagerConfig;
use serde_json::json;
use std::time::Duration;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::test_helpers::TimedMessageBox;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynError;
use tedge_actors::MessageReceiver;
use tedge_actors::MessageSource;
use tedge_actors::NoConfig;
use tedge_actors::Sender;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::messages::CommandStatus;
use tedge_api::messages::RestartCommandPayload;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::RestartCommand;
use tedge_config::SudoCommandBuilder;
use tedge_test_utils::fs::TempTedgeDir;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

#[tokio::test]
async fn test_pending_restart_operation() -> Result<(), DynError> {
    let temp_dir = TempTedgeDir::new();
    let content = json!({
            "target": "device/main//",
            "cmd_id": "1234",
            "payload": {
                "status": "executing",
            }
    });
    temp_dir
        .dir(".agent")
        .file("restart-current-operation")
        .with_raw_content(&content.to_string());

    let mut converter_box = spawn_restart_manager(&temp_dir).await?;

    converter_box
        .assert_received([RestartCommand {
            target: EntityTopicId::default_main_device(),
            cmd_id: "1234".to_string(),
            payload: RestartCommandPayload::new(CommandStatus::Successful),
        }])
        .await;

    Ok(())
}

#[tokio::test]
async fn test_pending_restart_operation_failed() -> Result<(), DynError> {
    let temp_dir = TempTedgeDir::new();
    let content = json!({
            "target": "device/main//",
            "cmd_id": "1234",
            "payload": {
                "status": "scheduled",
            }
    });
    temp_dir
        .dir(".agent")
        .file("restart-current-operation")
        .with_raw_content(&content.to_string());

    let mut converter_box = spawn_restart_manager(&temp_dir).await?;

    converter_box
        .assert_received([RestartCommand {
            target: EntityTopicId::default_main_device(),
            cmd_id: "1234".to_string(),
            payload: RestartCommandPayload::new(CommandStatus::Failed {
                reason: "The agent has been restarted but not the device".to_string(),
            }),
        }])
        .await;

    Ok(())
}

#[tokio::test]
async fn test_new_restart_operation() -> Result<(), DynError> {
    let temp_dir = TempTedgeDir::new();
    temp_dir.dir(".agent").file("restart-current-operation");

    // Spawn restart manager
    let mut converter_box = spawn_restart_manager(&temp_dir).await?;

    // Simulate RestartOperationRequest
    converter_box
        .send(RestartCommand {
            target: EntityTopicId::default_main_device(),
            cmd_id: "1234".to_string(),
            payload: RestartCommandPayload::new(CommandStatus::Scheduled),
        })
        .await?;

    let status = converter_box.recv().await.unwrap().status();
    assert_eq!(status, CommandStatus::Executing);

    // Check the agent restart temp file is created
    assert!(temp_dir.path().join("tedge_agent_restart").exists());

    Ok(())
}

async fn spawn_restart_manager(
    tmp_dir: &TempTedgeDir,
) -> Result<TimedMessageBox<SimpleMessageBox<RestartCommand, RestartCommand>>, DynError> {
    let mut converter_builder: SimpleMessageBoxBuilder<RestartCommand, RestartCommand> =
        SimpleMessageBoxBuilder::new("Converter", 5);

    let config = RestartManagerConfig {
        device_topic_id: EntityTopicId::default_main_device(),
        tmp_dir: tmp_dir.utf8_path_buf(),
        config_dir: tmp_dir.utf8_path_buf(),
        state_dir: "/some/unknown/dir".into(),
        sudo: SudoCommandBuilder::enabled(true),
    };

    let mut restart_actor_builder = RestartManagerBuilder::new(config);
    converter_builder.connect_sink(NoConfig, &restart_actor_builder);
    restart_actor_builder.connect_sink(NoConfig, &converter_builder);

    let converter_box = converter_builder.build().with_timeout(TEST_TIMEOUT_MS);

    let restart_actor = restart_actor_builder.build();
    tokio::spawn(async move { restart_actor.run().await });

    Ok(converter_box)
}
