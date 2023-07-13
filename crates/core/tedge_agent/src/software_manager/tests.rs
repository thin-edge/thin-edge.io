use crate::software_manager::actor::SoftwareRequest;
use crate::software_manager::actor::SoftwareResponse;
use crate::software_manager::builder::SoftwareManagerBuilder;
use crate::software_manager::config::SoftwareManagerConfig;
use std::time::Duration;
use tedge_actors::test_helpers::MessageReceiverExt;
use tedge_actors::test_helpers::TimedMessageBox;
use tedge_actors::Actor;
use tedge_actors::Builder;
use tedge_actors::DynError;
use tedge_actors::MessageReceiver;
use tedge_actors::Sender;
use tedge_actors::ServiceConsumer;
use tedge_actors::SimpleMessageBox;
use tedge_actors::SimpleMessageBoxBuilder;
use tedge_api::messages::SoftwareModuleAction;
use tedge_api::messages::SoftwareModuleItem;
use tedge_api::messages::SoftwareRequestResponseSoftwareList;
use tedge_api::OperationStatus;
use tedge_api::SoftwareListRequest;
use tedge_api::SoftwareListResponse;
use tedge_api::SoftwareRequestResponse;
use tedge_api::SoftwareUpdateRequest;
use tedge_api::SoftwareUpdateResponse;
use tedge_config::TEdgeConfigLocation;
use tedge_test_utils::fs::TempTedgeDir;

const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

#[tokio::test]
async fn test_pending_software_update_operation() -> Result<(), DynError> {
    let temp_dir = TempTedgeDir::new();
    let content = "operation_id = \'1234\'\noperation = \"update\"";
    temp_dir
        .dir(".agent")
        .file("software-current-operation")
        .with_raw_content(content);

    let mut converter_box = spawn_software_manager(&temp_dir).await?;

    let software_request_response = SoftwareRequestResponse::new("1234", OperationStatus::Failed);
    converter_box
        .assert_received([SoftwareUpdateResponse {
            response: software_request_response,
        }])
        .await;

    Ok(())
}

#[tokio::test]
async fn test_new_software_update_operation() -> Result<(), DynError> {
    let temp_dir = TempTedgeDir::new();
    temp_dir.dir(".agent");

    let mut converter_box = spawn_software_manager(&temp_dir).await?;

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

    converter_box
        .send(
            SoftwareUpdateRequest {
                id: "random".to_string(),
                update_list: vec![debian_list],
            }
            .into(),
        )
        .await?;

    match converter_box.recv().await.unwrap() {
        SoftwareResponse::SoftwareUpdateResponse(res) => {
            assert_eq!(res.response.status, OperationStatus::Executing);
        }
        SoftwareResponse::SoftwareListResponse(_) => {
            panic!("Received SoftwareListResponse")
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_pending_software_list_operation() -> Result<(), DynError> {
    let temp_dir = TempTedgeDir::new();
    let content = "operation_id = \'1234\'\noperation = \"list\"";
    temp_dir
        .dir(".agent")
        .file("software-current-operation")
        .with_raw_content(content);

    let mut converter_box = spawn_software_manager(&temp_dir).await?;

    let software_request_response = SoftwareRequestResponse::new("1234", OperationStatus::Failed);
    converter_box
        .assert_received([SoftwareListResponse {
            response: software_request_response,
        }])
        .await;

    Ok(())
}

#[tokio::test]
// testing that tedge-agent returns an empty software list when there is no sm plugin
async fn test_new_software_list_operation() -> Result<(), DynError> {
    let temp_dir = TempTedgeDir::new();
    temp_dir.dir(".agent");

    let mut converter_box = spawn_software_manager(&temp_dir).await?;

    converter_box
        .send(
            SoftwareListRequest {
                id: "1234".to_string(),
            }
            .into(),
        )
        .await?;

    let executing_response = SoftwareRequestResponse::new("1234", OperationStatus::Executing);
    let mut successful_response = SoftwareRequestResponse::new("1234", OperationStatus::Successful);
    successful_response.add_modules("".to_string(), vec![]);

    converter_box
        .assert_received([
            SoftwareListResponse {
                response: executing_response,
            },
            SoftwareListResponse {
                response: successful_response,
            },
        ])
        .await;

    Ok(())
}

async fn spawn_software_manager(
    tmp_dir: &TempTedgeDir,
) -> Result<TimedMessageBox<SimpleMessageBox<SoftwareResponse, SoftwareRequest>>, DynError> {
    let mut converter_builder: SimpleMessageBoxBuilder<SoftwareResponse, SoftwareRequest> =
        SimpleMessageBoxBuilder::new("Converter", 5);

    let config = SoftwareManagerConfig::new(
        &tmp_dir.utf8_path_buf(),
        &tmp_dir.utf8_path_buf(),
        &tmp_dir.utf8_path_buf(),
        &tmp_dir.utf8_path_buf(),
        None,
        &TEdgeConfigLocation::from_custom_root(tmp_dir.utf8_path_buf()),
    );

    let mut software_actor_builder = SoftwareManagerBuilder::new(config);
    converter_builder.set_connection(&mut software_actor_builder);

    let converter_box = converter_builder.build().with_timeout(TEST_TIMEOUT_MS);

    let mut software_actor = software_actor_builder.build();
    tokio::spawn(async move { software_actor.run().await });

    Ok(converter_box)
}
